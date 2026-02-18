import {
  VersionedTransaction,
  type Signer,
} from "@solana/web3.js";

import { ed25519 } from "@noble/curves/ed25519";

const ERROR_BODY_SNIPPET_LEN = 220;

export const HELIUS_SENDER_BASE_URL = "https://sender.helius-rpc.com";
export const HELIUS_SENDER_FAST_URL = `${HELIUS_SENDER_BASE_URL}/fast`;
export const HELIUS_SENDER_PING_URL = `${HELIUS_SENDER_BASE_URL}/ping`;

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

/**
 * Fast-path signer for LaserSell-provided unsigned transactions.
 *
 * Unlike `signUnsignedTx` (which deserializes into a `VersionedTransaction`), this method:
 * 1) base64-decodes the transaction
 * 2) signs the raw message bytes
 * 3) patches the signature bytes in-place
 * 4) re-encodes to base64
 *
 * This avoids `VersionedTransaction.deserialize()` and `tx.serialize()` on the hot path.
 */
export function signUnsignedTxB64Fast(
  unsignedTxB64: string,
  signer: Signer,
): string {
  let raw: Uint8Array;
  try {
    raw = decodeBase64Strict(unsignedTxB64);
  } catch (error) {
    throw TxSubmitError.decodeUnsignedTx(error);
  }

  try {
    patchSignTransactionInPlace(raw, signer);
  } catch (error) {
    throw TxSubmitError.signTx(error);
  }

  try {
    return Buffer.from(raw).toString("base64");
  } catch (error) {
    throw TxSubmitError.serializeTx(error);
  }
}

export async function signAndSendUnsignedTxViaHeliusSenderB64(
  unsignedTxB64: string,
  signer: Signer,
  options: SendTransactionOptions = {},
): Promise<string> {
  const signedTxB64 = signUnsignedTxB64Fast(unsignedTxB64, signer);
  return await sendViaHeliusSenderB64(signedTxB64, options);
}

