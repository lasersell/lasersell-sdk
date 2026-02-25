//! Transaction signing and submission helpers.
//!
//! This module provides utilities to:
//! - sign unsigned base64-encoded Solana transactions,
//! - encode signed transactions to base64, and
//! - submit transactions via Helius Sender or a standard RPC endpoint.

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::transaction::VersionedTransaction;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info, warn};

macro_rules! helius_sender_base_url {
    () => {
        "https://sender.helius-rpc.com"
    };
}

const ERROR_BODY_SNIPPET_LEN: usize = 220;
const CONFIRM_POLL_INTERVAL: Duration = Duration::from_millis(200);
const RPC_SEND_MAX_RETRIES: u64 = 3;
/// Base URL for Helius Sender.
pub const HELIUS_SENDER_BASE_URL: &str = helius_sender_base_url!();
/// Helius Sender endpoint used by [`send_transaction`] helpers.
pub const HELIUS_SENDER_FAST_URL: &str = concat!(helius_sender_base_url!(), "/fast");
/// Ping endpoint used for connection warming.
pub const HELIUS_SENDER_PING_URL: &str = concat!(helius_sender_base_url!(), "/ping");
/// Default Astralane Iris region (Frankfurt).
pub const ASTRALANE_DEFAULT_REGION: &str = "fr";

/// Builds the Astralane Iris endpoint URL for a given region.
///
/// Known regions: `fr` (Frankfurt, recommended), `fr2`, `la` (San Francisco),
/// `jp` (Tokyo), `ny` (New York), `ams` (Amsterdam, recommended), `ams2`,
/// `lim` (Limburg), `sg` (Singapore), `lit` (Lithuania).
pub fn astralane_iris_url(region: &str) -> String {
    format!("https://{region}.gateway.astralane.io/iris")
}

/// Unified send target that selects the transaction submission endpoint.
#[derive(Clone, Debug)]
pub enum SendTarget {
    /// Standard Solana JSON-RPC endpoint.
    Rpc { url: String },
    /// Helius Sender `/fast` endpoint.
    HeliusSender,
    /// Astralane Iris V1 gateway authenticated via `x-api-key` header.
    Astralane {
        api_key: String,
        /// Regional endpoint prefix (e.g. `"fr"`, `"ams"`). Defaults to [`ASTRALANE_DEFAULT_REGION`].
        region: Option<String>,
    },
}

impl SendTarget {
    /// Returns the HTTP endpoint URL to POST `sendTransaction` to.
    pub fn endpoint(&self) -> String {
        match self {
            Self::Rpc { url } => url.clone(),
            Self::HeliusSender => HELIUS_SENDER_FAST_URL.to_string(),
            Self::Astralane { region, .. } => {
                let r = region.as_deref().unwrap_or(ASTRALANE_DEFAULT_REGION);
                astralane_iris_url(r)
            }
        }
    }

    /// Human-readable label for logging.
    pub fn target_label(&self) -> &'static str {
        match self {
            Self::Rpc { .. } => "rpc",
            Self::HeliusSender => "helius sender",
            Self::Astralane { .. } => "astralane",
        }
    }

    /// Whether to include `preflightCommitment` in the `sendTransaction` config.
    pub fn include_preflight_commitment(&self) -> bool {
        matches!(self, Self::Rpc { .. })
    }
}

/// Submits a signed transaction via the specified [`SendTarget`].
pub async fn send_transaction(
    client: &Client,
    target: &SendTarget,
    tx: &VersionedTransaction,
) -> Result<String, TxSubmitError> {
    let tx_b64 = encode_signed_tx(tx)?;
    send_transaction_b64_to(client, target, &tx_b64).await
}

/// Submits a signed base64-encoded transaction via the specified [`SendTarget`].
pub async fn send_transaction_b64_to(
    client: &Client,
    target: &SendTarget,
    tx_b64: &str,
) -> Result<String, TxSubmitError> {
    let extra_headers = match target {
        SendTarget::Astralane { api_key, .. } => {
            vec![("x-api-key", api_key.as_str())]
        }
        _ => vec![],
    };
    send_transaction_b64(
        client,
        &target.endpoint(),
        target.target_label(),
        tx_b64,
        target.include_preflight_commitment(),
        &extra_headers,
    )
    .await
}

/// Error returned when signing or submitting a transaction.
#[derive(Debug, Error)]
pub enum TxSubmitError {
    /// Failed to decode the unsigned transaction from base64.
    #[error("decode unsigned tx b64: {0}")]
    DecodeUnsignedTx(base64::DecodeError),

