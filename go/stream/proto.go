package stream

import (
	"encoding/json"
	"fmt"
)

// MarketTypeMsg enumerates supported market backends.
type MarketTypeMsg string

const (
	MarketTypePumpFun          MarketTypeMsg = "pump_fun"
	MarketTypePumpSwap         MarketTypeMsg = "pump_swap"
	MarketTypeMeteoraDbc       MarketTypeMsg = "meteora_dbc"
	MarketTypeMeteoraDammV2    MarketTypeMsg = "meteora_damm_v2"
	MarketTypeRaydiumLaunchpad MarketTypeMsg = "raydium_launchpad"
	MarketTypeRaydiumCpmm      MarketTypeMsg = "raydium_cpmm"
)

// PumpFunContextMsg holds pumpfun market metadata.
type PumpFunContextMsg struct{}

// PumpSwapContextMsg holds pumpswap market metadata.
type PumpSwapContextMsg struct {
	Pool         string  `json:"pool"`
	GlobalConfig *string `json:"global_config,omitempty"`
}

// MeteoraDbcContextMsg holds meteora dbc market metadata.
type MeteoraDbcContextMsg struct {
	Pool      string `json:"pool"`
	Config    string `json:"config"`
	QuoteMint string `json:"quote_mint"`
}

// MeteoraDammV2ContextMsg holds meteora damm-v2 market metadata.
type MeteoraDammV2ContextMsg struct {
	Pool string `json:"pool"`
}

// RaydiumLaunchpadContextMsg holds raydium launchpad market metadata.
type RaydiumLaunchpadContextMsg struct {
	Pool             string `json:"pool"`
	Config           string `json:"config"`
	Platform         string `json:"platform"`
	QuoteMint        string `json:"quote_mint"`
	UserQuoteAccount string `json:"user_quote_account"`
}

// RaydiumCpmmContextMsg holds raydium cpmm market metadata.
type RaydiumCpmmContextMsg struct {
	Pool             string `json:"pool"`
	Config           string `json:"config"`
	QuoteMint        string `json:"quote_mint"`
	UserQuoteAccount string `json:"user_quote_account"`
}

// MarketContextMsg describes routing/market metadata for position and tx messages.
type MarketContextMsg struct {
	MarketType       MarketTypeMsg               `json:"market_type"`
	PumpFun          *PumpFunContextMsg          `json:"pumpfun,omitempty"`
	PumpSwap         *PumpSwapContextMsg         `json:"pumpswap,omitempty"`
	MeteoraDbc       *MeteoraDbcContextMsg       `json:"meteora_dbc,omitempty"`
	MeteoraDammV2    *MeteoraDammV2ContextMsg    `json:"meteora_damm_v2,omitempty"`
	RaydiumLaunchpad *RaydiumLaunchpadContextMsg `json:"raydium_launchpad,omitempty"`
	RaydiumCpmm      *RaydiumCpmmContextMsg      `json:"raydium_cpmm,omitempty"`
}

// TakeProfitLevelMsg describes a single take-profit ladder level.
type TakeProfitLevelMsg struct {
	TriggerPct     float64 `json:"trigger_pct"`
	SellPct        float64 `json:"sell_pct"`
	TrailingAdjPct float64 `json:"trailing_adj_pct"`
}

// StrategyConfigMsg defines stream-side exit strategy parameters.
type StrategyConfigMsg struct {
	TargetProfitPct   float64              `json:"target_profit_pct"`
	StopLossPct       float64              `json:"stop_loss_pct"`
	TrailingStopPct   float64              `json:"trailing_stop_pct,omitempty"`
	SellOnGraduation  bool                 `json:"sell_on_graduation,omitempty"`
	TakeProfitLevels  []TakeProfitLevelMsg `json:"take_profit_levels,omitempty"`
	LiquidityGuard    bool                 `json:"liquidity_guard,omitempty"`
	BreakevenTrailPct float64              `json:"breakeven_trail_pct,omitempty"`
}

// Exit reason constants for position close/exit messages.
const (
	ExitReasonTargetProfit = "target_profit"
	ExitReasonStopLoss     = "stop_loss"
	ExitReasonTrailingStop = "trailing_stop"
	ExitReasonSellNow      = "sell_now"
	ExitReasonGraduation    = "graduation"
	ExitReasonTimeout      = "timeout"
)

