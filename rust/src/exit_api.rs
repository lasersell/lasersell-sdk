use std::time::Duration;

use reqwest::{Client, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::retry::{retry_async, RetryPolicy};
use crate::stream::proto::MarketContextMsg;

const ERROR_BODY_SNIPPET_LEN: usize = 220;
pub const EXIT_API_BASE_URL: &str = "https://api.lasersell.io";
pub const LOCAL_EXIT_API_BASE_URL: &str = "http://localhost:8080";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExitApiDefaults;

impl ExitApiDefaults {
    pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(200);
    pub const ATTEMPT_TIMEOUT: Duration = Duration::from_millis(900);
    pub const MAX_ATTEMPTS: usize = 2;
    pub const BACKOFF: Duration = Duration::from_millis(25);
    pub const JITTER: Duration = Duration::from_millis(25);
}

#[derive(Clone, Debug)]
pub struct ExitApiClientOptions {
    pub connect_timeout: Duration,
    pub attempt_timeout: Duration,
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

#[derive(Clone)]
pub struct ExitApiClient {
    http: Client,
    api_key: Option<SecretString>,
    attempt_timeout: Duration,
    retry_policy: RetryPolicy,
    local: bool,
}

impl ExitApiClient {
    pub fn new() -> Result<Self, ExitApiError> {
        Self::with_options(None, ExitApiClientOptions::default())
    }

    pub fn with_api_key(api_key: SecretString) -> Result<Self, ExitApiError> {
        Self::with_options(Some(api_key), ExitApiClientOptions::default())
    }

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
        })
    }

    pub fn with_local_mode(mut self, local: bool) -> Self {
        self.local = local;
        self
    }

    pub async fn build_sell_tx(
        &self,
        request: &BuildSellTxRequest,
    ) -> Result<BuildTxResponse, ExitApiError> {
        self.build_tx("/v1/sell", request).await
    }

    pub async fn build_sell_tx_b64(
        &self,
        request: &BuildSellTxRequest,
    ) -> Result<String, ExitApiError> {
        Ok(self.build_sell_tx(request).await?.tx)
    }

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

    fn base_url(&self) -> &'static str {
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

        if !status.is_success() {
            return Err(ExitApiError::HttpStatus {
                status,
                body: summarize_error_body(&body),
            });
        }

        parse_build_tx_response(&body)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BuildSellTxRequest {
    pub mint: String,
    pub user_pubkey: String,
    pub amount_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slippage_bps: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<SellOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referral_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_context: Option<MarketContextMsg>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BuildBuyTxRequest {
    pub mint: String,
    pub user_pubkey: String,
    pub amount_quote_units: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slippage_bps: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referral_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum SellOutput {
    #[serde(rename = "SOL")]
    Sol,
    #[serde(rename = "USD1")]
    Usd1,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BuildTxResponse {
    pub tx: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<Value>,
}

#[derive(Debug, Error)]
pub enum ExitApiError {
    #[error("request failed: {0}")]
    Transport(reqwest::Error),

    #[error("http status {status}: {body}")]
    HttpStatus { status: StatusCode, body: String },

    #[error("exit-api status {status}: {detail}")]
    EnvelopeStatus { status: String, detail: String },

    #[error("failed to parse response: {0}")]
    Parse(String),
}

impl ExitApiError {
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
}
