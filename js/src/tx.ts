import {
  VersionedTransaction,
  type Signer,
} from "@solana/web3.js";

const ERROR_BODY_SNIPPET_LEN = 220;

export const HELIUS_SENDER_FAST_URL = "https://sender.helius-rpc.com/fast";

export type TxSubmitErrorKind =
  | "decode_unsigned_tx"
  | "deserialize_unsigned_tx"
  | "sign_tx"
  | "serialize_tx"
  | "request_send"
  | "response_read"
  | "http_status"
  | "decode_response"
  | "rpc_error"
  | "missing_result";

export class TxSubmitError extends Error {
  readonly kind: TxSubmitErrorKind;
  readonly target?: string;
  readonly status?: number;
  readonly body?: string;

  private constructor(
    kind: TxSubmitErrorKind,
    message: string,
    options?: {
      cause?: unknown;
      target?: string;
      status?: number;
      body?: string;
    },
  ) {
    super(message, { cause: options?.cause });
    this.name = "TxSubmitError";
    this.kind = kind;
    this.target = options?.target;
    this.status = options?.status;
    this.body = options?.body;
  }

  static decodeUnsignedTx(cause: unknown): TxSubmitError {
    return new TxSubmitError(
      "decode_unsigned_tx",
      `decode unsigned tx b64: ${stringifyError(cause)}`,
      { cause },
    );
  }

  static deserializeUnsignedTx(cause: unknown): TxSubmitError {
    return new TxSubmitError(
      "deserialize_unsigned_tx",
      `deserialize unsigned tx: ${stringifyError(cause)}`,
      { cause },
    );
  }

  static signTx(cause: unknown): TxSubmitError {
    return new TxSubmitError("sign_tx", `sign tx: ${stringifyError(cause)}`, {
      cause,
    });
  }

  static serializeTx(cause: unknown): TxSubmitError {
    return new TxSubmitError(
      "serialize_tx",
      `serialize tx: ${stringifyError(cause)}`,
      { cause },
    );
  }

  static requestSend(target: string, cause: unknown): TxSubmitError {
    return new TxSubmitError(
      "request_send",
      `${target} request send failed: ${stringifyError(cause)}`,
      { cause, target },
    );
  }

  static responseRead(target: string, cause: unknown): TxSubmitError {
    return new TxSubmitError(
      "response_read",
      `${target} response read failed: ${stringifyError(cause)}`,
      { cause, target },
    );
  }

  static httpStatus(target: string, status: number, body: string): TxSubmitError {
    return new TxSubmitError("http_status", `${target} http ${status}: ${body}`, {
      target,
      status,
      body,
    });
  }

  static decodeResponse(
    target: string,
    body: string,
    cause: unknown,
  ): TxSubmitError {
    return new TxSubmitError(
      "decode_response",
      `${target} response decode failed: ${stringifyError(cause)}. body=${body}`,
      {
        target,
        cause,
        body,
      },
    );
  }

  static rpcError(target: string, error: string): TxSubmitError {
    return new TxSubmitError("rpc_error", `${target} returned error: ${error}`, {
      target,
    });
  }

  static missingResult(target: string, response: string): TxSubmitError {
    return new TxSubmitError(
      "missing_result",
      `${target} response missing signature: ${response}`,
      { target },
    );
  }
}

export function signUnsignedTx(
  unsignedTxB64: string,
  signer: Signer,
): VersionedTransaction {
  let raw: Uint8Array;
  try {
    raw = decodeBase64Strict(unsignedTxB64);
  } catch (error) {
    throw TxSubmitError.decodeUnsignedTx(error);
  }

  let unsignedTx: VersionedTransaction;
  try {
    unsignedTx = VersionedTransaction.deserialize(raw);
  } catch (error) {
    throw TxSubmitError.deserializeUnsignedTx(error);
  }

  try {
    unsignedTx.sign([signer]);
    return unsignedTx;
  } catch (error) {
    throw TxSubmitError.signTx(error);
  }
}

