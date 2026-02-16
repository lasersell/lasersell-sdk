import {
  type PositionSelectorInput,
  StreamClient,
  type StreamConfigure,
  type StreamConnection,
  type StreamSender,
} from "./client.js";
import type { ServerMessage } from "./proto.js";

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

  private constructor(connection: StreamConnection) {
    this.connection = connection;
  }

  static async connect(
    client: StreamClient,
    configure: StreamConfigure,
  ): Promise<StreamSession> {
    const connection = await client.connect(configure);
    return StreamSession.fromConnection(connection);
  }

  static fromConnection(connection: StreamConnection): StreamSession {
    return new StreamSession(connection);
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

  close(handle: PositionHandle): void {
    this.sender().closePosition(positionHandleToSelector(handle));
  }

  requestExitSignal(handle: PositionHandle, slippage_bps?: number): void {
    this.sender().requestExitSignal(positionHandleToSelector(handle), slippage_bps);
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
}

function positionHandleToSelector(handle: PositionHandle): PositionSelectorInput {
  return {
    token_account: handle.token_account,
  };
}
