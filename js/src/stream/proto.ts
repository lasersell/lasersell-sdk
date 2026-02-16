export type MarketTypeMsg =
  | "pump_fun"
  | "pump_swap"
  | "meteora_dbc"
  | "meteora_damm_v2"
  | "raydium_launchpad"
  | "raydium_cpmm";

export interface PumpFunContextMsg {}

export interface PumpSwapContextMsg {
  pool: string;
  global_config?: string;
}

export interface MeteoraDbcContextMsg {
  pool: string;
  config: string;
  quote_mint: string;
}

export interface MeteoraDammV2ContextMsg {
  pool: string;
}

export interface RaydiumLaunchpadContextMsg {
  pool: string;
  config: string;
  platform: string;
  quote_mint: string;
  user_quote_account: string;
}

export interface RaydiumCpmmContextMsg {
  pool: string;
  config: string;
  quote_mint: string;
  user_quote_account: string;
}

export interface MarketContextMsg {
  market_type: MarketTypeMsg;
  pumpfun?: PumpFunContextMsg;
  pumpswap?: PumpSwapContextMsg;
  meteora_dbc?: MeteoraDbcContextMsg;
  meteora_damm_v2?: MeteoraDammV2ContextMsg;
  raydium_launchpad?: RaydiumLaunchpadContextMsg;
  raydium_cpmm?: RaydiumCpmmContextMsg;
}

export interface StrategyConfigMsg {
  target_profit_pct: number;
  stop_loss_pct: number;
  deadline_timeout_sec: number;
}

export interface LimitsMsg {
  hi_capacity: number;
  pnl_flush_ms: number;
  max_positions_per_session: number;
  max_wallets_per_session: number;
  max_positions_per_wallet: number;
  max_sessions_per_api_key: number;
}

export interface PingClientMessage {
  type: "ping";
  client_time_ms: number;
}

export interface ConfigureClientMessage {
  type: "configure";
  wallet_pubkeys: string[];
  strategy: StrategyConfigMsg;
}

export interface UpdateStrategyClientMessage {
  type: "update_strategy";
  strategy: StrategyConfigMsg;
}

export interface ClosePositionClientMessage {
  type: "close_position";
  position_id?: number;
  token_account?: string;
}

export interface RequestExitSignalClientMessage {
  type: "request_exit_signal";
  position_id?: number;
  token_account?: string;
  slippage_bps?: number;
}

export type ClientMessage =
  | PingClientMessage
  | ConfigureClientMessage
  | UpdateStrategyClientMessage
  | ClosePositionClientMessage
  | RequestExitSignalClientMessage;

export interface HelloOkServerMessage {
  type: "hello_ok";
  session_id: number;
  server_time_ms: number;
  limits: LimitsMsg;
}

export interface PongServerMessage {
  type: "pong";
  server_time_ms: number;
}

export interface ErrorServerMessage {
  type: "error";
  code: string;
  message: string;
}

export interface PnlUpdateServerMessage {
  type: "pnl_update";
  position_id: number;
  profit_units: number;
  proceeds_units: number;
  server_time_ms: number;
}

export interface BalanceUpdateServerMessage {
  type: "balance_update";
  wallet_pubkey: string;
  mint: string;
  token_account?: string;
  token_program?: string;
  tokens: number;
  slot: number;
}

export interface PositionOpenedServerMessage {
  type: "position_opened";
  position_id: number;
  wallet_pubkey: string;
  mint: string;
  token_account: string;
  token_program?: string;
  tokens: number;
  entry_quote_units: number;
  market_context?: MarketContextMsg;
  slot: number;
}

export interface PositionClosedServerMessage {
  type: "position_closed";
  position_id: number;
  wallet_pubkey: string;
  mint: string;
  token_account?: string;
  reason: string;
  slot: number;
}

