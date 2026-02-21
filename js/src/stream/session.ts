import {
  type PositionSelectorInput,
  StreamClient,
  type StreamConfigure,
  type StreamConnection,
  type StreamSender,
} from "./client.js";
import type { ServerMessage, StrategyConfigMsg } from "./proto.js";

export interface PositionHandle {
  position_id: number;
  token_account: string;
  wallet_pubkey: string;
  mint: string;
  token_program?: string;
  tokens: number;
  entry_quote_units: number;
}

export type StreamEvent =
  | {
      type: "message";
      message: ServerMessage;
    }
  | {
      type: "position_opened";
      handle: PositionHandle;
      message: ServerMessage;
    }
  | {
      type: "position_closed";
      handle?: PositionHandle;
      message: ServerMessage;
    }
  | {
      type: "exit_signal_with_tx";
      handle?: PositionHandle;
      message: ServerMessage;
    }
  | {
      type: "pnl_update";
      handle?: PositionHandle;
      message: ServerMessage;
    };

export class StreamSession {
  private readonly connection: StreamConnection;
  private readonly positionsById = new Map<number, PositionHandle>();
  private strategy: StrategyConfigMsg;
  private deadlineTimeoutSec: number;
  private readonly deadlineTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private readonly openedAtMs = new Map<string, number>();

  private constructor(
    connection: StreamConnection,
    strategy: StrategyConfigMsg,
    deadlineTimeoutSec: number,
  ) {
    this.connection = connection;
    this.strategy = { ...strategy };
    this.deadlineTimeoutSec = Math.max(0, deadlineTimeoutSec);
  }

  static async connect(
    client: StreamClient,
    configure: StreamConfigure,
  ): Promise<StreamSession> {
    const connection = await client.connect(configure);
    return StreamSession.fromConnection(
      connection,
      configure.strategy,
      configure.deadline_timeout_sec ?? 0,
    );
  }

  static fromConnection(
    connection: StreamConnection,
    strategy: StrategyConfigMsg = defaultStrategy(),
    deadlineTimeoutSec = 0,
  ): StreamSession {
    return new StreamSession(connection, strategy, deadlineTimeoutSec);
  }

  sender(): StreamSender {
    return this.connection.sender();
  }

  positions(): PositionHandle[] {
    return [...this.positionsById.values()].map((position) => ({ ...position }));
  }

  positionsForWalletMint(wallet: string, mint: string): PositionHandle[] {
    return this.positions()
      .filter(
        (position) =>
          position.wallet_pubkey === wallet && position.mint === mint,
      )
      .map((position) => ({ ...position }));
  }

  close(): void;
  close(handle: PositionHandle): void;
  close(handle?: PositionHandle): void {
    if (handle !== undefined) {
      this.sender().closePosition(positionHandleToSelector(handle));
      return;
    }
    this.cancelAllDeadlines();
    this.connection.close();
  }

  requestExitSignal(handle: PositionHandle, slippage_bps?: number): void {
    this.sender().requestExitSignal(positionHandleToSelector(handle), slippage_bps);
  }

  updateStrategy(
    strategy: StrategyConfigMsg,
    deadlineTimeoutSec = this.deadlineTimeoutSec,
  ): void {
    this.strategy = { ...strategy };
    this.deadlineTimeoutSec = Math.max(0, deadlineTimeoutSec);
    this.rescheduleAllDeadlines();
    this.sender().updateStrategy({ ...strategy });
  }

  async recv(): Promise<StreamEvent | null> {
    const message = await this.connection.recv();
    if (message === null) {
      return null;
    }

    return this.applyMessage(message);
  }

  private applyMessage(message: ServerMessage): StreamEvent {
    switch (message.type) {
      case "position_opened": {
        const handle: PositionHandle = {
          position_id: message.position_id,
          token_account: message.token_account,
          wallet_pubkey: message.wallet_pubkey,
          mint: message.mint,
          token_program: message.token_program,
          tokens: message.tokens,
          entry_quote_units: message.entry_quote_units,
        };

        this.positionsById.set(message.position_id, handle);
        this.armDeadline(handle.token_account);

        return {
          type: "position_opened",
          handle: { ...handle },
          message,
        };
      }
      case "position_closed": {
        const handle = this.removePosition(
          message.position_id,
          message.token_account,
        );
        const tokenAccount = handle?.token_account ?? message.token_account;
        if (tokenAccount !== undefined) {
          this.cancelDeadlineFor(tokenAccount);
        }

        return {
          type: "position_closed",
          handle: handle === undefined ? undefined : { ...handle },
          message,
        };
      }
      case "exit_signal_with_tx": {
        const handle = this.findPosition(message.position_id, message.token_account);

        return {
          type: "exit_signal_with_tx",
          handle: handle === undefined ? undefined : { ...handle },
          message,
        };
      }
      case "pnl_update": {
        const handle = this.findPosition(message.position_id);

        return {
          type: "pnl_update",
          handle: handle === undefined ? undefined : { ...handle },
          message,
        };
      }
      default: {
        return {
          type: "message",
          message,
        };
      }
    }
  }

