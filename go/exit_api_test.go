package lasersell

import (
	"encoding/json"
	"testing"
)

func TestParseBuildTxResponseEnvelopeOK(t *testing.T) {
	payload := []byte(`{"status":"ok","tx":"abc","route":{"market_type":"pumpfun"}}`)
	parsed, err := ParseBuildTxResponse(payload)
	if err != nil {
		t.Fatalf("expected parse success, got %v", err)
	}

	if parsed.Tx != "abc" {
		t.Fatalf("expected tx=abc, got %s", parsed.Tx)
	}

	routeMap, ok := parsed.Route.(map[string]any)
	if !ok {
		t.Fatalf("expected route map, got %T", parsed.Route)
	}
	if routeMap["market_type"] != "pumpfun" {
		t.Fatalf("unexpected route.market_type: %#v", routeMap["market_type"])
	}
}

func TestParseBuildTxResponseLegacy(t *testing.T) {
	payload := []byte(`{"unsigned_tx_b64":"legacy_tx"}`)
	parsed, err := ParseBuildTxResponse(payload)
	if err != nil {
		t.Fatalf("expected parse success, got %v", err)
	}
	if parsed.Tx != "legacy_tx" {
		t.Fatalf("expected tx=legacy_tx, got %s", parsed.Tx)
	}
}

func TestParseBuildTxResponseBare(t *testing.T) {
	payload := []byte(`{"tx":"bare_tx"}`)
	parsed, err := ParseBuildTxResponse(payload)
	if err != nil {
		t.Fatalf("expected parse success, got %v", err)
	}
	if parsed.Tx != "bare_tx" {
		t.Fatalf("expected tx=bare_tx, got %s", parsed.Tx)
	}
}

func TestParseBuildTxResponseNonOKEnvelope(t *testing.T) {
	payload := []byte(`{"status":"not_ready","reason":"indexing"}`)
	_, err := ParseBuildTxResponse(payload)
	if err == nil {
		t.Fatalf("expected parse failure")
	}

	apiErr, ok := err.(*ExitAPIError)
	if !ok {
		t.Fatalf("expected ExitAPIError, got %T", err)
	}
	if apiErr.Kind != ExitAPIErrorEnvelopeStatus {
		t.Fatalf("expected envelope_status, got %s", apiErr.Kind)
	}
	if apiErr.Status != "not_ready" {
		t.Fatalf("expected status=not_ready, got %s", apiErr.Status)
	}
	if apiErr.Detail != "indexing" {
		t.Fatalf("expected detail=indexing, got %s", apiErr.Detail)
	}
}

func TestBuildSellRequestSerializesAmountTokensContract(t *testing.T) {
	request := BuildSellTxRequest{
		Mint:         "mint",
		UserPubkey:   "user",
		AmountTokens: 42,
		SlippageBps:  Ptr(uint16(1200)),
		Mode:         Ptr("fast"),
		Output:       Ptr(SellOutputSOL),
	}

	payload, err := json.Marshal(request)
	if err != nil {
		t.Fatalf("marshal request: %v", err)
	}

	var encoded map[string]any
	if err := json.Unmarshal(payload, &encoded); err != nil {
		t.Fatalf("unmarshal encoded request: %v", err)
	}

	if encoded["amount_tokens"] != float64(42) {
		t.Fatalf("expected amount_tokens=42, got %#v", encoded["amount_tokens"])
	}
	if _, ok := encoded["amount"]; ok {
		t.Fatalf("unexpected amount field")
	}
	if encoded["output"] != "SOL" {
		t.Fatalf("expected output=SOL, got %#v", encoded["output"])
	}
}

func TestExitAPIClientUsesProductionBaseURLByDefault(t *testing.T) {
	client := NewExitAPIClient()
	if got := client.baseURL(); got != ExitAPIBaseURL {
		t.Fatalf("expected production base URL %q, got %q", ExitAPIBaseURL, got)
	}
}

func TestExitAPIClientUsesLocalBaseURLWhenEnabled(t *testing.T) {
	client := NewExitAPIClient().WithLocalMode(true)
	if got := client.baseURL(); got != LocalExitAPIBaseURL {
		t.Fatalf("expected local base URL %q, got %q", LocalExitAPIBaseURL, got)
	}
}

func TestExitAPIClientBaseURLOverrideTakesPrecedence(t *testing.T) {
	client := NewExitAPIClient().
		WithLocalMode(true).
		WithBaseURL("https://api-dev.example///")

	if got := client.baseURL(); got != "https://api-dev.example" {
		t.Fatalf("expected normalized override base URL, got %q", got)
	}
	if got := client.endpoint("/v1/sell"); got != "https://api-dev.example/v1/sell" {
		t.Fatalf("unexpected endpoint URL: %q", got)
	}
}