export interface ExitSignalWithTxServerMessage {
  type: "exit_signal_with_tx";
  session_id: number;
  position_id: number;
  wallet_pubkey: string;
  mint: string;
  token_account?: string;
  token_program?: string;
  position_tokens: number;
  profit_units: number;
  reason: string;
  triggered_at_ms: number;
  market_context?: MarketContextMsg;
  unsigned_tx_b64: string;
}

export type ServerMessage =
  | HelloOkServerMessage
  | PongServerMessage
  | ErrorServerMessage
  | PnlUpdateServerMessage
  | BalanceUpdateServerMessage
  | PositionOpenedServerMessage
  | PositionClosedServerMessage
  | ExitSignalWithTxServerMessage;

export function clientMessageFromText(text: string): ClientMessage {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    throw new Error(`decode client message JSON: ${stringifyError(error)}`);
  }
  return clientMessageFromUnknown(parsed);
}

export function clientMessageFromUnknown(value: unknown): ClientMessage {
  const obj = asRecord(value, "client message");
  const type = asString(obj.type, "client message.type");

  switch (type) {
    case "ping": {
      return {
        type: "ping",
        client_time_ms: asNumber(obj.client_time_ms, "ping.client_time_ms"),
      };
    }
    case "configure": {
      const wallet_pubkeys = parseWalletPubkeys(obj);
      return {
        type: "configure",
        wallet_pubkeys,
        strategy: parseStrategyConfig(obj.strategy),
      };
    }
    case "update_strategy": {
      return {
        type: "update_strategy",
        strategy: parseStrategyConfig(obj.strategy),
      };
    }
    case "close_position": {
      return {
        type: "close_position",
        position_id: optionalNumber(obj.position_id, "close_position.position_id"),
        token_account: optionalString(
          obj.token_account,
          "close_position.token_account",
        ),
      };
    }
    case "sell_now":
    case "request_exit_signal": {
      return {
        type: "request_exit_signal",
        position_id: optionalNumber(
          obj.position_id,
          "request_exit_signal.position_id",
        ),
        token_account: optionalString(
          obj.token_account,
          "request_exit_signal.token_account",
        ),
        slippage_bps: optionalNumber(
          obj.slippage_bps,
          "request_exit_signal.slippage_bps",
        ),
      };
    }
    default:
      throw new Error(`unsupported client message type: ${type}`);
  }
}

export function clientMessageToText(message: ClientMessage): string {
  return JSON.stringify(message);
}

export function serverMessageFromText(text: string): ServerMessage {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    throw new Error(`decode server message JSON: ${stringifyError(error)}`);
  }
  return serverMessageFromUnknown(parsed);
}

