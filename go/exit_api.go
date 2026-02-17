package lasersell

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/lasersell/lasersell-sdk/go/stream"
)

const (
	errorBodySnippetLen = 220
	// ExitAPIBaseURL is the production base URL for the LaserSell Exit API.
	ExitAPIBaseURL = "https://api.lasersell.io"
	// LocalExitAPIBaseURL is the local development base URL for the Exit API.
	LocalExitAPIBaseURL = "http://localhost:8080"
)

// ExitAPIDefaults groups the default tuning values used by DefaultExitAPIClientOptions.
var ExitAPIDefaults = struct {
	ConnectTimeout time.Duration
	AttemptTimeout time.Duration
	MaxAttempts    int
	Backoff        time.Duration
	Jitter         time.Duration
}{
	ConnectTimeout: 200 * time.Millisecond,
	AttemptTimeout: 900 * time.Millisecond,
	MaxAttempts:    2,
	Backoff:        25 * time.Millisecond,
	Jitter:         25 * time.Millisecond,
}

// ExitAPIClientOptions configures ExitAPIClient transport behavior.
type ExitAPIClientOptions struct {
	ConnectTimeout time.Duration
	AttemptTimeout time.Duration
	RetryPolicy    RetryPolicy
	HTTPClient     *http.Client
}

// DefaultExitAPIClientOptions returns the default options for ExitAPIClient.
func DefaultExitAPIClientOptions() ExitAPIClientOptions {
	return ExitAPIClientOptions{
		ConnectTimeout: ExitAPIDefaults.ConnectTimeout,
		AttemptTimeout: ExitAPIDefaults.AttemptTimeout,
		RetryPolicy: RetryPolicy{
			MaxAttempts:    ExitAPIDefaults.MaxAttempts,
			InitialBackoff: ExitAPIDefaults.Backoff,
			MaxBackoff:     ExitAPIDefaults.Backoff,
			Jitter:         ExitAPIDefaults.Jitter,
		},
		HTTPClient: nil,
	}
}

// ExitAPIClient builds unsigned buy/sell transactions via HTTP.
type ExitAPIClient struct {
	http               *http.Client
	apiKey             string
	attemptTimeout     time.Duration
	retryPolicy        RetryPolicy
	local              bool
	baseURLOverride    string
	hasBaseURLOverride bool
}

// NewExitAPIClient creates a client without an API key using default options.
func NewExitAPIClient() *ExitAPIClient {
	return NewExitAPIClientWithOptions("", DefaultExitAPIClientOptions())
}

// NewExitAPIClientWithAPIKey creates a client with an API key using default options.
func NewExitAPIClientWithAPIKey(apiKey string) *ExitAPIClient {
	return NewExitAPIClientWithOptions(apiKey, DefaultExitAPIClientOptions())
}

// NewExitAPIClientWithOptions creates a client with explicit options.
func NewExitAPIClientWithOptions(apiKey string, options ExitAPIClientOptions) *ExitAPIClient {
	normalized := options
	if normalized.ConnectTimeout < 0 {
		normalized.ConnectTimeout = 0
	}
	if normalized.AttemptTimeout <= 0 {
		normalized.AttemptTimeout = ExitAPIDefaults.AttemptTimeout
	}
	if normalized.HTTPClient == nil {
		normalized.HTTPClient = newNoProxyHTTPClient(normalized.ConnectTimeout)
	}

	return &ExitAPIClient{
		http:           normalized.HTTPClient,
		apiKey:         apiKey,
		attemptTimeout: normalized.AttemptTimeout,
		retryPolicy:    normalized.RetryPolicy,
		local:          false,
	}
}

// WithLocalMode enables or disables local mode.
//
// When local is true, requests are sent to LocalExitAPIBaseURL.
func (c *ExitAPIClient) WithLocalMode(local bool) *ExitAPIClient {
	c.local = local
	return c
}

// WithBaseURL sets an explicit Exit API base URL override.
//
// The override takes precedence over local mode when set.
func (c *ExitAPIClient) WithBaseURL(baseURL string) *ExitAPIClient {
	c.baseURLOverride = strings.TrimRight(baseURL, "/")
	c.hasBaseURLOverride = true
	return c
}

// SellOutput controls desired output asset for sell requests.
type SellOutput string

const (
	// SellOutputSOL returns proceeds as SOL.
	SellOutputSOL SellOutput = "SOL"
	// SellOutputUSD1 returns proceeds as USD1.
	SellOutputUSD1 SellOutput = "USD1"
)

// BuildSellTxRequest is the request payload for POST /v1/sell.
type BuildSellTxRequest struct {
	Mint          string                   `json:"mint"`
	UserPubkey    string                   `json:"user_pubkey"`
	AmountTokens  uint64                   `json:"amount_tokens"`
	SlippageBps   *uint16                  `json:"slippage_bps,omitempty"`
	Mode          *string                  `json:"mode,omitempty"`
	Output        *SellOutput              `json:"output,omitempty"`
	ReferralID    *string                  `json:"referral_id,omitempty"`
	MarketContext *stream.MarketContextMsg `json:"market_context,omitempty"`
}

