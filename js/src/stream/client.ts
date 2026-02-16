import WebSocket, { type RawData } from "ws";

import {
  clientMessageToText,
  serverMessageFromText,
  type ClientMessage,
  type ServerMessage,
  type StrategyConfigMsg,
} from "./proto.js";

const MIN_RECONNECT_BACKOFF_MS = 100;
const MAX_RECONNECT_BACKOFF_MS = 2_000;
const FRAME_IDLE_SLEEP_MS = 10;

export const STREAM_ENDPOINT = "wss://stream.lasersell.io/v1/ws";
export const LOCAL_STREAM_ENDPOINT = "ws://localhost:8082/v1/ws";

export interface StreamConfigure {
  wallet_pubkeys: string[];
  strategy: StrategyConfigMsg;
}

export function singleWalletStreamConfigure(
  walletPubkey: string,
  strategy: StrategyConfigMsg,
): StreamConfigure {
  return {
    wallet_pubkeys: [walletPubkey],
    strategy,
  };
}

export type PositionSelector =
  | { token_account: string; position_id?: never }
  | { position_id: number; token_account?: never };

export type PositionSelectorInput =
  | PositionSelector
  | string
  | number
  | { tokenAccount: string }
  | { positionId: number }
  | { token_account: string }
  | { position_id: number };

export type StreamClientErrorKind =
  | "websocket"
  | "json"
  | "invalid_api_key_header"
  | "send_queue_closed"
  | "protocol";

export class StreamClientError extends Error {
  readonly kind: StreamClientErrorKind;

  private constructor(
    kind: StreamClientErrorKind,
    message: string,
    cause?: unknown,
  ) {
    super(message, cause === undefined ? undefined : { cause });
    this.name = "StreamClientError";
    this.kind = kind;
  }

  static websocket(cause: unknown): StreamClientError {
    return new StreamClientError(
      "websocket",
      `websocket error: ${stringifyError(cause)}`,
      cause,
    );
  }

  static json(cause: unknown): StreamClientError {
    return new StreamClientError(
      "json",
      `json error: ${stringifyError(cause)}`,
      cause,
    );
  }

  static invalidApiKeyHeader(cause: unknown): StreamClientError {
    return new StreamClientError(
      "invalid_api_key_header",
      `invalid api-key header: ${stringifyError(cause)}`,
      cause,
    );
  }

  static sendQueueClosed(): StreamClientError {
    return new StreamClientError("send_queue_closed", "send queue is closed");
  }

  static protocol(message: string): StreamClientError {
    return new StreamClientError("protocol", `protocol error: ${message}`);
  }
}

export class StreamClient {
  private readonly apiKey: string;
  private local = false;

  constructor(apiKey: string) {
    this.apiKey = apiKey;
  }

  withLocalMode(local: boolean): this {
    this.local = local;
    return this;
  }

  async connect(configure: StreamConfigure): Promise<StreamConnection> {
    const worker = new StreamConnectionWorker(
      this.endpoint(),
      this.apiKey,
      configure,
    );
    await worker.waitReady();
    return new StreamConnection(worker);
  }

  private endpoint(): string {
    if (this.local) {
      return LOCAL_STREAM_ENDPOINT;
    }
    return STREAM_ENDPOINT;
  }
}

export class StreamConnection {
  private readonly worker: StreamConnectionWorker;

  constructor(worker: StreamConnectionWorker) {
    this.worker = worker;
  }

  sender(): StreamSender {
    return new StreamSender(this.worker);
  }

  split(): [StreamSender, StreamReceiver] {
    return [this.sender(), new StreamReceiver(this.worker)];
  }

  async recv(): Promise<ServerMessage | null> {
    return await this.worker.recv();
  }

  close(): void {
    this.worker.close();
  }
}

export class StreamReceiver {
  private readonly worker: StreamConnectionWorker;

  constructor(worker: StreamConnectionWorker) {
    this.worker = worker;
  }

  async recv(): Promise<ServerMessage | null> {
    return await this.worker.recv();
  }

  async *[Symbol.asyncIterator](): AsyncGenerator<ServerMessage, void, void> {
    while (true) {
      const message = await this.recv();
      if (message === null) {
        break;
      }
      yield message;
    }
  }
}

export class StreamSender {
  private readonly worker: StreamConnectionWorker;

  constructor(worker: StreamConnectionWorker) {
    this.worker = worker;
  }

  send(message: ClientMessage): void {
    this.worker.enqueue(message);
  }

  ping(client_time_ms: number): void {
    this.send({
      type: "ping",
      client_time_ms,
    });
  }

  updateStrategy(strategy: StrategyConfigMsg): void {
    this.send({
      type: "update_strategy",
      strategy,
    });
  }

