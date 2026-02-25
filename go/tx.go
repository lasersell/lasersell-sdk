package lasersell

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"strings"
	"time"

	"github.com/gagliardetto/solana-go"
)

const (
	txErrorBodySnippetLen = 220
	// HeliusSenderFastURL is the Helius Sender endpoint used by send helpers.
	HeliusSenderFastURL = "https://sender.helius-rpc.com/fast"
	// AstralaneDefaultRegion is the default Astralane Iris region (Frankfurt).
	AstralaneDefaultRegion = "fr"
)

// AstralaneIrisURL builds the Astralane Iris endpoint URL for a given region.
//
// Known regions: fr (Frankfurt, recommended), fr2, la (San Francisco),
// jp (Tokyo), ny (New York), ams (Amsterdam, recommended), ams2,
// lim (Limburg), sg (Singapore), lit (Lithuania).
func AstralaneIrisURL(region string) string {
	return "https://" + region + ".gateway.astralane.io/iris"
}

type sendTargetKind int

const (
	sendTargetKindRpc sendTargetKind = iota
	sendTargetKindHeliusSender
	sendTargetKindAstralane
)

// SendTarget selects the transaction submission endpoint.
type SendTarget struct {
	kind   sendTargetKind
	url    string
	apiKey string
	region string
}

// SendTargetRpc creates a send target for a standard Solana RPC endpoint.
func SendTargetRpc(url string) SendTarget {
	return SendTarget{kind: sendTargetKindRpc, url: url}
}

// SendTargetHeliusSender creates a send target for Helius Sender /fast.
func SendTargetHeliusSender() SendTarget {
	return SendTarget{kind: sendTargetKindHeliusSender}
}

// SendTargetAstralane creates a send target for the Astralane Iris V1 gateway.
// Uses the default Frankfurt region. Use [SendTargetAstralaneRegion] for a specific region.
func SendTargetAstralane(apiKey string) SendTarget {
	return SendTarget{kind: sendTargetKindAstralane, apiKey: apiKey}
}

// SendTargetAstralaneRegion creates a send target for a specific Astralane Iris region.
func SendTargetAstralaneRegion(apiKey, region string) SendTarget {
	return SendTarget{kind: sendTargetKindAstralane, apiKey: apiKey, region: region}
}

// Endpoint returns the HTTP URL to POST sendTransaction to.
func (t SendTarget) Endpoint() string {
	switch t.kind {
	case sendTargetKindRpc:
		return t.url
	case sendTargetKindHeliusSender:
		return HeliusSenderFastURL
	case sendTargetKindAstralane:
		region := t.region
		if region == "" {
			region = AstralaneDefaultRegion
		}
		return AstralaneIrisURL(region)
	default:
		return t.url
	}
}

// TargetLabel returns a human-readable label for logging.
func (t SendTarget) TargetLabel() string {
	switch t.kind {
	case sendTargetKindRpc:
		return "rpc"
	case sendTargetKindHeliusSender:
		return "helius sender"
	case sendTargetKindAstralane:
		return "astralane"
	default:
		return "unknown"
	}
}

// IncludePreflightCommitment reports whether to include preflightCommitment in the config.
func (t SendTarget) IncludePreflightCommitment() bool {
	return t.kind == sendTargetKindRpc
}

// SendTransaction submits a signed transaction via the specified send target.
func SendTransaction(
	ctx context.Context,
	client *http.Client,
	target SendTarget,
	tx *solana.Transaction,
) (string, error) {
	txB64, err := EncodeSignedTx(tx)
	if err != nil {
		return "", err
	}
	return SendTransactionB64To(ctx, client, target, txB64)
}

// SendTransactionB64To submits a signed base64-encoded transaction via the specified send target.
func SendTransactionB64To(
	ctx context.Context,
	client *http.Client,
	target SendTarget,
	txB64 string,
) (string, error) {
	return sendTransactionB64(ctx, client, target.Endpoint(), target.TargetLabel(), txB64, target.IncludePreflightCommitment(), target.extraHeaders())
}

func (t SendTarget) extraHeaders() map[string]string {
	if t.kind == sendTargetKindAstralane && t.apiKey != "" {
		return map[string]string{"x-api-key": t.apiKey}
	}
	return nil
}

