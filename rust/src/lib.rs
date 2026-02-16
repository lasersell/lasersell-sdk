//! User-facing Rust SDK for LaserSell stream and Exit API integrations.
//!
//! The crate is organized by transport surface:
//! - `exit_api`: HTTP client for building unsigned buy/sell transactions.
//! - `stream`: realtime websocket client and session helpers.
//! - `tx`: signing and transaction submission helpers.
//! - `retry`: shared retry and timeout utilities.

/// Exit API client and request/response types.
pub mod exit_api;
/// Retry and timeout helpers used across the SDK.
pub mod retry;
/// Realtime stream client, protocol types, and session state helpers.
pub mod stream;
/// Transaction signing and submission helpers.
pub mod tx;