    /// Failed to deserialize the unsigned transaction bytes.
    #[error("deserialize unsigned tx: {0}")]
    DeserializeUnsignedTx(bincode::Error),

    /// Failed to sign the transaction message.
    #[error("sign tx: {0}")]
    SignTx(solana_sdk::signer::SignerError),

    /// Failed to serialize the signed transaction.
    #[error("serialize tx: {0}")]
    SerializeTx(bincode::Error),

    /// HTTP request send phase failed.
    #[error("{target} request send failed ({kind}): {source}")]
    RequestSend {
        target: &'static str,
        kind: &'static str,
        #[source]
        source: reqwest::Error,
    },

    /// HTTP response body read phase failed.
    #[error("{target} response read failed ({kind}): {source}")]
    ResponseRead {
        target: &'static str,
        kind: &'static str,
        #[source]
        source: reqwest::Error,
    },

    /// Endpoint returned a non-success HTTP status.
    #[error("{target} http {status}: {body}")]
    HttpStatus {
        target: &'static str,
        status: StatusCode,
        body: String,
    },

    /// Response body could not be parsed as expected JSON payload.
    #[error("{target} response decode failed: {source}. body={body}")]
    DecodeResponse {
        target: &'static str,
        #[source]
        source: serde_json::Error,
        body: String,
    },

    /// JSON-RPC response contained an `error` field.
    #[error("{target} returned error: {error}")]
    RpcError { target: &'static str, error: String },

    /// JSON-RPC response did not contain a valid signature result.
    #[error("{target} response missing signature: {response}")]
    MissingResult {
        target: &'static str,
        response: String,
    },

    /// Signature was accepted by sendTransaction but never reached confirmed/finalized.
    #[error("tx confirm timed out for {signature}")]
    ConfirmTimeout { signature: String },

    /// Signature landed on-chain with an execution error.
    #[error("tx failed on-chain for {signature}: {error}")]
    TxFailed { signature: String, error: String },
}

/// Decodes and signs an unsigned base64-encoded transaction.
///
/// The function expects a serialized `VersionedTransaction` payload encoded as
/// standard base64.
pub fn sign_unsigned_tx(
    unsigned_tx_b64: &str,
    keypair: &Keypair,
) -> Result<VersionedTransaction, TxSubmitError> {
    let raw = BASE64_STANDARD
        .decode(unsigned_tx_b64)
        .map_err(TxSubmitError::DecodeUnsignedTx)?;
    let unsigned: VersionedTransaction =
        bincode::deserialize(&raw).map_err(TxSubmitError::DeserializeUnsignedTx)?;
    let signed =
        VersionedTransaction::try_new(unsigned.message, &[keypair]).map_err(TxSubmitError::SignTx)?;
    debug!(event = "tx_signed");
    Ok(signed)
}

/// Fast-path signer for an unsigned base64-encoded Solana transaction.
///
/// This avoids full bincode de/serialization by:
/// - decoding base64 into the raw `VersionedTransaction` wire format,
/// - signing the message bytes,
/// - patching the signature bytes in-place,
/// - re-encoding to base64.
///
/// Intended for hot paths where you only need a signed base64 string to submit.
pub fn sign_unsigned_tx_b64_fast(
    unsigned_tx_b64: &str,
    keypair: &Keypair,
) -> Result<String, TxSubmitError> {
    let mut raw = BASE64_STANDARD
        .decode(unsigned_tx_b64)
        .map_err(TxSubmitError::DecodeUnsignedTx)?;

    patch_sign_transaction_in_place(&mut raw, keypair)
        .map_err(|error| TxSubmitError::DeserializeUnsignedTx(custom_bincode_error(error)))?;

    Ok(BASE64_STANDARD.encode(raw))
}

/// Best-effort ping against Helius Sender.
///
/// Useful for connection warming when your application sits idle between sends.
pub async fn ping_helius_sender(client: &Client) -> bool {
    match client.get(HELIUS_SENDER_PING_URL).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Serializes a signed transaction to standard base64.
pub fn encode_signed_tx(tx: &VersionedTransaction) -> Result<String, TxSubmitError> {
    let raw = bincode::serialize(tx).map_err(TxSubmitError::SerializeTx)?;
    Ok(BASE64_STANDARD.encode(raw))
}

/// Confirms a submitted transaction signature via JSON-RPC polling.
///
/// `sendTransaction` returning a signature only means the node accepted the packet.
/// We must still verify chain execution before treating the sell as complete.
pub async fn confirm_signature_via_rpc(
    client: &Client,
    rpc_url: &str,
    signature: &str,
    timeout: std::time::Duration,
) -> Result<(), TxSubmitError> {
    debug!(event = "tx_confirm_start", signature, timeout_ms = timeout.as_millis() as u64);
    let started = tokio::time::Instant::now();

    loop {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignatureStatuses",
            "params": [[signature], { "searchTransactionHistory": true }],
        });

        let response = client
            .post(rpc_url)
            .json(&payload)
            .send()
            .await
            .map_err(|source| TxSubmitError::RequestSend {
                target: "rpc",
                kind: send_error_kind(&source),
                source,
            })?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|source| TxSubmitError::ResponseRead {
                target: "rpc",
                kind: read_error_kind(&source),
                source,
            })?;