export async function signAndSendUnsignedTxViaRpcB64(
  rpcUrl: string,
  unsignedTxB64: string,
  signer: Signer,
  options: SendTransactionOptions = {},
): Promise<string> {
  const signedTxB64 = signUnsignedTxB64Fast(unsignedTxB64, signer);
  return await sendViaRpcB64(rpcUrl, signedTxB64, options);
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

export async function pingHeliusSender(
  options: {
    baseUrl?: string;
    fetch_impl?: FetchLike;
  } = {},
): Promise<boolean> {
  const fetchImpl = options.fetch_impl ?? globalThis.fetch;
  if (typeof fetchImpl !== "function") {
    return false;
  }

  const pingUrl =
    options.baseUrl === undefined
      ? HELIUS_SENDER_PING_URL
      : senderPingUrl(options.baseUrl);
  try {
    const response = await fetchImpl(pingUrl, { method: "GET" });
    return response.ok;
  } catch {
    return false;
  }
}

export function startHeliusSenderWarmLoop(
  options: {
    baseUrl?: string;
    intervalMs?: number;
    fetch_impl?: FetchLike;
  } = {},
): { stop: () => void } {
  const fetchImpl = options.fetch_impl ?? globalThis.fetch;
  const intervalMs = options.intervalMs ?? 30_000;
  const pingUrl =
    options.baseUrl === undefined
      ? HELIUS_SENDER_PING_URL
      : senderPingUrl(options.baseUrl);

  if (typeof fetchImpl !== "function") {
    return { stop: () => {} };
  }

  const handle: ReturnType<typeof setInterval> = setInterval(() => {
    void fetchImpl(pingUrl, { method: "GET" }).catch(() => {
      // Best-effort only.
    });
  }, intervalMs);

  return {
    stop: () => clearInterval(handle),
  };
}

function senderPingUrl(baseUrl: string): string {
  const trimmed = baseUrl.trim().replace(/\/+$/, "");
  const withoutFast = trimmed.endsWith("/fast")
    ? trimmed.slice(0, trimmed.length - "/fast".length)
    : trimmed;
  return `${withoutFast}/ping`;
}

function patchSignTransactionInPlace(rawTx: Uint8Array, signer: Signer): void {
  const { value: signatureCount, bytesRead: sigCountLen } = decodeShortVecLen(
    rawTx,
    0,
  );

  const signaturesLen = signatureCount * 64;
  const messageStart = sigCountLen + signaturesLen;
  if (messageStart > rawTx.length) {
    throw new Error("transaction is truncated (signatures)");
  }

  const message = rawTx.subarray(messageStart);
  const signerPubkey = signer.publicKey.toBytes();
  const { index: maybeIndex, numRequiredSignatures } =
    findSignerIndexInMessage(message, signerPubkey);

  let signatureIndex = maybeIndex;
  if (signatureIndex === null) {
    if (signatureCount === 1 && numRequiredSignatures === 1) {
      signatureIndex = 0;
    } else {
      throw new Error(
        "signer pubkey not found among required signer accounts in tx message",
      );
    }
  }

  if (signatureIndex >= signatureCount) {
    throw new Error(
      `signer signature index (${signatureIndex}) out of range for signature vector length (${signatureCount})`,
    );
  }

  const signature = ed25519.sign(message, signer.secretKey.slice(0, 32));
  const signatureOffset = sigCountLen + signatureIndex * 64;
  rawTx.set(signature, signatureOffset);
}

function findSignerIndexInMessage(
  message: Uint8Array,
  signerPubkey: Uint8Array,
): { index: number | null; numRequiredSignatures: number } {
  if (signerPubkey.length !== 32) {
    throw new Error("signer pubkey must be 32 bytes");
  }

  const first = getByte(message, 0, "message prefix");
  let offset = 0;
  if ((first & 0x80) !== 0) {
    const version = first & 0x7f;
    if (version !== 0) {
      throw new Error(`unsupported Solana message version: ${version}`);
    }
    offset = 1;
  }

  const numRequiredSignatures = getByte(
    message,
    offset,
    "header.numRequiredSignatures",
  );

  // Skip message header: numRequiredSignatures, numReadonlySignedAccounts, numReadonlyUnsignedAccounts
  offset += 3;

  const { value: accountKeyCount, bytesRead: keyCountLen } = decodeShortVecLen(
    message,
    offset,
  );
  offset += keyCountLen;

  const keysBytes = accountKeyCount * 32;
  if (offset + keysBytes > message.length) {
    throw new Error("message is truncated (account keys)");
  }

  const signerCount = Math.min(numRequiredSignatures, accountKeyCount);
  for (let i = 0; i < signerCount; i += 1) {
    const keyOffset = offset + i * 32;
    if (bytesEqual32(message, keyOffset, signerPubkey)) {
      return { index: i, numRequiredSignatures };
    }
  }

  return { index: null, numRequiredSignatures };
}

function decodeShortVecLen(
  bytes: Uint8Array,
  offset: number,
): { value: number; bytesRead: number } {
  let value = 0;
  let shift = 0;
  let bytesRead = 0;

  while (true) {
    const byte = getByte(bytes, offset + bytesRead, "shortvec byte");
    value |= (byte & 0x7f) << shift;
    bytesRead += 1;
    if ((byte & 0x80) === 0) {
      break;
    }
    shift += 7;
    if (shift > 28) {
      throw new Error("shortvec length overflow");
    }
  }

  return { value, bytesRead };
}

function bytesEqual32(bytes: Uint8Array, offset: number, other: Uint8Array): boolean {
  for (let i = 0; i < 32; i += 1) {
    if (getByte(bytes, offset + i, "pubkey byte") !== getByte(other, i)) {
      return false;
    }
  }
  return true;
}

function getByte(bytes: Uint8Array, index: number, label = "byte"): number {
  const value = bytes[index];
  if (value === undefined) {
    throw new Error(`${label} out of range`);
  }
  return value;
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