// LimitsMsg captures server-advertised session limits.
type LimitsMsg struct {
	HiCapacity             uint32 `json:"hi_capacity"`
	PnlFlushMS             uint64 `json:"pnl_flush_ms"`
	MaxPositionsPerSession uint32 `json:"max_positions_per_session"`
	MaxWalletsPerSession   uint32 `json:"max_wallets_per_session"`
	MaxPositionsPerWallet  uint32 `json:"max_positions_per_wallet"`
	MaxSessionsPerAPIKey       uint32 `json:"max_sessions_per_api_key"`
	MaxWatchWalletsPerSession  uint32 `json:"max_watch_wallets_per_session"`
}

// AutoBuyConfigMsg configures auto-buy parameters for a watched wallet.
type AutoBuyConfigMsg struct {
	WalletPubkey     string  `json:"wallet_pubkey"`
	AmountQuoteUnits uint64  `json:"amount_quote_units"`
	AmountUsd1Units  *uint64 `json:"amount_usd1_units,omitempty"`
}

// WatchWalletEntryMsg is a single watched wallet entry with optional auto-buy config.
type WatchWalletEntryMsg struct {
	Pubkey  string            `json:"pubkey"`
	AutoBuy *AutoBuyConfigMsg `json:"auto_buy,omitempty"`
}

// ClientMessageType is the stream protocol discriminator for client messages.
type ClientMessageType string

const (
	ClientMessageTypePing              ClientMessageType = "ping"
	ClientMessageTypeConfigure         ClientMessageType = "configure"
	ClientMessageTypeUpdateStrategy    ClientMessageType = "update_strategy"
	ClientMessageTypeClosePosition     ClientMessageType = "close_position"
	ClientMessageTypeRequestExitSignal ClientMessageType = "request_exit_signal"
	ClientMessageTypeUpdateWallets      ClientMessageType = "update_wallets"
	ClientMessageTypeUpdateWatchWallets      ClientMessageType = "update_watch_wallets"
	ClientMessageTypeUpdatePositionStrategy ClientMessageType = "UpdatePositionStrategy"
)

// ClientMessage is a tagged client command payload.
type ClientMessage interface {
	ClientMessageType() ClientMessageType
}

// PingClientMessage sends a heartbeat ping.
type PingClientMessage struct {
	ClientTimeMS uint64 `json:"client_time_ms"`
}

func (PingClientMessage) ClientMessageType() ClientMessageType { return ClientMessageTypePing }

// ConfigureClientMessage configures wallets and strategy after hello.
type ConfigureClientMessage struct {
	WalletPubkeys []string              `json:"wallet_pubkeys"`
	Strategy      StrategyConfigMsg     `json:"strategy"`
	SendMode      *string               `json:"send_mode,omitempty"`
	TipLamports   *uint64               `json:"tip_lamports,omitempty"`
	WatchWallets  []WatchWalletEntryMsg `json:"watch_wallets,omitempty"`
}

func (ConfigureClientMessage) ClientMessageType() ClientMessageType {
	return ClientMessageTypeConfigure
}

// UpdateStrategyClientMessage updates strategy parameters.
type UpdateStrategyClientMessage struct {
	Strategy StrategyConfigMsg `json:"strategy"`
}

func (UpdateStrategyClientMessage) ClientMessageType() ClientMessageType {
	return ClientMessageTypeUpdateStrategy
}

// ClosePositionClientMessage requests position close by id or token account.
type ClosePositionClientMessage struct {
	PositionID   *uint64 `json:"position_id,omitempty"`
	TokenAccount *string `json:"token_account,omitempty"`
}

func (ClosePositionClientMessage) ClientMessageType() ClientMessageType {
	return ClientMessageTypeClosePosition
}

// RequestExitSignalClientMessage requests exit signal generation.
type RequestExitSignalClientMessage struct {
	PositionID   *uint64 `json:"position_id,omitempty"`
	TokenAccount *string `json:"token_account,omitempty"`
	SlippageBps  *uint16 `json:"slippage_bps,omitempty"`
}

func (RequestExitSignalClientMessage) ClientMessageType() ClientMessageType {
	return ClientMessageTypeRequestExitSignal
}

// UpdateWalletsClientMessage replaces the session wallet set.
type UpdateWalletsClientMessage struct {
	WalletPubkeys []string `json:"wallet_pubkeys"`
}

func (UpdateWalletsClientMessage) ClientMessageType() ClientMessageType {
	return ClientMessageTypeUpdateWallets
}

