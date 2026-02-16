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

// StrategyConfigMsg defines stream-side exit strategy parameters.
type StrategyConfigMsg struct {
	TargetProfitPct    float64 `json:"target_profit_pct"`
	StopLossPct        float64 `json:"stop_loss_pct"`
	DeadlineTimeoutSec uint64  `json:"deadline_timeout_sec"`
}

// LimitsMsg captures server-advertised session limits.
type LimitsMsg struct {
	HiCapacity             uint32 `json:"hi_capacity"`
	PnlFlushMS             uint64 `json:"pnl_flush_ms"`
	MaxPositionsPerSession uint32 `json:"max_positions_per_session"`
	MaxWalletsPerSession   uint32 `json:"max_wallets_per_session"`
	MaxPositionsPerWallet  uint32 `json:"max_positions_per_wallet"`
	MaxSessionsPerAPIKey   uint32 `json:"max_sessions_per_api_key"`
}

// ClientMessageType is the stream protocol discriminator for client messages.
type ClientMessageType string

const (
	ClientMessageTypePing              ClientMessageType = "ping"
	ClientMessageTypeConfigure         ClientMessageType = "configure"
	ClientMessageTypeUpdateStrategy    ClientMessageType = "update_strategy"
	ClientMessageTypeClosePosition     ClientMessageType = "close_position"
	ClientMessageTypeRequestExitSignal ClientMessageType = "request_exit_signal"
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
	WalletPubkeys []string          `json:"wallet_pubkeys"`
	Strategy      StrategyConfigMsg `json:"strategy"`
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
	ServerMessageTypeExitSignalWithTx ServerMessageType = "exit_signal_with_tx"
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
	PositionID    uint64 `json:"position_id"`
	ProfitUnits   int64  `json:"profit_units"`
	ProceedsUnits uint64 `json:"proceeds_units"`
	ServerTimeMS  uint64 `json:"server_time_ms"`
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
	PositionID      uint64            `json:"position_id"`
	WalletPubkey    string            `json:"wallet_pubkey"`
	Mint            string            `json:"mint"`
	TokenAccount    string            `json:"token_account"`
	TokenProgram    *string           `json:"token_program,omitempty"`
	Tokens          uint64            `json:"tokens"`
	EntryQuoteUnits uint64            `json:"entry_quote_units"`
	MarketContext   *MarketContextMsg `json:"market_context,omitempty"`
	Slot            uint64            `json:"slot"`
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
}

func (ExitSignalWithTxServerMessage) ServerMessageType() ServerMessageType {
	return ServerMessageTypeExitSignalWithTx
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

	return ConfigureClientMessage{
		WalletPubkeys: walletPubkeys,
		Strategy:      strategy,
	}, nil
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
