package stream

import (
	"encoding/json"
	"testing"
)

func TestMarketContextRoundTripRaydiumCpmm(t *testing.T) {
	ctx := MarketContextMsg{
		MarketType: MarketTypeRaydiumCpmm,
		RaydiumCpmm: &RaydiumCpmmContextMsg{
			Pool:             "11111111111111111111111111111111",
			Config:           "22222222222222222222222222222222",
			QuoteMint:        "33333333333333333333333333333333",
			UserQuoteAccount: "44444444444444444444444444444444",
		},
	}

	payload, err := json.Marshal(ctx)
	if err != nil {
		t.Fatalf("marshal market context: %v", err)
	}

	var decoded MarketContextMsg
	if err := json.Unmarshal(payload, &decoded); err != nil {
		t.Fatalf("unmarshal market context: %v", err)
	}

	if decoded.MarketType != MarketTypeRaydiumCpmm {
		t.Fatalf("unexpected market type: %s", decoded.MarketType)
	}
	if decoded.RaydiumCpmm == nil {
		t.Fatalf("expected raydium_cpmm context")
	}
	if decoded.RaydiumCpmm.Pool != ctx.RaydiumCpmm.Pool {
		t.Fatalf("unexpected pool: %s", decoded.RaydiumCpmm.Pool)
	}
}

func TestClientConfigureDeserializesLegacyWalletPubkeyString(t *testing.T) {
	raw := `{
		"type":"configure",
		"wallet_pubkey":"11111111111111111111111111111111",
		"strategy":{
			"target_profit_pct":5.0,
			"stop_loss_pct":1.5,
			"deadline_timeout_sec":45
		}
	}`

	message, err := ClientMessageFromText(raw)
	if err != nil {
		t.Fatalf("decode client message: %v", err)
	}

	configure, ok := message.(ConfigureClientMessage)
	if !ok {
		t.Fatalf("expected ConfigureClientMessage, got %T", message)
	}
	if len(configure.WalletPubkeys) != 1 || configure.WalletPubkeys[0] != "11111111111111111111111111111111" {
		t.Fatalf("unexpected wallet_pubkeys: %#v", configure.WalletPubkeys)
	}

	encoded, err := ClientMessageToJSON(configure)
	if err != nil {
		t.Fatalf("encode client message: %v", err)
	}

	var encodedMap map[string]any
	if err := json.Unmarshal(encoded, &encodedMap); err != nil {
		t.Fatalf("decode encoded json: %v", err)
	}

	if _, ok := encodedMap["wallet_pubkey"]; ok {
		t.Fatalf("wallet_pubkey alias should not be encoded")
	}
	wallets, ok := encodedMap["wallet_pubkeys"].([]any)
	if !ok || len(wallets) != 1 {
		t.Fatalf("unexpected wallet_pubkeys encoding: %#v", encodedMap["wallet_pubkeys"])
	}
}

func TestRequestExitSignalDeserializesSellNowAlias(t *testing.T) {
	raw := `{"type":"sell_now","position_id":123,"slippage_bps":42}`
	message, err := ClientMessageFromText(raw)
	if err != nil {
		t.Fatalf("decode client message: %v", err)
	}

	req, ok := message.(RequestExitSignalClientMessage)
	if !ok {
		t.Fatalf("expected RequestExitSignalClientMessage, got %T", message)
	}
	if req.PositionID == nil || *req.PositionID != 123 {
		t.Fatalf("unexpected position_id: %#v", req.PositionID)
	}
	if req.SlippageBps == nil || *req.SlippageBps != 42 {
		t.Fatalf("unexpected slippage_bps: %#v", req.SlippageBps)
	}

	encoded, err := ClientMessageToJSON(req)
	if err != nil {
		t.Fatalf("encode request_exit_signal: %v", err)
	}

	var encodedMap map[string]any
	if err := json.Unmarshal(encoded, &encodedMap); err != nil {
		t.Fatalf("decode encoded json: %v", err)
	}
	if encodedMap["type"] != "request_exit_signal" {
		t.Fatalf("expected canonical type=request_exit_signal, got %#v", encodedMap["type"])
	}
}

func TestServerExitSignalRoundTrip(t *testing.T) {
	message := ExitSignalWithTxServerMessage{
		SessionID:      7,
		PositionID:     8,
		WalletPubkey:   "55555555555555555555555555555555",
		Mint:           "11111111111111111111111111111111",
		TokenAccount:   ptr("22222222222222222222222222222222"),
		PositionTokens: 10,
		ProfitUnits:    5,
		Reason:         "tp",
		TriggeredAtMS:  123,
		UnsignedTxB64:  "dGVzdA==",
		MarketContext: &MarketContextMsg{
			MarketType: MarketTypeRaydiumCpmm,
			RaydiumCpmm: &RaydiumCpmmContextMsg{
				Pool:             "11111111111111111111111111111111",
				Config:           "22222222222222222222222222222222",
				QuoteMint:        "33333333333333333333333333333333",
				UserQuoteAccount: "44444444444444444444444444444444",
			},
		},
	}

	encoded, err := ServerMessageToJSON(message)
	if err != nil {
		t.Fatalf("encode server message: %v", err)
	}
	decoded, err := ServerMessageFromJSON(encoded)
	if err != nil {
		t.Fatalf("decode server message: %v", err)
	}

	exitMessage, ok := decoded.(ExitSignalWithTxServerMessage)
	if !ok {
		t.Fatalf("expected ExitSignalWithTxServerMessage, got %T", decoded)
	}
	if exitMessage.UnsignedTxB64 != "dGVzdA==" {
		t.Fatalf("unexpected unsigned_tx_b64: %s", exitMessage.UnsignedTxB64)
	}
}

func ptr[T any](value T) *T {
	return &value
}