// UpdateWatchWalletsClientMessage replaces the set of watched wallets for copy trading.
type UpdateWatchWalletsClientMessage struct {
	WatchWallets []WatchWalletEntryMsg `json:"watch_wallets"`
}

func (UpdateWatchWalletsClientMessage) ClientMessageType() ClientMessageType {
	return ClientMessageTypeUpdateWatchWallets
}

// UpdatePositionStrategyClientMessage updates strategy parameters for a single position.
type UpdatePositionStrategyClientMessage struct {
	PositionID uint64            `json:"position_id"`
	Strategy   StrategyConfigMsg `json:"strategy"`
}

func (UpdatePositionStrategyClientMessage) ClientMessageType() ClientMessageType {
	return ClientMessageTypeUpdatePositionStrategy
}

// ServerMessageType is the stream protocol discriminator for server messages.
type ServerMessageType string

const (
	ServerMessageTypeHelloOk          ServerMessageType = "hello_ok"
	ServerMessageTypePong             ServerMessageType = "pong"
	ServerMessageTypeError            ServerMessageType = "error"
	ServerMessageTypePnlUpdate        ServerMessageType = "pnl_update"
	ServerMessageTypeBalanceUpdate    ServerMessageType = "balance_update"
	ServerMessageTypePositionOpened   ServerMessageType = "position_opened"
	ServerMessageTypePositionClosed   ServerMessageType = "position_closed"
	ServerMessageTypeExitSignalWithTx    ServerMessageType = "exit_signal_with_tx"
	ServerMessageTypeLiquiditySnapshot ServerMessageType = "liquidity_snapshot"
	ServerMessageTypeTradeTick         ServerMessageType = "trade_tick"
)

// ServerMessage is a tagged stream event payload.
type ServerMessage interface {
	ServerMessageType() ServerMessageType
}

// HelloOkServerMessage is emitted as first server message.
type HelloOkServerMessage struct {
	SessionID    uint64    `json:"session_id"`
	ServerTimeMS uint64    `json:"server_time_ms"`
	Limits       LimitsMsg `json:"limits"`
}

func (HelloOkServerMessage) ServerMessageType() ServerMessageType { return ServerMessageTypeHelloOk }

// PongServerMessage responds to ping.
type PongServerMessage struct {
	ServerTimeMS uint64 `json:"server_time_ms"`
}

func (PongServerMessage) ServerMessageType() ServerMessageType { return ServerMessageTypePong }

// ErrorServerMessage reports protocol-level failures.
type ErrorServerMessage struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

func (ErrorServerMessage) ServerMessageType() ServerMessageType { return ServerMessageTypeError }

// PnlUpdateServerMessage reports PnL for an active position.
type PnlUpdateServerMessage struct {
	PositionID      uint64  `json:"position_id"`
	ProfitUnits     int64   `json:"profit_units"`
	ProceedsUnits   uint64  `json:"proceeds_units"`
	ServerTimeMS    uint64  `json:"server_time_ms"`
	TokenPriceQuote *uint64 `json:"token_price_quote,omitempty"`
	MarketCapQuote  *uint64 `json:"market_cap_quote,omitempty"`
	Watched         bool    `json:"watched,omitempty"`
}

func (PnlUpdateServerMessage) ServerMessageType() ServerMessageType {
	return ServerMessageTypePnlUpdate
}

// BalanceUpdateServerMessage reports token balance changes.
type BalanceUpdateServerMessage struct {
	WalletPubkey string  `json:"wallet_pubkey"`
	Mint         string  `json:"mint"`
	TokenAccount *string `json:"token_account,omitempty"`
	TokenProgram *string `json:"token_program,omitempty"`
	Tokens       uint64  `json:"tokens"`
	Slot         uint64  `json:"slot"`
}

func (BalanceUpdateServerMessage) ServerMessageType() ServerMessageType {
	return ServerMessageTypeBalanceUpdate
}