// BuildBuyTxRequest is the request payload for POST /v1/buy.
type BuildBuyTxRequest struct {
	Mint             string  `json:"mint"`
	UserPubkey       string  `json:"user_pubkey"`
	AmountQuoteUnits uint64  `json:"amount_quote_units"`
	SlippageBps      *uint16 `json:"slippage_bps,omitempty"`
	Mode             *string `json:"mode,omitempty"`
	ReferralID       *string `json:"referral_id,omitempty"`
}

// BuildTxResponse is the common buy/sell response payload.
type BuildTxResponse struct {
	Tx    string `json:"tx"`
	Route any    `json:"route,omitempty"`
	Debug any    `json:"debug,omitempty"`
}

// ExitAPIErrorKind classifies exit-api errors.
type ExitAPIErrorKind string

const (
	ExitAPIErrorTransport      ExitAPIErrorKind = "transport"
	ExitAPIErrorHTTPStatus     ExitAPIErrorKind = "http_status"
	ExitAPIErrorEnvelopeStatus ExitAPIErrorKind = "envelope_status"
	ExitAPIErrorParse          ExitAPIErrorKind = "parse"
)

// ExitAPIError is returned for Exit API transport and schema errors.
type ExitAPIError struct {
	Kind       ExitAPIErrorKind
	StatusCode int
	Status     string
	Body       string
	Detail     string
	Err        error
}

func (e *ExitAPIError) Error() string {
	switch e.Kind {
	case ExitAPIErrorTransport:
		return fmt.Sprintf("request failed: %v", e.Err)
	case ExitAPIErrorHTTPStatus:
		return fmt.Sprintf("http status %d: %s", e.StatusCode, e.Body)
	case ExitAPIErrorEnvelopeStatus:
		return fmt.Sprintf("exit-api status %s: %s", e.Status, e.Detail)
	case ExitAPIErrorParse:
		if e.Err != nil {
			return fmt.Sprintf("failed to parse response: %v", e.Err)
		}
		return fmt.Sprintf("failed to parse response: %s", e.Detail)
	default:
		return "exit-api error"
	}
}

// Unwrap returns the wrapped cause, if present.
func (e *ExitAPIError) Unwrap() error {
	return e.Err
}

// IsRetryable reports whether this error is safe to retry.
func (e *ExitAPIError) IsRetryable() bool {
	switch e.Kind {
	case ExitAPIErrorTransport:
		return true
	case ExitAPIErrorHTTPStatus:
		return e.StatusCode >= 500 || e.StatusCode == http.StatusTooManyRequests
	default:
		return false
	}
}

// BuildSellTx builds an unsigned sell transaction.
func (c *ExitAPIClient) BuildSellTx(
	ctx context.Context,
	request BuildSellTxRequest,
) (BuildTxResponse, error) {
	return c.buildTx(ctx, "/v1/sell", request)
}

// BuildSellTxB64 builds an unsigned sell transaction and returns only base64 payload.
func (c *ExitAPIClient) BuildSellTxB64(
	ctx context.Context,
	request BuildSellTxRequest,
) (string, error) {
	response, err := c.BuildSellTx(ctx, request)
	if err != nil {
		return "", err
	}
	return response.Tx, nil
}

// BuildBuyTx builds an unsigned buy transaction.
func (c *ExitAPIClient) BuildBuyTx(
	ctx context.Context,
	request BuildBuyTxRequest,
) (BuildTxResponse, error) {
	return c.buildTx(ctx, "/v1/buy", request)
}

func (c *ExitAPIClient) buildTx(
	ctx context.Context,
	path string,
	request any,
) (BuildTxResponse, error) {
	endpoint := c.endpoint(path)
	return Retry(ctx, c.retryPolicy, func(_ int) (BuildTxResponse, error) {
		return c.sendAttempt(ctx, endpoint, request)
	}, func(err error) bool {
		var apiErr *ExitAPIError
		if errors.As(err, &apiErr) {
			return apiErr.IsRetryable()
		}
		return false
	})
}

func (c *ExitAPIClient) endpoint(path string) string {
	return c.baseURL() + path
}

func (c *ExitAPIClient) baseURL() string {
	if c.hasBaseURLOverride {
		return c.baseURLOverride
	}
	if c.local {
		return LocalExitAPIBaseURL
	}
	return ExitAPIBaseURL
}