  closePosition(selector: PositionSelectorInput): void {
    this.send(closeMessage(normalizePositionSelector(selector)));
  }

  closeById(positionId: number): void {
    this.closePosition({ position_id: positionId });
  }

  requestExitSignal(
    selector: PositionSelectorInput,
    slippage_bps?: number,
  ): void {
    this.send(
      requestExitSignalMessage(normalizePositionSelector(selector), slippage_bps),
    );
  }

  requestExitSignalById(positionId: number, slippage_bps?: number): void {
    this.requestExitSignal({ position_id: positionId }, slippage_bps);
  }
}

function closeMessage(selector: PositionSelector): ClientMessage {
  if ("token_account" in selector) {
    return {
      type: "close_position",
      token_account: selector.token_account,
    };
  }

  return {
    type: "close_position",
    position_id: selector.position_id,
  };
}

function requestExitSignalMessage(
  selector: PositionSelector,
  slippageBps?: number,
): ClientMessage {
  if ("token_account" in selector) {
    return {
      type: "request_exit_signal",
      token_account: selector.token_account,
      slippage_bps: slippageBps,
    };
  }

  return {
    type: "request_exit_signal",
    position_id: selector.position_id,
    slippage_bps: slippageBps,
  };
}

function normalizePositionSelector(selector: PositionSelectorInput): PositionSelector {
  if (typeof selector === "string") {
    return {
      token_account: selector,
    };
  }

  if (typeof selector === "number") {
    return {
      position_id: selector,
    };
  }

  if ("token_account" in selector && typeof selector.token_account === "string") {
    return {
      token_account: selector.token_account,
    };
  }

  if ("position_id" in selector && typeof selector.position_id === "number") {
    return {
      position_id: selector.position_id,
    };
  }

  if ("tokenAccount" in selector && typeof selector.tokenAccount === "string") {
    return {
      token_account: selector.tokenAccount,
    };
  }

  if ("positionId" in selector && typeof selector.positionId === "number") {
    return {
      position_id: selector.positionId,
    };
  }

  throw StreamClientError.protocol(
    "position selector must be token account string or position id number",
  );
}

type SessionOutcome = "graceful_shutdown" | "reconnect";

class StreamConnectionWorker {
  private readonly endpoint: string;
  private readonly apiKey: string;
  private readonly configure: StreamConfigure;
  private readonly inbound = new AsyncQueue<ServerMessage>();
  private readonly outbound: ClientMessage[] = [];

  private currentSocket: WebSocket | null = null;
  private stopped = false;

  private readonly readyPromise: Promise<void>;
  private readySettled = false;
  private resolveReady!: () => void;
  private rejectReady!: (error: StreamClientError) => void;

  constructor(endpoint: string, apiKey: string, configure: StreamConfigure) {
    this.endpoint = endpoint;
    this.apiKey = apiKey;
    this.configure = configure;

    this.readyPromise = new Promise<void>((resolve, reject) => {
      this.resolveReady = resolve;
      this.rejectReady = (error: StreamClientError) => {
        reject(error);
      };
    });

    void this.run();
  }

  async waitReady(): Promise<void> {
    return await this.readyPromise;
  }

  enqueue(message: ClientMessage): void {
    if (this.stopped) {
      throw StreamClientError.sendQueueClosed();
    }
    this.outbound.push(message);
  }

  async recv(): Promise<ServerMessage | null> {
    return await this.inbound.shift();
  }

  close(): void {
    if (this.stopped) {
      return;
    }

    this.stopped = true;
    if (this.currentSocket !== null) {
      safeClose(this.currentSocket);
    }
  }

  private async run(): Promise<void> {
    let backoffMs = MIN_RECONNECT_BACKOFF_MS;

    while (!this.stopped) {
      try {
        const outcome = await this.runConnectedSession();
        if (outcome === "graceful_shutdown") {
          break;
        }
        backoffMs = MIN_RECONNECT_BACKOFF_MS;
      } catch (error) {
        const mapped = asStreamClientError(error);
        if (!this.readySettled) {
          this.setReadyError(mapped);
          break;
        }
      }

      if (this.stopped) {
        break;
      }

      const shouldContinue = await sleepWithStop(backoffMs, () => this.stopped);
      if (!shouldContinue) {
        break;
      }

      backoffMs = Math.min(backoffMs * 2, MAX_RECONNECT_BACKOFF_MS);
    }

    if (!this.readySettled) {
      this.setReadyError(StreamClientError.sendQueueClosed());
    }

    this.inbound.close();
  }