// PositionOpenedServerMessage reports newly opened positions.
type PositionOpenedServerMessage struct {
	PositionID         uint64            `json:"position_id"`
	WalletPubkey       string            `json:"wallet_pubkey"`
	Mint               string            `json:"mint"`
	TokenAccount       string            `json:"token_account"`
	TokenProgram       *string           `json:"token_program,omitempty"`
	Tokens             uint64            `json:"tokens"`
	EntryQuoteUnits    uint64            `json:"entry_quote_units"`
	MarketContext      *MarketContextMsg `json:"market_context,omitempty"`
	Slot               uint64            `json:"slot"`
	TokenName          *string           `json:"token_name,omitempty"`
	TokenSymbol        *string           `json:"token_symbol,omitempty"`
	TokenDecimals      *uint8            `json:"token_decimals,omitempty"`
	TokenPriceQuote    *uint64           `json:"token_price_quote,omitempty"`
	MarketCapQuote     *uint64           `json:"market_cap_quote,omitempty"`
	PoolLiquidityQuote *uint64           `json:"pool_liquidity_quote,omitempty"`
	OpenedAtMs         *uint64           `json:"opened_at_ms,omitempty"`
	MirrorSource       *string           `json:"mirror_source,omitempty"`
	Watched            bool              `json:"watched,omitempty"`
}

func (PositionOpenedServerMessage) ServerMessageType() ServerMessageType {
	return ServerMessageTypePositionOpened
}

// PositionClosedServerMessage reports position closure.
type PositionClosedServerMessage struct {
	PositionID   uint64  `json:"position_id"`
	WalletPubkey string  `json:"wallet_pubkey"`
	Mint         string  `json:"mint"`
	TokenAccount *string `json:"token_account,omitempty"`
	Reason       string  `json:"reason"`
	Slot         uint64  `json:"slot"`
	MirrorSource *string `json:"mirror_source,omitempty"`
	Watched      bool    `json:"watched,omitempty"`
}

func (PositionClosedServerMessage) ServerMessageType() ServerMessageType {
	return ServerMessageTypePositionClosed
}

// ExitSignalWithTxServerMessage carries unsigned tx payload for exits.
type ExitSignalWithTxServerMessage struct {
	SessionID      uint64            `json:"session_id"`
	PositionID     uint64            `json:"position_id"`
	WalletPubkey   string            `json:"wallet_pubkey"`
	Mint           string            `json:"mint"`
	TokenAccount   *string           `json:"token_account,omitempty"`
	TokenProgram   *string           `json:"token_program,omitempty"`
	PositionTokens uint64            `json:"position_tokens"`
	ProfitUnits    int64             `json:"profit_units"`
	Reason         string            `json:"reason"`
	TriggeredAtMS  uint64            `json:"triggered_at_ms"`
	MarketContext  *MarketContextMsg `json:"market_context,omitempty"`
	UnsignedTxB64  string            `json:"unsigned_tx_b64"`
	SellTokens     *uint64           `json:"sell_tokens,omitempty"`
	LevelIndex     *uint32           `json:"level_index,omitempty"`
	MirrorSource   *string           `json:"mirror_source,omitempty"`
	Watched        bool              `json:"watched,omitempty"`
}

func (ExitSignalWithTxServerMessage) ServerMessageType() ServerMessageType {
	return ServerMessageTypeExitSignalWithTx
}

// SlippageBandMsg describes the maximum sellable amount at a given slippage threshold.
type SlippageBandMsg struct {
	SlippageBps uint16  `json:"slippage_bps"`
	MaxTokens   uint64  `json:"max_tokens"`
	CoveragePct float64 `json:"coverage_pct"`
}

// LiquiditySnapshotServerMessage carries slippage band data for a position.
type LiquiditySnapshotServerMessage struct {
	PositionID     uint64            `json:"position_id"`
	Bands          []SlippageBandMsg `json:"bands"`
	LiquidityTrend string            `json:"liquidity_trend"`
	ServerTimeMS   uint64            `json:"server_time_ms"`
	Watched        bool              `json:"watched,omitempty"`
}

func (LiquiditySnapshotServerMessage) ServerMessageType() ServerMessageType {
	return ServerMessageTypeLiquiditySnapshot
}

// TradeTickServerMessage reports a trade tick for a position.
type TradeTickServerMessage struct {
	PositionID  uint64  `json:"position_id"`
	TimeMS      uint64  `json:"time_ms"`
	Side        string  `json:"side"`
	TokenAmount uint64  `json:"token_amount"`
	QuoteAmount uint64  `json:"quote_amount"`
	PriceQuote  uint64  `json:"price_quote"`
	Maker       *string `json:"maker,omitempty"`
	TxSignature *string `json:"tx_signature,omitempty"`
	Watched     bool    `json:"watched,omitempty"`
}

func (TradeTickServerMessage) ServerMessageType() ServerMessageType {
	return ServerMessageTypeTradeTick
}

