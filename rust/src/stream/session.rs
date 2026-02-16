//! Higher-level stream session wrapper with position tracking.
//!
//! `StreamSession` consumes raw stream messages and emits typed events while
//! maintaining an in-memory map of known positions.

use std::collections::HashMap;

use crate::stream::client::{
    IntoPositionSelector, PositionSelector, StreamClient, StreamClientError, StreamConfigure,
    StreamConnection, StreamSender,
};
use crate::stream::proto::ServerMessage;

/// Snapshot of a tracked stream position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionHandle {
    /// Numeric position identifier.
    pub position_id: u64,
    /// Token account associated with the position.
    pub token_account: String,
    /// Wallet that owns the position.
    pub wallet_pubkey: String,
    /// Mint address for the token position.
    pub mint: String,
    /// Optional token program identifier.
    pub token_program: Option<String>,
    /// Token balance in atomic units at open.
    pub tokens: u64,
    /// Entry quote amount in atomic units.
    pub entry_quote_units: u64,
}

impl IntoPositionSelector for PositionHandle {
    fn into_position_selector(self) -> PositionSelector {
        PositionSelector::TokenAccount(self.token_account)
    }
}

impl IntoPositionSelector for &PositionHandle {
    fn into_position_selector(self) -> PositionSelector {
        PositionSelector::TokenAccount(self.token_account.clone())
    }
}

/// Session-level event emitted by [`StreamSession::recv`].
#[derive(Clone, Debug)]
pub enum StreamEvent {
    /// Raw message that did not map to a higher-level event.
    Message(ServerMessage),
    /// Position was opened and inserted into session state.
    PositionOpened {
        /// Resolved position handle.
        handle: PositionHandle,
        /// Original underlying server message.
        message: ServerMessage,
    },
    /// Position was closed and removed from session state.
    PositionClosed {
        /// Resolved position handle if known.
        handle: Option<PositionHandle>,
        /// Original underlying server message.
        message: ServerMessage,
    },
    /// Exit signal with unsigned transaction payload.
    ExitSignalWithTx {
        /// Resolved position handle if known.
        handle: Option<PositionHandle>,
        /// Original underlying server message.
        message: ServerMessage,
    },
    /// PnL update for an active position.
    PnlUpdate {
        /// Resolved position handle if known.
        handle: Option<PositionHandle>,
        /// Original underlying server message.
        message: ServerMessage,
    },
}

/// Stateful wrapper around a stream connection.
#[derive(Debug)]
pub struct StreamSession {
    connection: StreamConnection,
    positions: HashMap<u64, PositionHandle>,
}

impl StreamSession {
    /// Connects a new stream session and initializes empty position state.
    pub async fn connect(
        client: &StreamClient,
        configure: StreamConfigure,
    ) -> Result<Self, StreamClientError> {
        let connection = client.connect(configure).await?;
        Ok(Self::from_connection(connection))
    }

    /// Creates a session from an existing low-level connection.
    pub fn from_connection(connection: StreamConnection) -> Self {
        Self {
            connection,
            positions: HashMap::new(),
        }
    }

    /// Returns a cloneable sender for outbound stream commands.
    pub fn sender(&self) -> StreamSender {
        self.connection.sender()
    }

    /// Returns all currently tracked positions.
    pub fn positions(&self) -> Vec<PositionHandle> {
        self.positions.values().cloned().collect()
    }

    /// Returns tracked positions for a specific wallet and mint pair.
    pub fn positions_for_wallet_mint(&self, wallet: &str, mint: &str) -> Vec<PositionHandle> {
        self.positions
            .values()
            .filter(|handle| handle.wallet_pubkey == wallet && handle.mint == mint)
            .cloned()
            .collect()
    }

    /// Requests close for a tracked position.
    pub fn close(&self, handle: &PositionHandle) -> Result<(), StreamClientError> {
        self.sender().close_position(handle)
    }

    /// Requests an exit signal for a tracked position.
    ///
    /// `slippage_bps` overrides strategy slippage for this request when
    /// provided.
    pub fn request_exit_signal(
        &self,
        handle: &PositionHandle,
        slippage_bps: Option<u16>,
    ) -> Result<(), StreamClientError> {
        self.sender().request_exit_signal(handle, slippage_bps)
    }

    /// Receives the next message and maps it into a typed [`StreamEvent`].
    pub async fn recv(&mut self) -> Option<StreamEvent> {
        let message = self.connection.recv().await?;
        Some(self.apply_message(message))
    }

    fn apply_message(&mut self, message: ServerMessage) -> StreamEvent {
        match &message {
            ServerMessage::PositionOpened {
                position_id,
                wallet_pubkey,
                mint,
                token_account,
                token_program,
                tokens,
                entry_quote_units,
                ..
            } => {
                let handle = PositionHandle {
                    position_id: *position_id,
                    token_account: token_account.clone(),
                    wallet_pubkey: wallet_pubkey.clone(),
                    mint: mint.clone(),
                    token_program: token_program.clone(),
                    tokens: *tokens,
                    entry_quote_units: *entry_quote_units,
                };
                self.positions.insert(*position_id, handle.clone());
                StreamEvent::PositionOpened { handle, message }
            }
            ServerMessage::PositionClosed {
                position_id,
                token_account,
                ..
            } => {
                let handle = self.remove_position(*position_id, token_account.as_deref());
                StreamEvent::PositionClosed { handle, message }
            }
            ServerMessage::ExitSignalWithTx {
                position_id,
                token_account,
                ..
            } => {
                let handle = self.find_position(*position_id, token_account.as_deref());
                StreamEvent::ExitSignalWithTx { handle, message }
            }
            ServerMessage::PnlUpdate { position_id, .. } => {
                let handle = self.find_position(*position_id, None);
                StreamEvent::PnlUpdate { handle, message }
            }
            _ => StreamEvent::Message(message),
        }
    }

    fn find_position(
        &self,
        position_id: u64,
        token_account: Option<&str>,
    ) -> Option<PositionHandle> {
        self.positions.get(&position_id).cloned().or_else(|| {
            token_account.and_then(|account| {
                self.positions
                    .values()
                    .find(|handle| handle.token_account == account)
                    .cloned()
            })
        })
    }

    fn remove_position(
        &mut self,
        position_id: u64,
        token_account: Option<&str>,
    ) -> Option<PositionHandle> {
        if let Some(handle) = self.positions.remove(&position_id) {
            return Some(handle);
        }

        let account = token_account?;
        let removed_id = self
            .positions
            .iter()
            .find_map(|(id, handle)| (handle.token_account == account).then_some(*id))?;
        self.positions.remove(&removed_id)
    }
}