  private async runConnectedSession(): Promise<SessionOutcome> {
    const socket = await openSocket(this.endpoint, this.apiKey);
    this.currentSocket = socket;

    const frames = new WebSocketFrameQueue(socket);

    try {
      const firstServerMessage = await recvServerMessageBeforeConfigure(socket, frames);
      if (firstServerMessage.type !== "hello_ok") {
        throw StreamClientError.protocol(
          "expected first server message to be hello_ok",
        );
      }
      this.inbound.push(firstServerMessage);

      const configureMessage: ClientMessage = {
        type: "configure",
        wallet_pubkeys: [...this.configure.wallet_pubkeys],
        strategy: { ...this.configure.strategy },
      };
      await sendClientMessage(socket, configureMessage);

      const configuredMessage = await recvServerMessageAfterConfigure(socket, frames);
      this.inbound.push(configuredMessage);

      if (!this.readySettled) {
        this.readySettled = true;
        this.resolveReady();
      }

      while (!this.stopped) {
        const nextOutbound = this.outbound[0];
        if (nextOutbound !== undefined) {
          try {
            await sendClientMessage(socket, nextOutbound);
            this.outbound.shift();
            continue;
          } catch {
            return "reconnect";
          }
        }

        const frame = frames.shiftNow();
        if (frame !== undefined) {
          const outcome = await this.handleFrame(socket, frame);
          if (outcome === "reconnect") {
            return "reconnect";
          }
          continue;
        }

        await sleep(FRAME_IDLE_SLEEP_MS);
      }

      return "graceful_shutdown";
    } finally {
      this.currentSocket = null;
      safeClose(socket);
    }
  }

  private async handleFrame(
    socket: WebSocket,
    frame: WsFrame,
  ): Promise<SessionOutcome | "continue"> {
    switch (frame.kind) {
      case "text": {
        let parsed: ServerMessage;
        try {
          parsed = serverMessageFromText(frame.text);
        } catch {
          return "reconnect";
        }
        this.inbound.push(parsed);
        return "continue";
      }
      case "ping": {
        try {
          socket.pong(frame.payload);
        } catch {
          return "reconnect";
        }
        return "continue";
      }
      case "pong": {
        return "continue";
      }
      case "binary":
      case "close":
      case "error": {
        return "reconnect";
      }
      default: {
        const _unreachable: never = frame;
        return _unreachable;
      }
    }
  }

  private setReadyError(error: StreamClientError): void {
    if (this.readySettled) {
      return;
    }

    this.readySettled = true;
    this.rejectReady(error);
  }
}

async function openSocket(url: string, apiKey: string): Promise<WebSocket> {
  return await new Promise<WebSocket>((resolve, reject) => {
    let settled = false;

    let socket: WebSocket;
    try {
      socket = new WebSocket(url, {
        headers: {
          "x-api-key": apiKey,
        },
      });
    } catch (error) {
      reject(StreamClientError.invalidApiKeyHeader(error));
      return;
    }

    const onOpen = (): void => {
      if (settled) {
        return;
      }
      settled = true;
      cleanup();
      resolve(socket);
    };

    const onError = (error: Error): void => {
      if (settled) {
        return;
      }
      settled = true;
      cleanup();
      reject(StreamClientError.websocket(error));
    };

    const onClose = (): void => {
      if (settled) {
        return;
      }
      settled = true;
      cleanup();
      reject(StreamClientError.protocol("socket closed before open"));
    };

    const cleanup = (): void => {
      socket.off("open", onOpen);
      socket.off("error", onError);
      socket.off("close", onClose);
    };

    socket.on("open", onOpen);
    socket.on("error", onError);
    socket.on("close", onClose);
  });
}

async function recvServerMessageBeforeConfigure(
  socket: WebSocket,
  frames: WebSocketFrameQueue,
): Promise<ServerMessage> {
  while (true) {
    const frame = await frames.waitNext();
    switch (frame.kind) {
      case "text": {
        try {
          return serverMessageFromText(frame.text);
        } catch (error) {
          throw StreamClientError.json(error);
        }
      }
      case "ping": {
        socket.pong(frame.payload);
        break;
      }
      case "pong": {
        break;
      }
      case "close": {
        throw StreamClientError.protocol("socket closed before hello_ok");
      }
      case "error": {
        throw StreamClientError.websocket(frame.error);
      }
      case "binary": {
        throw StreamClientError.protocol(
          "received non-text frame before hello_ok",
        );
      }
      default: {
        const _unreachable: never = frame;
        throw _unreachable;
      }
    }
  }
}

