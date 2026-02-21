//! Realtime stream modules.
//!
//! - `client`: websocket transport, send queue, and reconnect handling.
//! - `proto`: protocol messages shared with the stream service.
//! - `session`: higher-level typed event stream with position tracking and
//!   SDK-enforced deadline timers.

/// Websocket connection and command sender.
pub mod client;
/// Stream protocol messages.
pub mod proto;
/// Session wrapper that tracks positions and emits typed events.
pub mod session;