export function serverMessageFromUnknown(value: unknown): ServerMessage {
  const obj = asRecord(value, "server message");
  const type = asString(obj.type, "server message.type");

  switch (type) {
    case "hello_ok": {
      return {
        type: "hello_ok",
        session_id: asNumber(obj.session_id, "hello_ok.session_id"),
        server_time_ms: asNumber(obj.server_time_ms, "hello_ok.server_time_ms"),
        limits: parseLimits(obj.limits),
      };
    }
    case "pong": {
      return {
        type: "pong",
        server_time_ms: asNumber(obj.server_time_ms, "pong.server_time_ms"),
      };
    }
    case "error": {
      return {
        type: "error",
        code: asString(obj.code, "error.code"),
        message: asString(obj.message, "error.message"),
      };
    }
    case "pnl_update": {
      return {
        type: "pnl_update",
        position_id: asNumber(obj.position_id, "pnl_update.position_id"),
        profit_units: asNumber(obj.profit_units, "pnl_update.profit_units"),
        proceeds_units: asNumber(obj.proceeds_units, "pnl_update.proceeds_units"),
        server_time_ms: asNumber(obj.server_time_ms, "pnl_update.server_time_ms"),
      };
    }
    case "balance_update": {
      return {
        type: "balance_update",
        wallet_pubkey: asString(obj.wallet_pubkey, "balance_update.wallet_pubkey"),
        mint: asString(obj.mint, "balance_update.mint"),
        token_account: optionalString(
          obj.token_account,
          "balance_update.token_account",
        ),
        token_program: optionalString(
          obj.token_program,
          "balance_update.token_program",
        ),
        tokens: asNumber(obj.tokens, "balance_update.tokens"),
        slot: asNumber(obj.slot, "balance_update.slot"),
      };
    }
    case "position_opened": {
      return {
        type: "position_opened",
        position_id: asNumber(obj.position_id, "position_opened.position_id"),
        wallet_pubkey: asString(
          obj.wallet_pubkey,
          "position_opened.wallet_pubkey",
        ),
        mint: asString(obj.mint, "position_opened.mint"),
        token_account: asString(obj.token_account, "position_opened.token_account"),
        token_program: optionalString(
          obj.token_program,
          "position_opened.token_program",
        ),
        tokens: asNumber(obj.tokens, "position_opened.tokens"),
        entry_quote_units: asNumber(
          obj.entry_quote_units,
          "position_opened.entry_quote_units",
        ),
        market_context: optionalMarketContext(
          obj.market_context,
          "position_opened.market_context",
        ),
        slot: asNumber(obj.slot, "position_opened.slot"),
      };
    }
    case "position_closed": {
      return {
        type: "position_closed",
        position_id: asNumber(obj.position_id, "position_closed.position_id"),
        wallet_pubkey: asString(
          obj.wallet_pubkey,
          "position_closed.wallet_pubkey",
        ),
        mint: asString(obj.mint, "position_closed.mint"),
        token_account: optionalString(
          obj.token_account,
          "position_closed.token_account",
        ),
        reason: asString(obj.reason, "position_closed.reason"),
        slot: asNumber(obj.slot, "position_closed.slot"),
      };
    }
    case "exit_signal_with_tx": {
      return {
        type: "exit_signal_with_tx",
        session_id: asNumber(obj.session_id, "exit_signal_with_tx.session_id"),
        position_id: asNumber(obj.position_id, "exit_signal_with_tx.position_id"),
        wallet_pubkey: asString(
          obj.wallet_pubkey,
          "exit_signal_with_tx.wallet_pubkey",
        ),
        mint: asString(obj.mint, "exit_signal_with_tx.mint"),
        token_account: optionalString(
          obj.token_account,
          "exit_signal_with_tx.token_account",
        ),
        token_program: optionalString(
          obj.token_program,
          "exit_signal_with_tx.token_program",
        ),
        position_tokens: asNumber(
          obj.position_tokens,
          "exit_signal_with_tx.position_tokens",
        ),
        profit_units: asNumber(obj.profit_units, "exit_signal_with_tx.profit_units"),
        reason: asString(obj.reason, "exit_signal_with_tx.reason"),
        triggered_at_ms: asNumber(
          obj.triggered_at_ms,
          "exit_signal_with_tx.triggered_at_ms",
        ),
        market_context: optionalMarketContext(
          obj.market_context,
          "exit_signal_with_tx.market_context",
        ),
        unsigned_tx_b64: asString(
          obj.unsigned_tx_b64,
          "exit_signal_with_tx.unsigned_tx_b64",
        ),
      };
    }
    default:
      throw new Error(`unsupported server message type: ${type}`);
  }
}

export function serverMessageToText(message: ServerMessage): string {
  return JSON.stringify(message);
}

function parseWalletPubkeys(obj: Record<string, unknown>): string[] {
  const legacyWalletPubkey = obj.wallet_pubkey;
  const wallets = obj.wallet_pubkeys ?? legacyWalletPubkey;

  if (typeof wallets === "string") {
    return [wallets];
  }

  if (Array.isArray(wallets)) {
    const parsed = wallets.map((item, idx) =>
      asString(item, `configure.wallet_pubkeys[${idx}]`),
    );
    return parsed;
  }

  throw new Error(
    "configure.wallet_pubkeys must be a string or string[] (legacy wallet_pubkey is supported)",
  );
}