// ClientMessageFromText decodes a client message JSON string.
func ClientMessageFromText(text string) (ClientMessage, error) {
	return ClientMessageFromJSON([]byte(text))
}

// ClientMessageFromJSON decodes a client message JSON payload.
func ClientMessageFromJSON(data []byte) (ClientMessage, error) {
	var envelope struct {
		Type string `json:"type"`
	}
	if err := json.Unmarshal(data, &envelope); err != nil {
		return nil, fmt.Errorf("decode client message JSON: %w", err)
	}

	switch envelope.Type {
	case string(ClientMessageTypePing):
		var msg PingClientMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode ping client message: %w", err)
		}
		return msg, nil
	case string(ClientMessageTypeConfigure):
		return decodeConfigureClientMessage(data)
	case string(ClientMessageTypeUpdateStrategy):
		var msg UpdateStrategyClientMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode update_strategy client message: %w", err)
		}
		return msg, nil
	case string(ClientMessageTypeClosePosition):
		var msg ClosePositionClientMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode close_position client message: %w", err)
		}
		return msg, nil
	case "sell_now", string(ClientMessageTypeRequestExitSignal):
		var msg RequestExitSignalClientMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode request_exit_signal client message: %w", err)
		}
		return msg, nil
	case string(ClientMessageTypeUpdateWallets):
		var msg UpdateWalletsClientMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode update_wallets client message: %w", err)
		}
		return msg, nil
	case string(ClientMessageTypeUpdateWatchWallets):
		var msg UpdateWatchWalletsClientMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode update_watch_wallets client message: %w", err)
		}
		return msg, nil
	case string(ClientMessageTypeUpdatePositionStrategy):
		var msg UpdatePositionStrategyClientMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode UpdatePositionStrategy client message: %w", err)
		}
		return msg, nil
	default:
		return nil, fmt.Errorf("unsupported client message type: %s", envelope.Type)
	}
}

func decodeConfigureClientMessage(data []byte) (ClientMessage, error) {
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return nil, fmt.Errorf("decode configure client message: %w", err)
	}

	var strategy StrategyConfigMsg
	if err := json.Unmarshal(raw["strategy"], &strategy); err != nil {
		return nil, fmt.Errorf("decode configure.strategy: %w", err)
	}

	walletRaw := raw["wallet_pubkeys"]
	if len(walletRaw) == 0 {
		walletRaw = raw["wallet_pubkey"]
	}
	if len(walletRaw) == 0 {
		return nil, fmt.Errorf("configure.wallet_pubkeys must be present")
	}

	walletPubkeys, err := decodeWalletPubkeys(walletRaw)
	if err != nil {
		return nil, err
	}

	msg := ConfigureClientMessage{
		WalletPubkeys: walletPubkeys,
		Strategy:      strategy,
	}

	if sendModeRaw, ok := raw["send_mode"]; ok {
		var sendMode string
		if err := json.Unmarshal(sendModeRaw, &sendMode); err == nil {
			msg.SendMode = &sendMode
		}
	}
	if tipRaw, ok := raw["tip_lamports"]; ok {
		var tipLamports uint64
		if err := json.Unmarshal(tipRaw, &tipLamports); err == nil {
			msg.TipLamports = &tipLamports
		}
	}

	return msg, nil
}

func decodeWalletPubkeys(raw json.RawMessage) ([]string, error) {
	var one string
	if err := json.Unmarshal(raw, &one); err == nil {
		return []string{one}, nil
	}

	var many []string
	if err := json.Unmarshal(raw, &many); err == nil {
		return many, nil
	}

	return nil, fmt.Errorf("configure.wallet_pubkeys must be a string or []string")
}

