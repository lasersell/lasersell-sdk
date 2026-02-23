//! Exit API client and request/response types.
//!
//! The Exit API builds unsigned buy/sell transactions that can be signed and
//! submitted by the caller.

use std::time::Duration;

use reqwest::{Client, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use tracing::{debug, warn};

use crate::retry::{retry_async, RetryPolicy};
use crate::stream::proto::MarketContextMsg;

const ERROR_BODY_SNIPPET_LEN: usize = 220;
/// Production base URL for the LaserSell Exit API.
pub const EXIT_API_BASE_URL: &str = "https://api.lasersell.io";
/// Local development base URL for the Exit API.
pub const LOCAL_EXIT_API_BASE_URL: &str = "http://localhost:8080";

/// Default tuning values used by [`ExitApiClientOptions::default`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExitApiDefaults;

impl ExitApiDefaults {
    /// Default TCP connect timeout for new HTTP connections.
    pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(200);
    /// Default per-attempt request timeout.
    pub const ATTEMPT_TIMEOUT: Duration = Duration::from_millis(900);
    /// Default maximum number of attempts per request.
    pub const MAX_ATTEMPTS: usize = 2;
    /// Default initial and maximum backoff delay.
    pub const BACKOFF: Duration = Duration::from_millis(25);
    /// Default jitter upper bound added to backoff delays.
    pub const JITTER: Duration = Duration::from_millis(25);
}

/// Configuration options for constructing an [`ExitApiClient`].
#[derive(Clone, Debug)]
pub struct ExitApiClientOptions {
    /// Connection timeout applied when building the underlying HTTP client.
    pub connect_timeout: Duration,
    /// Timeout applied to each individual HTTP attempt.
    pub attempt_timeout: Duration,
    /// Retry policy used for transient request failures.
    pub retry_policy: RetryPolicy,
}

impl Default for ExitApiClientOptions {
    fn default() -> Self {
        Self {
            connect_timeout: ExitApiDefaults::CONNECT_TIMEOUT,
            attempt_timeout: ExitApiDefaults::ATTEMPT_TIMEOUT,
            retry_policy: RetryPolicy {
                max_attempts: ExitApiDefaults::MAX_ATTEMPTS,
                initial_backoff: ExitApiDefaults::BACKOFF,
                max_backoff: ExitApiDefaults::BACKOFF,
                jitter: ExitApiDefaults::JITTER,
            },
        }
    }
}

/// HTTP client for building unsigned buy/sell transactions.
#[derive(Clone)]
pub struct ExitApiClient {
    http: Client,
    api_key: Option<SecretString>,
    attempt_timeout: Duration,
    retry_policy: RetryPolicy,
    local: bool,
    base_url_override: Option<String>,
}

impl ExitApiClient {
    /// Creates a client without an API key using default options.
    pub fn new() -> Result<Self, ExitApiError> {
        Self::with_options(None, ExitApiClientOptions::default())
    }

    /// Creates a client with an API key using default options.
    pub fn with_api_key(api_key: SecretString) -> Result<Self, ExitApiError> {
        Self::with_options(Some(api_key), ExitApiClientOptions::default())
    }

    /// Creates a client with explicit API key and transport options.
    pub fn with_options(
        api_key: Option<SecretString>,
        options: ExitApiClientOptions,
    ) -> Result<Self, ExitApiError> {
        let http = Client::builder()
            .no_proxy()
            .connect_timeout(options.connect_timeout)
            .build()
            .map_err(ExitApiError::Transport)?;

        Ok(Self {
            http,
            api_key,
            attempt_timeout: options.attempt_timeout,
            retry_policy: options.retry_policy,
            local: false,
            base_url_override: None,
        })
    }

    /// Enables or disables local mode.
    ///
    /// When `local` is `true`, requests are sent to
    /// [`LOCAL_EXIT_API_BASE_URL`] instead of [`EXIT_API_BASE_URL`].
    pub fn with_local_mode(mut self, local: bool) -> Self {
        self.local = local;
        self
    }

    /// Sets an explicit Exit API base URL override.
    ///
    /// The override takes precedence over local mode when set.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        self.base_url_override = Some(base_url.trim_end_matches('/').to_string());
        self
    }

    /// Builds an unsigned sell transaction.
    pub async fn build_sell_tx(
        &self,
        request: &BuildSellTxRequest,
    ) -> Result<BuildTxResponse, ExitApiError> {
        self.build_tx("/v1/sell", request).await
    }

    /// Builds an unsigned sell transaction and returns only base64 transaction
    /// data.
    pub async fn build_sell_tx_b64(
        &self,
        request: &BuildSellTxRequest,
    ) -> Result<String, ExitApiError> {
        Ok(self.build_sell_tx(request).await?.tx)
    }

    /// Builds an unsigned buy transaction.
    pub async fn build_buy_tx(
        &self,
        request: &BuildBuyTxRequest,
    ) -> Result<BuildTxResponse, ExitApiError> {
        self.build_tx("/v1/buy", request).await
    }

    async fn build_tx<T>(&self, path: &str, request: &T) -> Result<BuildTxResponse, ExitApiError>
    where
        T: Serialize + Clone,
    {
        let endpoint = self.endpoint(path);
        let policy = self.retry_policy.clone();

        debug!(event = "exit_api_build_tx", endpoint = %endpoint);

        retry_async(
            &policy,
            |_| {
                let endpoint = endpoint.clone();
                let body = request.clone();
                async move { self.send_attempt(&endpoint, &body).await }
            },
            ExitApiError::is_retryable,
        )
        .await
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url(), path)
    }

    fn base_url(&self) -> &str {
        if let Some(base_url) = self.base_url_override.as_deref() {
            return base_url;
        }
        if self.local {
            LOCAL_EXIT_API_BASE_URL
        } else {
            EXIT_API_BASE_URL
        }
    }

    async fn send_attempt<T: Serialize + ?Sized>(
        &self,
        endpoint: &str,
        request: &T,
    ) -> Result<BuildTxResponse, ExitApiError> {
        let mut builder = self
            .http
            .post(endpoint)
            .timeout(self.attempt_timeout)
            .json(request);

        if let Some(api_key) = self.api_key.as_ref() {
            builder = builder.header("x-api-key", api_key.expose_secret());
        }

        let response = builder.send().await.map_err(ExitApiError::Transport)?;
        let status = response.status();
        let body = response.text().await.map_err(ExitApiError::Transport)?;

        debug!(event = "exit_api_response", status = %status);

        if !status.is_success() {
            warn!(event = "exit_api_http_error", status = %status, body = %summarize_error_body(&body));
            return Err(ExitApiError::HttpStatus {
                status,
                body: summarize_error_body(&body),
            });
        }

        let result = parse_build_tx_response(&body);
        match &result {
            Ok(resp) => {
                debug!(event = "exit_api_build_tx_ok", tx_len = resp.tx.len());
            }
            Err(ExitApiError::EnvelopeStatus { status, detail }) => {
                warn!(event = "exit_api_envelope_error", status = %status, detail = %detail);
            }
            _ => {}
        }
        result
    }
}