func (c *ExitAPIClient) sendAttempt(
	ctx context.Context,
	endpoint string,
	request any,
) (BuildTxResponse, error) {
	var zero BuildTxResponse

	payload, err := json.Marshal(request)
	if err != nil {
		return zero, &ExitAPIError{
			Kind:   ExitAPIErrorParse,
			Detail: "failed to encode request body",
			Err:    err,
		}
	}

	attemptCtx, cancel := context.WithTimeout(ctx, c.attemptTimeout)
	defer cancel()

	httpRequest, err := http.NewRequestWithContext(attemptCtx, http.MethodPost, endpoint, bytes.NewReader(payload))
	if err != nil {
		return zero, &ExitAPIError{Kind: ExitAPIErrorTransport, Err: err}
	}
	httpRequest.Header.Set("content-type", "application/json")
	if c.apiKey != "" {
		httpRequest.Header.Set("x-api-key", c.apiKey)
	}

	response, err := c.http.Do(httpRequest)
	if err != nil {
		return zero, &ExitAPIError{Kind: ExitAPIErrorTransport, Err: err}
	}
	defer response.Body.Close()

	body, err := io.ReadAll(response.Body)
	if err != nil {
		return zero, &ExitAPIError{Kind: ExitAPIErrorTransport, Err: err}
	}

	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return zero, &ExitAPIError{
			Kind:       ExitAPIErrorHTTPStatus,
			StatusCode: response.StatusCode,
			Body:       summarizeErrorBody(body),
		}
	}

	parsed, err := ParseBuildTxResponse(body)
	if err != nil {
		return zero, err
	}
	return parsed, nil
}

// ParseBuildTxResponse parses supported Exit API response schemas.
func ParseBuildTxResponse(body []byte) (BuildTxResponse, error) {
	var zero BuildTxResponse

	var tagged struct {
		Status        *string         `json:"status"`
		Tx            *string         `json:"tx"`
		UnsignedTxB64 *string         `json:"unsigned_tx_b64"`
		Route         json.RawMessage `json:"route"`
		Debug         json.RawMessage `json:"debug"`
		Reason        *string         `json:"reason"`
		Message       *string         `json:"message"`
		Error         *string         `json:"error"`
	}
	if err := json.Unmarshal(body, &tagged); err == nil && tagged.Status != nil {
		if strings.EqualFold(*tagged.Status, "ok") {
			tx := tagged.Tx
			if tx == nil {
				tx = tagged.UnsignedTxB64
			}
			if tx == nil || *tx == "" {
				return zero, &ExitAPIError{
					Kind:   ExitAPIErrorParse,
					Detail: "status=ok payload missing tx",
				}
			}

			return BuildTxResponse{
				Tx:    *tx,
				Route: decodeRawAny(tagged.Route),
				Debug: decodeRawAny(tagged.Debug),
			}, nil
		}

		detail := firstNonEmpty(tagged.Reason, tagged.Message, tagged.Error)
		if detail == "" {
			detail = "unknown failure"
		}

		return zero, &ExitAPIError{
			Kind:   ExitAPIErrorEnvelopeStatus,
			Status: *tagged.Status,
			Detail: detail,
		}
	}

	var legacy struct {
		UnsignedTxB64 string          `json:"unsigned_tx_b64"`
		Route         json.RawMessage `json:"route"`
		Debug         json.RawMessage `json:"debug"`
	}
	if err := json.Unmarshal(body, &legacy); err == nil && legacy.UnsignedTxB64 != "" {
		return BuildTxResponse{
			Tx:    legacy.UnsignedTxB64,
			Route: decodeRawAny(legacy.Route),
			Debug: decodeRawAny(legacy.Debug),
		}, nil
	}

	var bare struct {
		Tx    string          `json:"tx"`
		Route json.RawMessage `json:"route"`
		Debug json.RawMessage `json:"debug"`
	}
	if err := json.Unmarshal(body, &bare); err == nil && bare.Tx != "" {
		return BuildTxResponse{
			Tx:    bare.Tx,
			Route: decodeRawAny(bare.Route),
			Debug: decodeRawAny(bare.Debug),
		}, nil
	}

	return zero, &ExitAPIError{
		Kind:   ExitAPIErrorParse,
		Detail: "response did not match any supported schema",
	}
}

func decodeRawAny(raw json.RawMessage) any {
	if len(raw) == 0 || string(raw) == "null" {
		return nil
	}
	var value any
	if err := json.Unmarshal(raw, &value); err != nil {
		return string(raw)
	}
	return value
}

func firstNonEmpty(values ...*string) string {
	for _, value := range values {
		if value == nil {
			continue
		}
		if strings.TrimSpace(*value) != "" {
			return *value
		}
	}
	return ""
}

func summarizeErrorBody(body []byte) string {
	var parsed struct {
		Error   *string `json:"error"`
		Message *string `json:"message"`
		Reason  *string `json:"reason"`
	}
	if err := json.Unmarshal(body, &parsed); err == nil {
		if message := firstNonEmpty(parsed.Error, parsed.Message, parsed.Reason); message != "" {
			return message
		}
	}

	runes := []rune(string(body))
	if len(runes) > errorBodySnippetLen {
		runes = runes[:errorBodySnippetLen]
	}
	return strings.TrimSpace(string(runes))
}
