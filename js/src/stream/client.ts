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

export const STREAM_ENDPOINT = "wss://stream.lasersell.io/v1/ws";
export const LOCAL_STREAM_ENDPOINT = "ws://localhost:8082/v1/ws";

export interface StreamConfigure {
  wallet_pubkeys: string[];
  strategy: StrategyConfigMsg;
  deadline_timeout_sec?: number;
}

export interface StreamLanesOptions {
  /**
   * Maximum number of low-priority messages (currently `pnl_update`) to buffer.
   *
   * When full, the oldest low-priority message is dropped to keep the stream hot path responsive.
   *
   * Defaults to 1024.
   */
  lowPriorityCapacity?: number;
}

export function singleWalletStreamConfigure(
  walletPubkey: string,
  strategy: StrategyConfigMsg,
  deadlineTimeoutSec?: number,
): StreamConfigure {
  return {
    wallet_pubkeys: [walletPubkey],
    strategy,
    deadline_timeout_sec: deadlineTimeoutSec,
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
  private endpointOverride?: string;

  constructor(apiKey: string) {
    this.apiKey = apiKey;
  }

  withLocalMode(local: boolean): this {
    this.local = local;
    return this;
  }

  withEndpoint(endpoint: string): this {
    this.endpointOverride = endpoint.trimEnd();
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

  async connectLanes(
    configure: StreamConfigure,
    options: StreamLanesOptions = {},
  ): Promise<StreamConnectionLanes> {
    const worker = new StreamConnectionWorker(
      this.endpoint(),
      this.apiKey,
      configure,
      {
        inboundMode: "lanes",
        lowPriorityCapacity: options.lowPriorityCapacity,
      },
    );
    await worker.waitReady();
    return new StreamConnectionLanes(worker);
  }

  private endpoint(): string {
    if (this.endpointOverride !== undefined) {
      return this.endpointOverride;
    }
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

export class StreamConnectionLanes {
  private readonly worker: StreamConnectionWorker;

  constructor(worker: StreamConnectionWorker) {
    this.worker = worker;
  }

  sender(): StreamSender {
    return new StreamSender(this.worker);
  }

  highReceiver(): StreamHighReceiver {
    return new StreamHighReceiver(this.worker);
  }

  lowReceiver(): StreamLowReceiver {
    return new StreamLowReceiver(this.worker);
  }

  split(): [StreamSender, StreamHighReceiver, StreamLowReceiver] {
    return [this.sender(), this.highReceiver(), this.lowReceiver()];
  }

  close(): void {
    this.worker.close();
  }
}

export class StreamHighReceiver {
  private readonly worker: StreamConnectionWorker;

  constructor(worker: StreamConnectionWorker) {
    this.worker = worker;
  }

  async recv(): Promise<ServerMessage | null> {
    return await this.worker.recvHigh();
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

export class StreamLowReceiver {
  private readonly worker: StreamConnectionWorker;

  constructor(worker: StreamConnectionWorker) {
    this.worker = worker;
  }

  async recv(): Promise<ServerMessage | null> {
    return await this.worker.recvLow();
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

type InboundMode = "combined" | "lanes";

interface StreamConnectionWorkerOptions {
  inboundMode?: InboundMode;
  lowPriorityCapacity?: number;
}

class StreamConnectionWorker {
  private readonly endpoint: string;
  private readonly apiKey: string;
  private readonly configure: StreamConfigure;

  private readonly inboundMode: InboundMode;
  private readonly inboundCombined?: AsyncQueue<ServerMessage>;
  private readonly inboundHigh?: AsyncQueue<ServerMessage>;
  private readonly inboundLow?: AsyncQueue<ServerMessage>;

  private readonly outbound = new AsyncQueue<ClientMessage>();

  private currentSocket: WebSocket | null = null;
  private stopped = false;

  private readonly readyPromise: Promise<void>;
  private readySettled = false;
  private resolveReady!: () => void;
  private rejectReady!: (error: StreamClientError) => void;

  constructor(
    endpoint: string,
    apiKey: string,
    configure: StreamConfigure,
    options: StreamConnectionWorkerOptions = {},
  ) {
    this.endpoint = endpoint;
    this.apiKey = apiKey;
    this.configure = configure;

    this.inboundMode = options.inboundMode ?? "combined";
    if (this.inboundMode === "combined") {
      this.inboundCombined = new AsyncQueue<ServerMessage>();
    } else {
      this.inboundHigh = new AsyncQueue<ServerMessage>();
      this.inboundLow = new AsyncQueue<ServerMessage>({
        maxLen: options.lowPriorityCapacity ?? 1024,
        dropOldest: true,
      });
    }

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
    if (this.inboundCombined === undefined) {
      throw StreamClientError.protocol(
        "recv() is not available for lane-mode connections; use StreamConnectionLanes receivers",
      );
    }
    return await this.inboundCombined.shift();
  }

  async recvHigh(): Promise<ServerMessage | null> {
    if (this.inboundHigh === undefined) {
      throw StreamClientError.protocol(
        "recvHigh() is only available for lane-mode connections",
      );
    }
    return await this.inboundHigh.shift();
  }

  async recvLow(): Promise<ServerMessage | null> {
    if (this.inboundLow === undefined) {
      throw StreamClientError.protocol(
        "recvLow() is only available for lane-mode connections",
      );
    }
    return await this.inboundLow.shift();
  }

  close(): void {
    if (this.stopped) {
      return;
    }

    this.stopped = true;
    this.outbound.close();
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

    this.stopped = true;
    this.outbound.close();
    this.closeInbound();
  }

  private async runConnectedSession(): Promise<SessionOutcome> {
    const { socket, frames } = await openSocket(this.endpoint, this.apiKey);
    this.currentSocket = socket;

    try {
      const firstServerMessage = await recvServerMessageBeforeConfigure(socket, frames);
      if (firstServerMessage.type !== "hello_ok") {
        throw StreamClientError.protocol(
          "expected first server message to be hello_ok",
        );
      }
      this.pushInbound(firstServerMessage);

      const configureMessage: ClientMessage = {
        type: "configure",
        wallet_pubkeys: [...this.configure.wallet_pubkeys],
        strategy: { ...this.configure.strategy },
      };
      await sendClientMessage(socket, configureMessage);

      const configuredMessage = await recvServerMessageAfterConfigure(socket, frames);
      this.pushInbound(configuredMessage);

      if (!this.readySettled) {
        this.readySettled = true;
        this.resolveReady();
      }

      while (!this.stopped) {
        const nextOutbound = this.outbound.shiftNow();
        if (nextOutbound !== undefined) {
          try {
            await sendClientMessage(socket, nextOutbound);
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

        const outboundWait = this.outbound.shiftCancelable();
        const frameWait = frames.waitNextCancelable();
        const next = await Promise.race([
          outboundWait.promise.then((message) => ({
            source: "outbound" as const,
            message,
          })),
          frameWait.promise.then((nextFrame) => ({
            source: "frame" as const,
            frame: nextFrame,
          })),
        ]);

        if (next.source === "outbound") {
          frameWait.cancel();
          if (next.message === null) {
            return "graceful_shutdown";
          }

          try {
            await sendClientMessage(socket, next.message);
            continue;
          } catch {
            return "reconnect";
          }
        }

        outboundWait.cancel();
        const outcome = await this.handleFrame(socket, next.frame);
        if (outcome === "reconnect") {
          return "reconnect";
        }
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
        this.pushInbound(parsed);
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

  private pushInbound(message: ServerMessage): void {
    if (this.inboundMode === "combined") {
      this.inboundCombined!.push(message);
      return;
    }

    if (message.type === "pnl_update") {
      this.inboundLow!.push(message);
    } else {
      this.inboundHigh!.push(message);
    }
  }

  private closeInbound(): void {
    this.inboundCombined?.close();
    this.inboundHigh?.close();
    this.inboundLow?.close();
  }
}

async function openSocket(
  url: string,
  apiKey: string,
): Promise<{ socket: WebSocket; frames: WebSocketFrameQueue }> {
  return await new Promise<{ socket: WebSocket; frames: WebSocketFrameQueue }>(
    (resolve, reject) => {
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

      // Attach frame listeners before waiting for `open` so we never miss an
      // immediate server hello frame on low-latency links.
      const frames = new WebSocketFrameQueue(socket);

      const onOpen = (): void => {
        if (settled) {
          return;
        }
        settled = true;
        cleanup();
        resolve({ socket, frames });
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
    },
  );
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

class Deque<T> {
  private buffer: Array<T | undefined>;
  private head = 0;
  private len = 0;
  private mask: number;

  constructor(initialCapacity = 16) {
    const cap = nextPowerOfTwo(Math.max(2, initialCapacity));
    this.buffer = new Array<T | undefined>(cap);
    this.mask = cap - 1;
  }

  get length(): number {
    return this.len;
  }

  push(value: T): void {
    if (this.len === this.buffer.length) {
      this.grow();
    }
    const index = (this.head + this.len) & this.mask;
    this.buffer[index] = value;
    this.len += 1;
  }

  unshift(value: T): void {
    if (this.len === this.buffer.length) {
      this.grow();
    }
    this.head = (this.head - 1) & this.mask;
    this.buffer[this.head] = value;
    this.len += 1;
  }

  shift(): T | undefined {
    if (this.len === 0) {
      return undefined;
    }
    const value = this.buffer[this.head];
    this.buffer[this.head] = undefined;
    this.head = (this.head + 1) & this.mask;
    this.len -= 1;
    return value;
  }

  private grow(): void {
    const old = this.buffer;
    const newCap = old.length * 2;
    const next = new Array<T | undefined>(newCap);
    for (let i = 0; i < this.len; i += 1) {
      next[i] = old[(this.head + i) & this.mask];
    }
    this.buffer = next;
    this.head = 0;
    this.mask = newCap - 1;
  }
}

function nextPowerOfTwo(value: number): number {
  let v = Math.max(2, Math.floor(value));
  v -= 1;
  v |= v >> 1;
  v |= v >> 2;
  v |= v >> 4;
  v |= v >> 8;
  v |= v >> 16;
  return v + 1;
}

class WebSocketFrameQueue {
  private readonly frames = new Deque<WsFrame>();
  private readonly waiters: Array<{
    canceled: boolean;
    hasValue: boolean;
    value: WsFrame | null;
    settled: boolean;
    resolve: (frame: WsFrame) => void;
  }> = [];

  constructor(socket: WebSocket) {
    socket.on("message", (data, isBinary) => {
      if (isBinary) {
        this.push({ kind: "binary" });
        return;
      }

      const text = rawDataToString(data);
      this.push({ kind: "text", text });
    });

    socket.on("ping", (payload) => {
      this.push({
        kind: "ping",
        payload: rawDataToBuffer(payload),
      });
    });

    socket.on("pong", (payload) => {
      this.push({
        kind: "pong",
        payload: rawDataToBuffer(payload),
      });
    });

    socket.on("close", (code, reason) => {
      this.push({
        kind: "close",
        code,
        reason: reason.toString("utf8"),
      });
    });

    socket.on("error", (error) => {
      this.push({ kind: "error", error });
    });
  }

  shiftNow(): WsFrame | undefined {
    return this.frames.shift();
  }

  waitNextCancelable(): {
    promise: Promise<WsFrame>;
    cancel: () => void;
  } {
    const next = this.shiftNow();
    if (next !== undefined) {
      return {
        promise: Promise.resolve(next),
        cancel: () => {},
      };
    }

    let waiter:
      | {
          canceled: boolean;
          hasValue: boolean;
          value: WsFrame | null;
          settled: boolean;
          resolve: (frame: WsFrame) => void;
        }
      | undefined;
    const promise = new Promise<WsFrame>((resolve) => {
      waiter = {
        canceled: false,
        hasValue: false,
        value: null,
        settled: false,
        resolve,
      };
      this.waiters.push(waiter);
    });

    const cancel = (): void => {
      if (waiter === undefined || waiter.canceled) {
        return;
      }
      waiter.canceled = true;
      if (!waiter.settled) {
        const index = this.waiters.indexOf(waiter);
        if (index >= 0) {
          this.waiters.splice(index, 1);
        }
        return;
      }
      if (waiter.hasValue && waiter.value !== null) {
        this.frames.unshift(waiter.value);
        waiter.hasValue = false;
        waiter.value = null;
      }
    };

    return { promise, cancel };
  }

  async waitNext(): Promise<WsFrame> {
    const { promise } = this.waitNextCancelable();
    return await promise;
  }

  private push(frame: WsFrame): void {
    const waiter = this.waiters.shift();
    if (waiter !== undefined) {
      waiter.settled = true;
      waiter.hasValue = true;
      waiter.value = frame;
      waiter.resolve(frame);
      return;
    }

    this.frames.push(frame);
  }
}

class AsyncQueue<T> {
  private readonly items = new Deque<T>();
  private readonly waiters: Array<{
    canceled: boolean;
    hasValue: boolean;
    value: T | null;
    settled: boolean;
    resolve: (value: T | null) => void;
  }> = [];
  private closed = false;

  private readonly maxLen?: number;
  private readonly dropOldest: boolean;

  constructor(options?: { maxLen?: number; dropOldest?: boolean }) {
    this.maxLen = options?.maxLen;
    this.dropOldest = options?.dropOldest ?? false;
  }

  push(item: T): void {
    if (this.closed) {
      return;
    }

    const waiter = this.waiters.shift();
    if (waiter !== undefined) {
      waiter.settled = true;
      waiter.hasValue = true;
      waiter.value = item;
      waiter.resolve(item);
      return;
    }

    if (this.maxLen !== undefined && this.items.length >= this.maxLen) {
      if (this.dropOldest) {
        this.items.shift();
      } else {
        return;
      }
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
      if (waiter !== undefined) {
        waiter.settled = true;
        waiter.hasValue = true;
        waiter.value = null;
        waiter.resolve(null);
      }
    }
  }

  shiftNow(): T | undefined {
    return this.items.shift();
  }

  shiftCancelable(): {
    promise: Promise<T | null>;
    cancel: () => void;
  } {
    const next = this.shiftNow();
    if (next !== undefined) {
      return {
        promise: Promise.resolve(next),
        cancel: () => {},
      };
    }

    if (this.closed) {
      return {
        promise: Promise.resolve(null),
        cancel: () => {},
      };
    }

    let waiter:
      | {
          canceled: boolean;
          hasValue: boolean;
          value: T | null;
          settled: boolean;
          resolve: (value: T | null) => void;
        }
      | undefined;
    const promise = new Promise<T | null>((resolve) => {
      waiter = {
        canceled: false,
        hasValue: false,
        value: null,
        settled: false,
        resolve,
      };
      this.waiters.push(waiter);
    });

    const cancel = (): void => {
      if (waiter === undefined || waiter.canceled) {
        return;
      }
      waiter.canceled = true;
      if (!waiter.settled) {
        const index = this.waiters.indexOf(waiter);
        if (index >= 0) {
          this.waiters.splice(index, 1);
        }
        return;
      }
      if (waiter.hasValue && waiter.value !== null && !this.closed) {
        this.items.unshift(waiter.value);
        waiter.hasValue = false;
        waiter.value = null;
      }
    };

    return { promise, cancel };
  }

  async shift(): Promise<T | null> {
    const { promise } = this.shiftCancelable();
    return await promise;
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
