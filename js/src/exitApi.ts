import {
  LOW_LATENCY_RETRY_POLICY,
  retryAsync,
  type RetryPolicy,
} from "./retry.js";
import type { MarketContextMsg } from "./stream/proto.js";

const ERROR_BODY_SNIPPET_LEN = 220;

export const EXIT_API_BASE_URL = "https://api.lasersell.io";
export const LOCAL_EXIT_API_BASE_URL = "http://localhost:8080";

export const EXIT_API_DEFAULTS = {
  connect_timeout_ms: 200,
  attempt_timeout_ms: 900,
  max_attempts: 2,
  backoff_ms: 25,
  jitter_ms: 25,
} as const;

export type SellOutput = "SOL" | "USD1";

export interface BuildSellTxRequest {
  mint: string;
  user_pubkey: string;
  amount_tokens: number;
  slippage_bps?: number;
  mode?: string;
  output?: SellOutput;
  referral_id?: string;
  market_context?: MarketContextMsg;
}

export interface BuildBuyTxRequest {
  mint: string;
  user_pubkey: string;
  amount_quote_units: number;
  slippage_bps?: number;
  mode?: string;
  referral_id?: string;
}

export interface BuildTxResponse {
  tx: string;
  route?: unknown;
  debug?: unknown;
}

export interface ExitApiClientOptions {
  // Kept for parity with the Rust SDK. Fetch does not expose a strict
  // connection-only timeout across environments.
  connect_timeout_ms: number;
  attempt_timeout_ms: number;
  retry_policy: RetryPolicy;
  fetch_impl: FetchLike;
}

export type ExitApiErrorKind =
  | "transport"
  | "http_status"
  | "envelope_status"
  | "parse";

export class ExitApiError extends Error {
  readonly kind: ExitApiErrorKind;
  readonly status?: number;
  readonly body?: string;
  readonly detail?: string;

  private constructor(
    kind: ExitApiErrorKind,
    message: string,
    options?: {
      cause?: unknown;
      status?: number;
      body?: string;
      detail?: string;
    },
  ) {
    super(message, { cause: options?.cause });
    this.name = "ExitApiError";
    this.kind = kind;
    this.status = options?.status;
    this.body = options?.body;
    this.detail = options?.detail;
  }

  static transport(cause: unknown): ExitApiError {
    return new ExitApiError(
      "transport",
      `request failed: ${stringifyError(cause)}`,
      { cause },
    );
  }

  static httpStatus(status: number, body: string): ExitApiError {
    return new ExitApiError("http_status", `http status ${status}: ${body}`, {
      status,
      body,
    });
  }

  static envelopeStatus(status: string, detail: string): ExitApiError {
    return new ExitApiError(
      "envelope_status",
      `exit-api status ${status}: ${detail}`,
      { detail },
    );
  }

  static parse(message: string, cause?: unknown): ExitApiError {
    return new ExitApiError("parse", `failed to parse response: ${message}`, {
      cause,
    });
  }

  isRetryable(): boolean {
    if (this.kind === "transport") {
      return true;
    }

    if (this.kind === "http_status") {
      return (
        (this.status !== undefined && this.status >= 500) || this.status === 429
      );
    }

    return false;
  }
}

export class ExitApiClient {
  private readonly apiKey?: string;
  private readonly connectTimeoutMs: number;
  private readonly attemptTimeoutMs: number;
  private readonly retryPolicy: RetryPolicy;
  private readonly fetchImpl: FetchLike;
  private local = false;

  constructor(
    apiKey?: string,
    options: Partial<ExitApiClientOptions> = {},
  ) {
    this.apiKey = apiKey;
    this.connectTimeoutMs =
      options.connect_timeout_ms ?? EXIT_API_DEFAULTS.connect_timeout_ms;
    this.attemptTimeoutMs =
      options.attempt_timeout_ms ?? EXIT_API_DEFAULTS.attempt_timeout_ms;
    this.retryPolicy = options.retry_policy
      ? { ...options.retry_policy }
      : {
          max_attempts: EXIT_API_DEFAULTS.max_attempts,
          initial_backoff_ms: EXIT_API_DEFAULTS.backoff_ms,
          max_backoff_ms: EXIT_API_DEFAULTS.backoff_ms,
          jitter_ms: EXIT_API_DEFAULTS.jitter_ms,
        };
    this.fetchImpl = options.fetch_impl ?? globalThis.fetch;

    if (typeof this.fetchImpl !== "function") {
      throw new Error(
        "No fetch implementation available. Provide options.fetch_impl in environments without global fetch.",
      );
    }
  }

  static withApiKey(apiKey: string): ExitApiClient {
    return new ExitApiClient(apiKey);
  }

  static withOptions(
    apiKey: string | undefined,
    options: Partial<ExitApiClientOptions>,
  ): ExitApiClient {
    return new ExitApiClient(apiKey, options);
  }