export function encodeSignedTx(tx: VersionedTransaction): string {
  try {
    return Buffer.from(tx.serialize()).toString("base64");
  } catch (error) {
    throw TxSubmitError.serializeTx(error);
  }
}

export async function sendViaHeliusSender(
  tx: VersionedTransaction,
  options: SendTransactionOptions = {},
): Promise<string> {
  const txB64 = encodeSignedTx(tx);
  return await sendViaHeliusSenderB64(txB64, options);
}

export async function sendViaRpc(
  rpcUrl: string,
  tx: VersionedTransaction,
  options: SendTransactionOptions = {},
): Promise<string> {
  const txB64 = encodeSignedTx(tx);
  return await sendViaRpcB64(rpcUrl, txB64, options);
}

export async function sendViaHeliusSenderB64(
  txB64: string,
  options: SendTransactionOptions = {},
): Promise<string> {
  return await sendTransactionB64(
    HELIUS_SENDER_FAST_URL,
    "helius sender",
    txB64,
    false,
    options.fetch_impl ?? globalThis.fetch,
  );
}

export async function sendViaRpcB64(
  rpcUrl: string,
  txB64: string,
  options: SendTransactionOptions = {},
): Promise<string> {
  return await sendTransactionB64(
    rpcUrl,
    "rpc",
    txB64,
    true,
    options.fetch_impl ?? globalThis.fetch,
  );
}

export interface SendTransactionOptions {
  fetch_impl?: FetchLike;
}

async function sendTransactionB64(
  endpoint: string,
  target: string,
  txB64: string,
  includePreflightCommitment: boolean,
  fetchImpl: FetchLike | undefined,
): Promise<string> {
  if (typeof fetchImpl !== "function") {
    throw TxSubmitError.requestSend(target, "no fetch implementation available");
  }

  const config: Record<string, unknown> = {
    encoding: "base64",
    skipPreflight: true,
    maxRetries: 0,
  };

  if (includePreflightCommitment) {
    config.preflightCommitment = "processed";
  }

  const payload = {
    jsonrpc: "2.0",
    id: 1,
    method: "sendTransaction",
    params: [txB64, config],
  };

  let response: Response;
  try {
    response = await fetchImpl(endpoint, {
      method: "POST",
      headers: {
        "content-type": "application/json",
      },
      body: JSON.stringify(payload),
    });
  } catch (error) {
    throw TxSubmitError.requestSend(target, error);
  }

  let body: string;
  try {
    body = await response.text();
  } catch (error) {
    throw TxSubmitError.responseRead(target, error);
  }

  if (!response.ok) {
    throw TxSubmitError.httpStatus(target, response.status, summarizeBody(body));
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(body);
  } catch (error) {
    throw TxSubmitError.decodeResponse(target, summarizeBody(body), error);
  }

  const obj = asRecord(parsed, target);
  if (obj.error !== undefined) {
    throw TxSubmitError.rpcError(target, JSON.stringify(obj.error));
  }

  const result = obj.result;
  if (typeof result === "string" && result.length > 0) {
    return result;
  }

  throw TxSubmitError.missingResult(
    target,
    typeof result === "undefined" ? JSON.stringify(obj) : JSON.stringify(result),
  );
}

function decodeBase64Strict(value: string): Uint8Array {
  const normalized = value.trim();
  if (normalized.length === 0) {
    throw new Error("base64 payload is empty");
  }

  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(normalized) || normalized.length % 4 !== 0) {
    throw new Error("invalid base64 encoding");
  }

  return Buffer.from(normalized, "base64");
}

function summarizeBody(body: string): string {
  return body.slice(0, ERROR_BODY_SNIPPET_LEN);
}

type FetchLike = (
  input: RequestInfo | URL,
  init?: RequestInit,
) => Promise<Response>;

function asRecord(value: unknown, target: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw TxSubmitError.decodeResponse(
      target,
      JSON.stringify(value),
      "response was not an object",
    );
  }
  return value as Record<string, unknown>;
}

function stringifyError(error: unknown): string {
  if (error instanceof Error) {
    return `${error.name}: ${error.message}`;
  }
  return String(error);
}