function parseStrategyConfig(value: unknown): StrategyConfigMsg {
  const obj = asRecord(value, "strategy");

  return {
    target_profit_pct: asNumber(
      obj.target_profit_pct,
      "strategy.target_profit_pct",
    ),
    stop_loss_pct: asNumber(obj.stop_loss_pct, "strategy.stop_loss_pct"),
    deadline_timeout_sec: asNumber(
      obj.deadline_timeout_sec,
      "strategy.deadline_timeout_sec",
    ),
  };
}

function parseLimits(value: unknown): LimitsMsg {
  const obj = asRecord(value, "limits");

  return {
    hi_capacity: asNumber(obj.hi_capacity, "limits.hi_capacity"),
    pnl_flush_ms: asNumber(obj.pnl_flush_ms, "limits.pnl_flush_ms"),
    max_positions_per_session: asNumber(
      obj.max_positions_per_session,
      "limits.max_positions_per_session",
    ),
    max_wallets_per_session: optionalNumber(
      obj.max_wallets_per_session,
      "limits.max_wallets_per_session",
    ) ?? 0,
    max_positions_per_wallet:
      optionalNumber(
        obj.max_positions_per_wallet,
        "limits.max_positions_per_wallet",
      ) ?? 0,
    max_sessions_per_api_key:
      optionalNumber(
        obj.max_sessions_per_api_key,
        "limits.max_sessions_per_api_key",
      ) ?? 0,
  };
}

function optionalMarketContext(
  value: unknown,
  path: string,
): MarketContextMsg | undefined {
  if (value === undefined || value === null) {
    return undefined;
  }

  return parseMarketContext(value, path);
}

function parseMarketContext(value: unknown, path: string): MarketContextMsg {
  const obj = asRecord(value, path);

  return {
    market_type: parseMarketType(obj.market_type, `${path}.market_type`),
    pumpfun: optionalObject(obj.pumpfun, `${path}.pumpfun`),
    pumpswap: optionalObject(obj.pumpswap, `${path}.pumpswap`),
    meteora_dbc: optionalObject(obj.meteora_dbc, `${path}.meteora_dbc`),
    meteora_damm_v2: optionalObject(
      obj.meteora_damm_v2,
      `${path}.meteora_damm_v2`,
    ),
    raydium_launchpad: optionalObject(
      obj.raydium_launchpad,
      `${path}.raydium_launchpad`,
    ),
    raydium_cpmm: optionalObject(obj.raydium_cpmm, `${path}.raydium_cpmm`),
  };
}

function parseMarketType(value: unknown, path: string): MarketTypeMsg {
  const str = asString(value, path);
  switch (str) {
    case "pump_fun":
    case "pump_swap":
    case "meteora_dbc":
    case "meteora_damm_v2":
    case "raydium_launchpad":
    case "raydium_cpmm":
      return str;
    default:
      throw new Error(`invalid market type at ${path}: ${str}`);
  }
}

function optionalString(value: unknown, path: string): string | undefined {
  if (value === undefined || value === null) {
    return undefined;
  }
  return asString(value, path);
}

function optionalNumber(value: unknown, path: string): number | undefined {
  if (value === undefined || value === null) {
    return undefined;
  }
  return asNumber(value, path);
}

function optionalObject<T extends object>(
  value: unknown,
  path: string,
): T | undefined {
  if (value === undefined || value === null) {
    return undefined;
  }
  return asRecord(value, path) as T;
}

function asRecord(value: unknown, path: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`${path} must be an object`);
  }
  return value as Record<string, unknown>;
}

function asString(value: unknown, path: string): string {
  if (typeof value !== "string") {
    throw new Error(`${path} must be a string`);
  }
  return value;
}

function asNumber(value: unknown, path: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new Error(`${path} must be a finite number`);
  }
  return value;
}

function stringifyError(error: unknown): string {
  if (error instanceof Error) {
    return `${error.name}: ${error.message}`;
  }
  return String(error);
}
