use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use solana_sdk::signature::Keypair;
use solana_sdk::transaction::VersionedTransaction;
use thiserror::Error;

const ERROR_BODY_SNIPPET_LEN: usize = 220;
pub const HELIUS_SENDER_FAST_URL: &str = "https://sender.helius-rpc.com/fast";

#[derive(Debug, Error)]
pub enum TxSubmitError {
    #[error("decode unsigned tx b64: {0}")]
    DecodeUnsignedTx(base64::DecodeError),

    #[error("deserialize unsigned tx: {0}")]
    DeserializeUnsignedTx(bincode::Error),

    #[error("sign tx: {0}")]
    SignTx(solana_sdk::signer::SignerError),

    #[error("serialize tx: {0}")]
    SerializeTx(bincode::Error),

    #[error("{target} request send failed ({kind}): {source}")]
    RequestSend {
        target: &'static str,
        kind: &'static str,
        #[source]
        source: reqwest::Error,
    },

    #[error("{target} response read failed ({kind}): {source}")]
    ResponseRead {
        target: &'static str,
        kind: &'static str,
        #[source]
        source: reqwest::Error,
    },

    #[error("{target} http {status}: {body}")]
    HttpStatus {
        target: &'static str,
        status: StatusCode,
        body: String,
    },

    #[error("{target} response decode failed: {source}. body={body}")]
    DecodeResponse {
        target: &'static str,
        #[source]
        source: serde_json::Error,
        body: String,
    },

    #[error("{target} returned error: {error}")]
    RpcError { target: &'static str, error: String },

    #[error("{target} response missing signature: {response}")]
    MissingResult {
        target: &'static str,
        response: String,
    },
}

pub fn sign_unsigned_tx(
    unsigned_tx_b64: &str,
    keypair: &Keypair,
) -> Result<VersionedTransaction, TxSubmitError> {
    let raw = BASE64_STANDARD
        .decode(unsigned_tx_b64)
        .map_err(TxSubmitError::DecodeUnsignedTx)?;
    let unsigned: VersionedTransaction =
        bincode::deserialize(&raw).map_err(TxSubmitError::DeserializeUnsignedTx)?;
    VersionedTransaction::try_new(unsigned.message, &[keypair]).map_err(TxSubmitError::SignTx)
}

pub fn encode_signed_tx(tx: &VersionedTransaction) -> Result<String, TxSubmitError> {
    let raw = bincode::serialize(tx).map_err(TxSubmitError::SerializeTx)?;
    Ok(BASE64_STANDARD.encode(raw))
}

pub async fn send_via_helius_sender(
    client: &Client,
    tx: &VersionedTransaction,
) -> Result<String, TxSubmitError> {
    let tx_b64 = encode_signed_tx(tx)?;
    send_via_helius_sender_b64(client, &tx_b64).await
}

pub async fn send_via_rpc(
    client: &Client,
    rpc_url: &str,
    tx: &VersionedTransaction,
) -> Result<String, TxSubmitError> {
    let tx_b64 = encode_signed_tx(tx)?;
    send_via_rpc_b64(client, rpc_url, &tx_b64).await
}

pub async fn send_via_helius_sender_b64(
    client: &Client,
    tx_b64: &str,
) -> Result<String, TxSubmitError> {
    send_transaction_b64(
        client,
        HELIUS_SENDER_FAST_URL,
        "helius sender",
        tx_b64,
        false,
    )
    .await
}

pub async fn send_via_rpc_b64(
    client: &Client,
    rpc_url: &str,
    tx_b64: &str,
) -> Result<String, TxSubmitError> {
    send_transaction_b64(client, rpc_url, "rpc", tx_b64, true).await
}

async fn send_transaction_b64(
    client: &Client,
    endpoint: &str,
    target: &'static str,
    tx_b64: &str,
    include_preflight_commitment: bool,
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
    }

    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [tx_b64, config],
    });

    let response = client
        .post(endpoint)
        .json(&payload)
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
    result
        .as_str()
        .filter(|signature| !signature.is_empty())
        .map(|signature| signature.to_string())
        .ok_or_else(|| TxSubmitError::MissingResult {
            target,
            response: result.to_string(),
        })
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