/// Request payload for `POST /v1/sell`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BuildSellTxRequest {
    /// Mint address of the token being sold.
    pub mint: String,
    /// Trader wallet public key.
    pub user_pubkey: String,
    /// Amount of tokens to sell in mint atomic units.
    pub amount_tokens: u64,
    /// Optional max slippage in basis points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slippage_bps: Option<u16>,
    /// Optional backend mode override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Desired output asset for sell proceeds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<SellOutput>,
    /// Optional referral identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referral_id: Option<String>,
    /// Optional market routing hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_context: Option<MarketContextMsg>,
}

/// Request payload for `POST /v1/buy`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BuildBuyTxRequest {
    /// Mint address of the token being bought.
    pub mint: String,
    /// Trader wallet public key.
    pub user_pubkey: String,
    /// Buy amount in quote-asset atomic units.
    pub amount_quote_units: u64,
    /// Optional max slippage in basis points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slippage_bps: Option<u16>,
    /// Optional backend mode override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Optional referral identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referral_id: Option<String>,
}

/// Preferred output asset for sell requests.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum SellOutput {
    /// Return proceeds as SOL.
    #[serde(rename = "SOL")]
    Sol,
    /// Return proceeds as USD1.
    #[serde(rename = "USD1")]
    Usd1,
}