// TxSubmitErrorKind classifies transaction sign/submit failures.
type TxSubmitErrorKind string

const (
	TxSubmitErrorDecodeUnsignedTx TxSubmitErrorKind = "decode_unsigned_tx"
	TxSubmitErrorDeserializeTx    TxSubmitErrorKind = "deserialize_unsigned_tx"
	TxSubmitErrorSignTx           TxSubmitErrorKind = "sign_tx"
	TxSubmitErrorSerializeTx      TxSubmitErrorKind = "serialize_tx"
	TxSubmitErrorRequestSend      TxSubmitErrorKind = "request_send"
	TxSubmitErrorResponseRead     TxSubmitErrorKind = "response_read"
	TxSubmitErrorHTTPStatus       TxSubmitErrorKind = "http_status"
	TxSubmitErrorDecodeResponse   TxSubmitErrorKind = "decode_response"
	TxSubmitErrorRPCError         TxSubmitErrorKind = "rpc_error"
	TxSubmitErrorMissingResult    TxSubmitErrorKind = "missing_result"
)

// TxSubmitError describes failures while signing or submitting transactions.
type TxSubmitError struct {
	Kind       TxSubmitErrorKind
	Target     string
	StatusCode int
	Body       string
	Detail     string
	Err        error
}

func (e *TxSubmitError) Error() string {
	switch e.Kind {
	case TxSubmitErrorDecodeUnsignedTx:
		return fmt.Sprintf("decode unsigned tx b64: %v", e.Err)
	case TxSubmitErrorDeserializeTx:
		return fmt.Sprintf("deserialize unsigned tx: %v", e.Err)
	case TxSubmitErrorSignTx:
		return fmt.Sprintf("sign tx: %v", e.Err)
	case TxSubmitErrorSerializeTx:
		return fmt.Sprintf("serialize tx: %v", e.Err)
	case TxSubmitErrorRequestSend:
		return fmt.Sprintf("%s request send failed (%s): %v", e.Target, sendErrorKind(e.Err), e.Err)
	case TxSubmitErrorResponseRead:
		return fmt.Sprintf("%s response read failed (%s): %v", e.Target, readErrorKind(e.Err), e.Err)
	case TxSubmitErrorHTTPStatus:
		return fmt.Sprintf("%s http %d: %s", e.Target, e.StatusCode, e.Body)
	case TxSubmitErrorDecodeResponse:
		return fmt.Sprintf("%s response decode failed: %v. body=%s", e.Target, e.Err, e.Body)
	case TxSubmitErrorRPCError:
		return fmt.Sprintf("%s returned error: %s", e.Target, e.Detail)
	case TxSubmitErrorMissingResult:
		return fmt.Sprintf("%s response missing signature: %s", e.Target, e.Detail)
	default:
		return "transaction submit error"
	}
}

// Unwrap returns wrapped cause.
func (e *TxSubmitError) Unwrap() error {
	return e.Err
}

// SignUnsignedTx decodes and signs an unsigned base64 transaction.
func SignUnsignedTx(unsignedTxB64 string, signer solana.PrivateKey) (*solana.Transaction, error) {
	raw, err := base64.StdEncoding.DecodeString(unsignedTxB64)
	if err != nil {
		return nil, &TxSubmitError{Kind: TxSubmitErrorDecodeUnsignedTx, Err: err}
	}

	tx, err := solana.TransactionFromBytes(raw)
	if err != nil {
		return nil, &TxSubmitError{Kind: TxSubmitErrorDeserializeTx, Err: err}
	}

	signerPubkey := signer.PublicKey()
	signerCopy := signer
	_, err = tx.Sign(func(pubkey solana.PublicKey) *solana.PrivateKey {
		if pubkey.Equals(signerPubkey) {
			return &signerCopy
		}
		return nil
	})
	if err != nil {
		return nil, &TxSubmitError{Kind: TxSubmitErrorSignTx, Err: err}
	}

	return tx, nil
}

// EncodeSignedTx serializes a signed transaction to base64.
func EncodeSignedTx(tx *solana.Transaction) (string, error) {
	raw, err := tx.MarshalBinary()
	if err != nil {
		return "", &TxSubmitError{Kind: TxSubmitErrorSerializeTx, Err: err}
	}
	return base64.StdEncoding.EncodeToString(raw), nil
}