        if !status.is_success() {
            return Err(TxSubmitError::HttpStatus {
                target: "rpc",
                status,
                body: summarize_body(&body),
            });
        }

        let parsed: Value =
            serde_json::from_str(&body).map_err(|source| TxSubmitError::DecodeResponse {
                target: "rpc",
                source,
                body: summarize_body(&body),
            })?;
        if let Some(err) = parsed.get("error") {
            return Err(TxSubmitError::RpcError {
                target: "rpc",
                error: err.to_string(),
            });
        }

        let value = parsed
            .get("result")
            .and_then(|result| result.get("value"))
            .and_then(Value::as_array)
            .and_then(|values| values.first())
            .ok_or_else(|| TxSubmitError::MissingResult {
                target: "rpc",
                response: parsed.to_string(),
            })?;

        if !value.is_null() {
            if let Some(err) = value.get("err").filter(|err| !err.is_null()) {
                warn!(event = "tx_failed_onchain", signature, error = %err);
                return Err(TxSubmitError::TxFailed {
                    signature: signature.to_string(),
                    error: err.to_string(),
                });
            }

            if let Some(status) = value.get("confirmationStatus").and_then(Value::as_str) {
                if status == "confirmed" || status == "finalized" {
                    info!(event = "tx_confirmed", signature, status);
                    return Ok(());
                }
            }
        }

        if started.elapsed() >= timeout {
            warn!(event = "tx_confirm_timeout", signature, elapsed_ms = started.elapsed().as_millis() as u64);
            return Err(TxSubmitError::ConfirmTimeout {
                signature: signature.to_string(),
            });
        }
        tokio::time::sleep(CONFIRM_POLL_INTERVAL).await;
    }
}

async fn send_transaction_b64(
    client: &Client,
    endpoint: &str,
    target: &'static str,
    tx_b64: &str,
    include_preflight_commitment: bool,
    extra_headers: &[(&str, &str)],
) -> Result<String, TxSubmitError> {
    let mut config = json!({
        "encoding": "base64",
        "skipPreflight": true,
        "maxRetries": 0
    });
    if include_preflight_commitment {
        let obj = config
            .as_object_mut()
            .expect("sendTransaction config must be an object");
        obj.insert(
            "preflightCommitment".to_string(),
            Value::String("processed".to_string()),
        );
        obj.insert("maxRetries".to_string(), Value::from(RPC_SEND_MAX_RETRIES));
    }

    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [tx_b64, config],
    });

    debug!(event = "tx_submitting", target);

    let mut request = client.post(endpoint).json(&payload);
    for &(key, value) in extra_headers {
        request = request.header(key, value);
    }
    let response = request
        .send()
        .await
        .map_err(|source| TxSubmitError::RequestSend {
            target,
            kind: send_error_kind(&source),
            source,
        })?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|source| TxSubmitError::ResponseRead {
            target,
            kind: read_error_kind(&source),
            source,
        })?;

    debug!(event = "tx_submit_response", target, status = %status);

    if !status.is_success() {
        return Err(TxSubmitError::HttpStatus {
            target,
            status,
            body: summarize_body(&body),
        });
    }

    let parsed: Value =
        serde_json::from_str(&body).map_err(|source| TxSubmitError::DecodeResponse {
            target,
            source,
            body: summarize_body(&body),
        })?;
    if let Some(err) = parsed.get("error") {
        warn!(event = "tx_submit_rpc_error", target, error = %err);
        return Err(TxSubmitError::RpcError {
            target,
            error: err.to_string(),
        });
    }

    let result = parsed
        .get("result")
        .ok_or_else(|| TxSubmitError::MissingResult {
            target,
            response: parsed.to_string(),
        })?;
    let signature = result
        .as_str()
        .filter(|signature| !signature.is_empty())
        .map(|signature| signature.to_string())
        .ok_or_else(|| TxSubmitError::MissingResult {
            target,
            response: result.to_string(),
        })?;
    info!(event = "tx_submitted", target, signature = %signature);
    Ok(signature)
}