async function recvServerMessageAfterConfigure(
  socket: WebSocket,
  frames: WebSocketFrameQueue,
): Promise<ServerMessage> {
  while (true) {
    const frame = await frames.waitNext();
    switch (frame.kind) {
      case "text": {
        try {
          return serverMessageFromText(frame.text);
        } catch (error) {
          throw StreamClientError.json(error);
        }
      }
      case "ping": {
        socket.pong(frame.payload);
        break;
      }
      case "pong": {
        break;
      }
      case "close": {
        throw StreamClientError.protocol(
          "socket closed before configure acknowledgement",
        );
      }
      case "error": {
        throw StreamClientError.websocket(frame.error);
      }
      case "binary": {
        throw StreamClientError.protocol(
          "received non-text frame before configure acknowledgement",
        );
      }
      default: {
        const _unreachable: never = frame;
        throw _unreachable;
      }
    }
  }
}

async function sendClientMessage(
  socket: WebSocket,
  message: ClientMessage,
): Promise<void> {
  const text = clientMessageToText(message);

  await new Promise<void>((resolve, reject) => {
    socket.send(text, (error) => {
      if (error) {
        reject(StreamClientError.websocket(error));
      } else {
        resolve();
      }
    });
  });
}

function safeClose(socket: WebSocket): void {
  if (
    socket.readyState === WebSocket.CLOSING ||
    socket.readyState === WebSocket.CLOSED
  ) {
    return;
  }
  socket.close();
}

type WsFrame =
  | { kind: "text"; text: string }
  | { kind: "binary" }
  | { kind: "ping"; payload: Buffer }
  | { kind: "pong"; payload: Buffer }
  | { kind: "close"; code: number; reason: string }
  | { kind: "error"; error: Error };

class WebSocketFrameQueue {
  private readonly frames: WsFrame[] = [];

  constructor(socket: WebSocket) {
    socket.on("message", (data, isBinary) => {
      if (isBinary) {
        this.frames.push({ kind: "binary" });
        return;
      }

      const text = rawDataToString(data);
      this.frames.push({ kind: "text", text });
    });

    socket.on("ping", (payload) => {
      this.frames.push({
        kind: "ping",
        payload: rawDataToBuffer(payload),
      });
    });

    socket.on("pong", (payload) => {
      this.frames.push({
        kind: "pong",
        payload: rawDataToBuffer(payload),
      });
    });

    socket.on("close", (code, reason) => {
      this.frames.push({
        kind: "close",
        code,
        reason: reason.toString("utf8"),
      });
    });

    socket.on("error", (error) => {
      this.frames.push({ kind: "error", error });
    });
  }

  shiftNow(): WsFrame | undefined {
    return this.frames.shift();
  }

  async waitNext(): Promise<WsFrame> {
    while (true) {
      const next = this.shiftNow();
      if (next !== undefined) {
        return next;
      }
      await sleep(FRAME_IDLE_SLEEP_MS);
    }
  }
}

class AsyncQueue<T> {
  private readonly items: T[] = [];
  private readonly waiters: Array<(value: T | null) => void> = [];
  private closed = false;

  push(item: T): void {
    if (this.closed) {
      return;
    }

    const waiter = this.waiters.shift();
    if (waiter !== undefined) {
      waiter(item);
      return;
    }

    this.items.push(item);
  }

  close(): void {
    if (this.closed) {
      return;
    }

    this.closed = true;
    while (this.waiters.length > 0) {
      const waiter = this.waiters.shift();
      waiter?.(null);
    }
  }

  async shift(): Promise<T | null> {
    const next = this.items.shift();
    if (next !== undefined) {
      return next;
    }

    if (this.closed) {
      return null;
    }

    return await new Promise<T | null>((resolve) => {
      this.waiters.push(resolve);
    });
  }
}

function rawDataToString(data: RawData): string {
  if (typeof data === "string") {
    return data;
  }

  return rawDataToBuffer(data).toString("utf8");
}

function rawDataToBuffer(data: RawData): Buffer {
  if (Buffer.isBuffer(data)) {
    return data;
  }

  if (Array.isArray(data)) {
    return Buffer.concat(data);
  }

  const value: unknown = data;

  if (value instanceof ArrayBuffer) {
    return Buffer.from(value);
  }

  if (ArrayBuffer.isView(value)) {
    return Buffer.from(value.buffer, value.byteOffset, value.byteLength);
  }

  throw StreamClientError.protocol("unexpected websocket raw data frame");
}

function asStreamClientError(error: unknown): StreamClientError {
  if (error instanceof StreamClientError) {
    return error;
  }

  return StreamClientError.websocket(error);
}

async function sleepWithStop(
  ms: number,
  shouldStop: () => boolean,
): Promise<boolean> {
  const intervalMs = 20;
  let remaining = ms;

  while (remaining > 0) {
    if (shouldStop()) {
      return false;
    }

    const step = Math.min(intervalMs, remaining);
    await sleep(step);
    remaining -= step;
  }

  return !shouldStop();
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

function stringifyError(error: unknown): string {
  if (error instanceof Error) {
    return `${error.name}: ${error.message}`;
  }
  return String(error);
}