// ClientMessageToJSON encodes a client message to JSON bytes.
func ClientMessageToJSON(message ClientMessage) ([]byte, error) {
	switch msg := message.(type) {
	case PingClientMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			PingClientMessage
		}{Type: string(ClientMessageTypePing), PingClientMessage: msg})
	case *PingClientMessage:
		if msg == nil {
			return nil, fmt.Errorf("client message cannot be nil")
		}
		return ClientMessageToJSON(*msg)
	case ConfigureClientMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			ConfigureClientMessage
		}{Type: string(ClientMessageTypeConfigure), ConfigureClientMessage: msg})
	case *ConfigureClientMessage:
		if msg == nil {
			return nil, fmt.Errorf("client message cannot be nil")
		}
		return ClientMessageToJSON(*msg)
	case UpdateStrategyClientMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			UpdateStrategyClientMessage
		}{Type: string(ClientMessageTypeUpdateStrategy), UpdateStrategyClientMessage: msg})
	case *UpdateStrategyClientMessage:
		if msg == nil {
			return nil, fmt.Errorf("client message cannot be nil")
		}
		return ClientMessageToJSON(*msg)
	case ClosePositionClientMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			ClosePositionClientMessage
		}{Type: string(ClientMessageTypeClosePosition), ClosePositionClientMessage: msg})
	case *ClosePositionClientMessage:
		if msg == nil {
			return nil, fmt.Errorf("client message cannot be nil")
		}
		return ClientMessageToJSON(*msg)
	case RequestExitSignalClientMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			RequestExitSignalClientMessage
		}{Type: string(ClientMessageTypeRequestExitSignal), RequestExitSignalClientMessage: msg})
	case *RequestExitSignalClientMessage:
		if msg == nil {
			return nil, fmt.Errorf("client message cannot be nil")
		}
		return ClientMessageToJSON(*msg)
	case UpdateWalletsClientMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			UpdateWalletsClientMessage
		}{Type: string(ClientMessageTypeUpdateWallets), UpdateWalletsClientMessage: msg})
	case *UpdateWalletsClientMessage:
		if msg == nil {
			return nil, fmt.Errorf("client message cannot be nil")
		}
		return ClientMessageToJSON(*msg)
	case UpdateWatchWalletsClientMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			UpdateWatchWalletsClientMessage
		}{Type: string(ClientMessageTypeUpdateWatchWallets), UpdateWatchWalletsClientMessage: msg})
	case *UpdateWatchWalletsClientMessage:
		if msg == nil {
			return nil, fmt.Errorf("client message cannot be nil")
		}
		return ClientMessageToJSON(*msg)
	case UpdatePositionStrategyClientMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			UpdatePositionStrategyClientMessage
		}{Type: string(ClientMessageTypeUpdatePositionStrategy), UpdatePositionStrategyClientMessage: msg})
	case *UpdatePositionStrategyClientMessage:
		if msg == nil {
			return nil, fmt.Errorf("client message cannot be nil")
		}
		return ClientMessageToJSON(*msg)
	default:
		return nil, fmt.Errorf("unsupported client message type %T", message)
	}
}

// ClientMessageToText encodes a client message to JSON text.
func ClientMessageToText(message ClientMessage) (string, error) {
	encoded, err := ClientMessageToJSON(message)
	if err != nil {
		return "", err
	}
	return string(encoded), nil
}

// ServerMessageFromText decodes a server message JSON string.
func ServerMessageFromText(text string) (ServerMessage, error) {
	return ServerMessageFromJSON([]byte(text))
}

// ServerMessageFromJSON decodes a server message JSON payload.
func ServerMessageFromJSON(data []byte) (ServerMessage, error) {
	var envelope struct {
		Type string `json:"type"`
	}
	if err := json.Unmarshal(data, &envelope); err != nil {
		return nil, fmt.Errorf("decode server message JSON: %w", err)
	}

	switch envelope.Type {
	case string(ServerMessageTypeHelloOk):
		var msg HelloOkServerMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode hello_ok server message: %w", err)
		}
		return msg, nil
	case string(ServerMessageTypePong):
		var msg PongServerMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode pong server message: %w", err)
		}
		return msg, nil
	case string(ServerMessageTypeError):
		var msg ErrorServerMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode error server message: %w", err)
		}
		return msg, nil
	case string(ServerMessageTypePnlUpdate):
		var msg PnlUpdateServerMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode pnl_update server message: %w", err)
		}
		return msg, nil
	case string(ServerMessageTypeBalanceUpdate):
		var msg BalanceUpdateServerMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode balance_update server message: %w", err)
		}
		return msg, nil
	case string(ServerMessageTypePositionOpened):
		var msg PositionOpenedServerMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode position_opened server message: %w", err)
		}
		return msg, nil
	case string(ServerMessageTypePositionClosed):
		var msg PositionClosedServerMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode position_closed server message: %w", err)
		}
		return msg, nil
	case string(ServerMessageTypeExitSignalWithTx):
		var msg ExitSignalWithTxServerMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode exit_signal_with_tx server message: %w", err)
		}
		return msg, nil
	case string(ServerMessageTypeLiquiditySnapshot):
		var msg LiquiditySnapshotServerMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("decode liquidity_snapshot: %w", err)
		}
		return msg, nil
	case string(ServerMessageTypeTradeTick):
		var msg TradeTickServerMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			return nil, fmt.Errorf("unmarshal trade_tick: %w", err)
		}
		return msg, nil
	default:
		return nil, fmt.Errorf("unsupported server message type: %s", envelope.Type)
	}
}