fn send_error_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else {
        "send"
    }
}

fn read_error_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else {
        "read"
    }
}

fn summarize_body(body: &str) -> String {
    body.chars().take(ERROR_BODY_SNIPPET_LEN).collect()
}

fn custom_bincode_error(message: String) -> bincode::Error {
    Box::new(bincode::ErrorKind::Custom(message))
}

fn patch_sign_transaction_in_place(raw: &mut [u8], keypair: &Keypair) -> Result<(), String> {
    let (signature_count, sig_count_len) = decode_shortvec_len(raw, 0)?;
    let message_start = sig_count_len
        .checked_add(signature_count.saturating_mul(64))
        .ok_or_else(|| "transaction signature section overflow".to_string())?;

    if message_start > raw.len() {
        return Err("transaction is truncated (signatures)".to_string());
    }

    let message = &raw[message_start..];
    let signer_pubkey = keypair.pubkey().to_bytes();
    let (maybe_index, num_required_signatures) =
        find_signer_index_in_message(message, &signer_pubkey)?;

    let signature_index = match maybe_index {
        Some(index) => index,
        None => {
            if signature_count == 1 && num_required_signatures == 1 {
                0
            } else {
                return Err(
                    "signer pubkey not found among required signer accounts in tx message"
                        .to_string(),
                );
            }
        }
    };

    if signature_index >= signature_count {
        return Err(format!(
            "signer signature index ({signature_index}) out of range for signature vector length ({signature_count})"
        ));
    }

    let signature_offset = sig_count_len + signature_index * 64;
    let end = signature_offset + 64;
    if end > raw.len() {
        return Err("transaction is truncated (signature bytes)".to_string());
    }

    let signature = keypair.sign_message(message);
    raw[signature_offset..end].copy_from_slice(signature.as_ref());
    Ok(())
}

fn find_signer_index_in_message(
    message: &[u8],
    signer_pubkey: &[u8; 32],
) -> Result<(Option<usize>, usize), String> {
    let first = *message
        .get(0)
        .ok_or_else(|| "message is empty".to_string())?;
    let mut offset = 0usize;

    if (first & 0x80) != 0 {
        let version = first & 0x7f;
        if version != 0 {
            return Err(format!("unsupported Solana message version: {version}"));
        }
        offset = 1;
    }

    let num_required_signatures = *message
        .get(offset)
        .ok_or_else(|| "message is truncated (header)".to_string())?
        as usize;

    // Skip header: numRequiredSignatures, numReadonlySignedAccounts, numReadonlyUnsignedAccounts
    offset = offset
        .checked_add(3)
        .ok_or_else(|| "message header overflow".to_string())?;

    let (account_key_count, key_count_len) = decode_shortvec_len(message, offset)?;
    offset += key_count_len;

    let keys_len = account_key_count.saturating_mul(32);
    if offset + keys_len > message.len() {
        return Err("message is truncated (account keys)".to_string());
    }

    let signer_count = std::cmp::min(num_required_signatures, account_key_count);
    for i in 0..signer_count {
        let start = offset + i * 32;
        let end = start + 32;
        if message[start..end] == signer_pubkey[..] {
            return Ok((Some(i), num_required_signatures));
        }
    }

    Ok((None, num_required_signatures))
}

fn decode_shortvec_len(bytes: &[u8], offset: usize) -> Result<(usize, usize), String> {
    let mut value: usize = 0;
    let mut shift: usize = 0;
    let mut idx = offset;

    loop {
        let byte = *bytes
            .get(idx)
            .ok_or_else(|| "shortvec byte out of range".to_string())?;
        value |= ((byte & 0x7f) as usize) << shift;
        idx += 1;
        if (byte & 0x80) == 0 {
            break;
        }
        shift += 7;
        if shift > 28 {
            return Err("shortvec length overflow".to_string());
        }
    }

    Ok((value, idx - offset))
}