  private findPosition(
    positionId: number,
    tokenAccount?: string,
  ): PositionHandle | undefined {
    const byId = this.positionsById.get(positionId);
    if (byId !== undefined) {
      return byId;
    }

    if (tokenAccount === undefined) {
      return undefined;
    }

    for (const handle of this.positionsById.values()) {
      if (handle.token_account === tokenAccount) {
        return handle;
      }
    }

    return undefined;
  }

  private removePosition(
    positionId: number,
    tokenAccount?: string,
  ): PositionHandle | undefined {
    const byId = this.positionsById.get(positionId);
    if (byId !== undefined) {
      this.positionsById.delete(positionId);
      return byId;
    }

    if (tokenAccount === undefined) {
      return undefined;
    }

    for (const [id, handle] of this.positionsById.entries()) {
      if (handle.token_account === tokenAccount) {
        this.positionsById.delete(id);
        return handle;
      }
    }

    return undefined;
  }

  private deadlineMs(): number {
    const raw = this.deadlineTimeoutSec;
    const sec = Number.isFinite(raw) ? Math.max(0, raw) : 0;
    return sec * 1_000;
  }

  private armDeadline(tokenAccount: string): void {
    this.cancelDeadlineTimer(tokenAccount);

    const deadlineMs = this.deadlineMs();
    const openedAt = Date.now();
    this.openedAtMs.set(tokenAccount, openedAt);
    if (deadlineMs === 0) {
      return;
    }

    this.scheduleDeadline(tokenAccount, deadlineMs);
  }

  private rescheduleAllDeadlines(): void {
    this.cancelAllDeadlineTimers();

    const deadlineMs = this.deadlineMs();
    if (deadlineMs === 0) {
      return;
    }

    const now = Date.now();
    const tokenAccounts = new Set(
      [...this.positionsById.values()].map((position) => position.token_account),
    );

    for (const tokenAccount of tokenAccounts) {
      const openedAt = this.openedAtMs.get(tokenAccount) ?? now;
      this.openedAtMs.set(tokenAccount, openedAt);
      const remaining = openedAt + deadlineMs - now;
      if (remaining <= 0) {
        queueMicrotask(() => {
          this.tryRequestExitSignal(tokenAccount);
        });
        continue;
      }
      this.scheduleDeadline(tokenAccount, remaining);
    }
  }

  private scheduleDeadline(tokenAccount: string, delayMs: number): void {
    const timer = setTimeout(() => {
      this.deadlineTimers.delete(tokenAccount);
      this.tryRequestExitSignal(tokenAccount);
    }, Math.max(0, delayMs));
    this.deadlineTimers.set(tokenAccount, timer);
  }

  private tryRequestExitSignal(tokenAccount: string): void {
    if (!this.hasOpenPositionForTokenAccount(tokenAccount)) {
      return;
    }

    try {
      this.sender().requestExitSignal(tokenAccount);
    } catch {
      // Ignore timer callback failures to avoid crashing caller loops.
    }
  }

  private hasOpenPositionForTokenAccount(tokenAccount: string): boolean {
    for (const handle of this.positionsById.values()) {
      if (handle.token_account === tokenAccount) {
        return true;
      }
    }
    return false;
  }

  private cancelDeadlineFor(tokenAccount: string): void {
    this.cancelDeadlineTimer(tokenAccount);
    this.openedAtMs.delete(tokenAccount);
  }

  private cancelDeadlineTimer(tokenAccount: string): void {
    const timer = this.deadlineTimers.get(tokenAccount);
    if (timer !== undefined) {
      clearTimeout(timer);
      this.deadlineTimers.delete(tokenAccount);
    }
  }

  private cancelAllDeadlineTimers(): void {
    for (const timer of this.deadlineTimers.values()) {
      clearTimeout(timer);
    }
    this.deadlineTimers.clear();
  }

  private cancelAllDeadlines(): void {
    this.cancelAllDeadlineTimers();
    this.openedAtMs.clear();
  }
}

function positionHandleToSelector(handle: PositionHandle): PositionSelectorInput {
  return {
    token_account: handle.token_account,
  };
}

function defaultStrategy(): StrategyConfigMsg {
  return {
    target_profit_pct: 0,
    stop_loss_pct: 0,
  };
}