// ServerMessageToJSON encodes a server message to JSON bytes.
func ServerMessageToJSON(message ServerMessage) ([]byte, error) {
	switch msg := message.(type) {
	case HelloOkServerMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			HelloOkServerMessage
		}{Type: string(ServerMessageTypeHelloOk), HelloOkServerMessage: msg})
	case *HelloOkServerMessage:
		if msg == nil {
			return nil, fmt.Errorf("server message cannot be nil")
		}
		return ServerMessageToJSON(*msg)
	case PongServerMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			PongServerMessage
		}{Type: string(ServerMessageTypePong), PongServerMessage: msg})
	case *PongServerMessage:
		if msg == nil {
			return nil, fmt.Errorf("server message cannot be nil")
		}
		return ServerMessageToJSON(*msg)
	case ErrorServerMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			ErrorServerMessage
		}{Type: string(ServerMessageTypeError), ErrorServerMessage: msg})
	case *ErrorServerMessage:
		if msg == nil {
			return nil, fmt.Errorf("server message cannot be nil")
		}
		return ServerMessageToJSON(*msg)
	case PnlUpdateServerMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			PnlUpdateServerMessage
		}{Type: string(ServerMessageTypePnlUpdate), PnlUpdateServerMessage: msg})
	case *PnlUpdateServerMessage:
		if msg == nil {
			return nil, fmt.Errorf("server message cannot be nil")
		}
		return ServerMessageToJSON(*msg)
	case BalanceUpdateServerMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			BalanceUpdateServerMessage
		}{Type: string(ServerMessageTypeBalanceUpdate), BalanceUpdateServerMessage: msg})
	case *BalanceUpdateServerMessage:
		if msg == nil {
			return nil, fmt.Errorf("server message cannot be nil")
		}
		return ServerMessageToJSON(*msg)
	case PositionOpenedServerMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			PositionOpenedServerMessage
		}{Type: string(ServerMessageTypePositionOpened), PositionOpenedServerMessage: msg})
	case *PositionOpenedServerMessage:
		if msg == nil {
			return nil, fmt.Errorf("server message cannot be nil")
		}
		return ServerMessageToJSON(*msg)
	case PositionClosedServerMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			PositionClosedServerMessage
		}{Type: string(ServerMessageTypePositionClosed), PositionClosedServerMessage: msg})
	case *PositionClosedServerMessage:
		if msg == nil {
			return nil, fmt.Errorf("server message cannot be nil")
		}
		return ServerMessageToJSON(*msg)
	case ExitSignalWithTxServerMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			ExitSignalWithTxServerMessage
		}{Type: string(ServerMessageTypeExitSignalWithTx), ExitSignalWithTxServerMessage: msg})
	case *ExitSignalWithTxServerMessage:
		if msg == nil {
			return nil, fmt.Errorf("server message cannot be nil")
		}
		return ServerMessageToJSON(*msg)
	case LiquiditySnapshotServerMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			LiquiditySnapshotServerMessage
		}{Type: string(ServerMessageTypeLiquiditySnapshot), LiquiditySnapshotServerMessage: msg})
	case *LiquiditySnapshotServerMessage:
		return ServerMessageToJSON(*msg)
	case TradeTickServerMessage:
		return json.Marshal(struct {
			Type string `json:"type"`
			TradeTickServerMessage
		}{Type: string(ServerMessageTypeTradeTick), TradeTickServerMessage: msg})
	case *TradeTickServerMessage:
		if msg == nil {
			return nil, fmt.Errorf("server message cannot be nil")
		}
		return ServerMessageToJSON(*msg)
	default:
		return nil, fmt.Errorf("unsupported server message type %T", message)
	}
}

// ServerMessageToText encodes a server message to JSON text.
func ServerMessageToText(message ServerMessage) (string, error) {
	encoded, err := ServerMessageToJSON(message)
	if err != nil {
		return "", err
	}
	return string(encoded), nil
}