  withLocalMode(local: boolean): this {
    this.local = local;
    return this;
  }

  async buildSellTx(request: BuildSellTxRequest): Promise<BuildTxResponse> {
    return await this.buildTx("/v1/sell", request);
  }

  async buildSellTxB64(request: BuildSellTxRequest): Promise<string> {
    const response = await this.buildSellTx(request);
    return response.tx;
  }

  async buildBuyTx(request: BuildBuyTxRequest): Promise<BuildTxResponse> {
    return await this.buildTx("/v1/buy", request);
  }

  private async buildTx<T extends object>(
    path: string,
    request: T,
  ): Promise<BuildTxResponse> {
    const endpoint = this.endpoint(path);
    return await retryAsync(
      this.retryPolicy,
      async () => await this.sendAttempt(endpoint, request),
      (error: unknown) =>
        error instanceof ExitApiError && error.isRetryable(),
    );
  }

  private endpoint(path: string): string {
    return `${this.baseUrl()}${path}`;
  }

  private baseUrl(): string {
    if (this.local) {
      return LOCAL_EXIT_API_BASE_URL;
    }
    return EXIT_API_BASE_URL;
  }

  private async sendAttempt<T extends object>(
    endpoint: string,
    request: T,
  ): Promise<BuildTxResponse> {
    const timeoutMs = Math.max(this.connectTimeoutMs, this.attemptTimeoutMs);
    const controller = new AbortController();
    const timer = setTimeout(() => {
      controller.abort();
    }, timeoutMs);

    try {
      const headers: Record<string, string> = {
        "content-type": "application/json",
      };

      if (this.apiKey !== undefined) {
        headers["x-api-key"] = this.apiKey;
      }

      const response = await this.fetchImpl(endpoint, {
        method: "POST",
        headers,
        body: JSON.stringify(request),
        signal: controller.signal,
      });

      const body = await response.text();

      if (!response.ok) {
        throw ExitApiError.httpStatus(
          response.status,
          summarizeErrorBody(body),
        );
      }

      return parseBuildTxResponse(body);
    } catch (error) {
      if (error instanceof ExitApiError) {
        throw error;
      }

      throw ExitApiError.transport(error);
    } finally {
      clearTimeout(timer);
    }
  }
}

export function parseBuildTxResponse(body: string): BuildTxResponse {
  let parsed: unknown;
  try {
    parsed = JSON.parse(body);
  } catch (error) {
    throw ExitApiError.parse("response was not valid JSON", error);
  }

  const obj = asRecord(parsed, "response");

  const taggedStatus = asOptionalString(obj.status);
  if (taggedStatus !== undefined) {
    if (taggedStatus.toLowerCase() === "ok") {
      const tx = asOptionalString(obj.tx) ?? asOptionalString(obj.unsigned_tx_b64);
      if (tx === undefined) {
        throw ExitApiError.parse("status=ok payload missing tx");
      }
      return {
        tx,
        route: obj.route,
        debug: obj.debug,
      };
    }

    const detail =
      asOptionalString(obj.reason) ??
      asOptionalString(obj.message) ??
      asOptionalString(obj.error) ??
      "unknown failure";

    throw ExitApiError.envelopeStatus(taggedStatus, detail);
  }

  const legacyTx = asOptionalString(obj.unsigned_tx_b64);
  if (legacyTx !== undefined) {
    return {
      tx: legacyTx,
      route: obj.route,
      debug: obj.debug,
    };
  }

  const bareTx = asOptionalString(obj.tx);
  if (bareTx !== undefined) {
    return {
      tx: bareTx,
      route: obj.route,
      debug: obj.debug,
    };
  }

  throw ExitApiError.parse("response did not match any supported schema");
}

function summarizeErrorBody(body: string): string {
  try {
    const parsed = JSON.parse(body) as unknown;
    const obj = asRecord(parsed, "error body");
    const message =
      asOptionalString(obj.error) ??
      asOptionalString(obj.message) ??
      asOptionalString(obj.reason);
    if (message !== undefined) {
      return message;
    }
  } catch {
    // Ignore JSON parse errors and fall back to body snippet.
  }

  return body.slice(0, ERROR_BODY_SNIPPET_LEN);
}

type FetchLike = (
  input: RequestInfo | URL,
  init?: RequestInit,
) => Promise<Response>;

function asRecord(value: unknown, path: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw ExitApiError.parse(`${path} must be an object`);
  }
  return value as Record<string, unknown>;
}

function asOptionalString(value: unknown): string | undefined {
  if (typeof value === "string") {
    return value;
  }
  return undefined;
}

function stringifyError(error: unknown): string {
  if (error instanceof Error) {
    return `${error.name}: ${error.message}`;
  }
  return String(error);
}

export const DEFAULT_RETRY_POLICY: RetryPolicy = {
  ...LOW_LATENCY_RETRY_POLICY,
};