func sendTransactionB64(
	ctx context.Context,
	client *http.Client,
	endpoint string,
	target string,
	txB64 string,
	includePreflightCommitment bool,
	extraHeaders map[string]string,
) (string, error) {
	if client == nil {
		client = newNoProxyHTTPClient(0)
	}

	config := map[string]any{
		"encoding":      "base64",
		"skipPreflight": true,
		"maxRetries":    0,
	}
	if includePreflightCommitment {
		config["preflightCommitment"] = "processed"
	}

	payload := map[string]any{
		"jsonrpc": "2.0",
		"id":      1,
		"method":  "sendTransaction",
		"params":  []any{txB64, config},
	}

	bodyJSON, err := json.Marshal(payload)
	if err != nil {
		return "", &TxSubmitError{Kind: TxSubmitErrorDecodeResponse, Target: target, Err: err}
	}

	httpRequest, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, bytes.NewReader(bodyJSON))
	if err != nil {
		return "", &TxSubmitError{Kind: TxSubmitErrorRequestSend, Target: target, Err: err}
	}
	httpRequest.Header.Set("content-type", "application/json")
	for k, v := range extraHeaders {
		httpRequest.Header.Set(k, v)
	}

	response, err := client.Do(httpRequest)
	if err != nil {
		return "", &TxSubmitError{Kind: TxSubmitErrorRequestSend, Target: target, Err: err}
	}
	defer response.Body.Close()

	body, err := io.ReadAll(response.Body)
	if err != nil {
		return "", &TxSubmitError{Kind: TxSubmitErrorResponseRead, Target: target, Err: err}
	}

	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return "", &TxSubmitError{
			Kind:       TxSubmitErrorHTTPStatus,
			Target:     target,
			StatusCode: response.StatusCode,
			Body:       summarizeTxBody(body),
		}
	}

	var parsed map[string]json.RawMessage
	if err := json.Unmarshal(body, &parsed); err != nil {
		return "", &TxSubmitError{
			Kind:   TxSubmitErrorDecodeResponse,
			Target: target,
			Err:    err,
			Body:   summarizeTxBody(body),
		}
	}

	if rawErr, ok := parsed["error"]; ok && len(rawErr) > 0 && string(rawErr) != "null" {
		return "", &TxSubmitError{
			Kind:   TxSubmitErrorRPCError,
			Target: target,
			Detail: strings.TrimSpace(string(rawErr)),
		}
	}

	rawResult, ok := parsed["result"]
	if !ok {
		return "", &TxSubmitError{
			Kind:   TxSubmitErrorMissingResult,
			Target: target,
			Detail: strings.TrimSpace(string(body)),
		}
	}

	var signature string
	if err := json.Unmarshal(rawResult, &signature); err == nil && strings.TrimSpace(signature) != "" {
		return signature, nil
	}

	return "", &TxSubmitError{
		Kind:   TxSubmitErrorMissingResult,
		Target: target,
		Detail: strings.TrimSpace(string(rawResult)),
	}
}

func sendErrorKind(err error) string {
	if err == nil {
		return "send"
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return "timeout"
	}
	var netErr net.Error
	if errors.As(err, &netErr) && netErr.Timeout() {
		return "timeout"
	}
	var opErr *net.OpError
	if errors.As(err, &opErr) {
		if opErr.Op == "dial" {
			return "connect"
		}
	}
	return "send"
}

func readErrorKind(err error) string {
	if err == nil {
		return "read"
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return "timeout"
	}
	var netErr net.Error
	if errors.As(err, &netErr) && netErr.Timeout() {
		return "timeout"
	}
	return "read"
}

func summarizeTxBody(body []byte) string {
	runes := []rune(strings.TrimSpace(string(body)))
	if len(runes) > txErrorBodySnippetLen {
		runes = runes[:txErrorBodySnippetLen]
	}
	return string(runes)
}

func withWriteTimeout(ctx context.Context, timeout time.Duration) (context.Context, context.CancelFunc) {
	if timeout <= 0 {
		return context.WithCancel(ctx)
	}
	return context.WithTimeout(ctx, timeout)
}