/// Common response payload returned by buy/sell build endpoints.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BuildTxResponse {
    /// Unsigned transaction serialized as base64.
    pub tx: String,
    /// Optional route metadata from the routing engine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<Value>,
    /// Optional debug payload emitted by the backend.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<Value>,
}

/// Exit API request/response errors.
#[derive(Debug, Error)]
pub enum ExitApiError {
    /// Transport-layer request failure (connect/timeout/send/read).
    #[error("request failed: {0}")]
    Transport(reqwest::Error),

    /// Non-success HTTP status with trimmed response details.
    #[error("http status {status}: {body}")]
    HttpStatus { status: StatusCode, body: String },

    /// Envelope-style response reported non-`ok` status.
    #[error("exit-api status {status}: {detail}")]
    EnvelopeStatus { status: String, detail: String },

    /// Response payload could not be parsed into a supported schema.
    #[error("failed to parse response: {0}")]
    Parse(String),
}

impl ExitApiError {
    /// Returns `true` when the error is likely transient and safe to retry.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Transport(err) => err.is_timeout() || err.is_connect(),
            Self::HttpStatus { status, .. } => {
                status.is_server_error() || *status == StatusCode::TOO_MANY_REQUESTS
            }
            Self::EnvelopeStatus { .. } | Self::Parse(_) => false,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TaggedBuildResponse {
    status: String,
    #[serde(default)]
    tx: Option<String>,
    #[serde(default)]
    unsigned_tx_b64: Option<String>,
    #[serde(default)]
    route: Option<Value>,
    #[serde(default)]
    debug: Option<Value>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LegacyBuildResponse {
    unsigned_tx_b64: String,
    #[serde(default)]
    route: Option<Value>,
    #[serde(default)]
    debug: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct BareBuildResponse {
    tx: String,
    #[serde(default)]
    route: Option<Value>,
    #[serde(default)]
    debug: Option<Value>,
}

fn parse_build_tx_response(body: &str) -> Result<BuildTxResponse, ExitApiError> {
    if let Ok(tagged) = serde_json::from_str::<TaggedBuildResponse>(body) {
        if tagged.status.eq_ignore_ascii_case("ok") {
            let tx = tagged
                .tx
                .or(tagged.unsigned_tx_b64)
                .ok_or_else(|| ExitApiError::Parse("status=ok payload missing tx".to_string()))?;

            return Ok(BuildTxResponse {
                tx,
                route: tagged.route,
                debug: tagged.debug,
            });
        }

        let detail = tagged
            .reason
            .or(tagged.message)
            .or(tagged.error)
            .unwrap_or_else(|| "unknown failure".to_string());

        return Err(ExitApiError::EnvelopeStatus {
            status: tagged.status,
            detail,
        });
    }

    if let Ok(legacy) = serde_json::from_str::<LegacyBuildResponse>(body) {
        return Ok(BuildTxResponse {
            tx: legacy.unsigned_tx_b64,
            route: legacy.route,
            debug: legacy.debug,
        });
    }

    if let Ok(bare) = serde_json::from_str::<BareBuildResponse>(body) {
        return Ok(BuildTxResponse {
            tx: bare.tx,
            route: bare.route,
            debug: bare.debug,
        });
    }

    Err(ExitApiError::Parse(
        "response did not match any supported schema".to_string(),
    ))
}

fn summarize_error_body(body: &str) -> String {
    #[derive(Debug, Deserialize)]
    struct ErrorBody {
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        reason: Option<String>,
    }

    if let Ok(parsed) = serde_json::from_str::<ErrorBody>(body) {
        if let Some(message) = parsed.error.or(parsed.message).or(parsed.reason) {
            return message;
        }
    }

    body.chars().take(ERROR_BODY_SNIPPET_LEN).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        parse_build_tx_response, BuildSellTxRequest, BuildTxResponse, ExitApiClient,
        ExitApiClientOptions, ExitApiError, SellOutput, EXIT_API_BASE_URL, LOCAL_EXIT_API_BASE_URL,
    };

    #[test]
    fn parse_envelope_ok_response() {
        let payload = r#"{"status":"ok","tx":"abc","route":{"market_type":"pumpfun"}}"#;
        let parsed = parse_build_tx_response(payload).expect("parse ok envelope");

        assert_eq!(
            parsed,
            BuildTxResponse {
                tx: "abc".to_string(),
                route: Some(json!({"market_type":"pumpfun"})),
                debug: None,
            }
        );
    }

    #[test]
    fn parse_legacy_unsigned_tx_response() {
        let payload = r#"{"unsigned_tx_b64":"legacy_tx"}"#;
        let parsed = parse_build_tx_response(payload).expect("parse legacy");
        assert_eq!(parsed.tx, "legacy_tx");
    }

    #[test]
    fn parse_bare_tx_response() {
        let payload = r#"{"tx":"bare_tx"}"#;
        let parsed = parse_build_tx_response(payload).expect("parse bare");
        assert_eq!(parsed.tx, "bare_tx");
    }

    #[test]
    fn parse_non_ok_envelope_as_error() {
        let payload = r#"{"status":"not_ready","reason":"indexing"}"#;
        let error = parse_build_tx_response(payload).expect_err("non-ok should error");

        match error {
            ExitApiError::EnvelopeStatus { status, detail } => {
                assert_eq!(status, "not_ready");
                assert_eq!(detail, "indexing");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn sell_request_serializes_amount_tokens_contract() {
        let request = BuildSellTxRequest {
            mint: "mint".to_string(),
            user_pubkey: "user".to_string(),
            amount_tokens: 42,
            slippage_bps: Some(1200),
            mode: Some("fast".to_string()),
            output: Some(SellOutput::Sol),
            referral_id: None,
            market_context: None,
        };

        let value = serde_json::to_value(request).expect("serialize request");
        assert_eq!(
            value.get("amount_tokens").and_then(|v| v.as_u64()),
            Some(42)
        );
        assert!(value.get("amount").is_none());
        assert_eq!(value.get("output").and_then(|v| v.as_str()), Some("SOL"));
    }

    #[test]
    fn exit_api_client_uses_production_base_url() {
        assert_eq!(EXIT_API_BASE_URL, "https://api.lasersell.io");
    }

    #[test]
    fn exit_api_client_uses_local_base_url_when_enabled() {
        let client = ExitApiClient::with_options(None, ExitApiClientOptions::default())
            .expect("build client")
            .with_local_mode(true);
        assert_eq!(LOCAL_EXIT_API_BASE_URL, "http://localhost:8080");
        assert_eq!(client.base_url(), LOCAL_EXIT_API_BASE_URL);
    }

    #[test]
    fn exit_api_client_override_base_url_takes_precedence() {
        let client = ExitApiClient::with_options(None, ExitApiClientOptions::default())
            .expect("build client")
            .with_local_mode(true)
            .with_base_url("https://api-dev.example///");

        assert_eq!(client.base_url(), "https://api-dev.example");
        assert_eq!(
            client.endpoint("/v1/sell"),
            "https://api-dev.example/v1/sell"
        );
    }
}
