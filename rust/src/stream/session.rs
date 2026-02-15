use std::collections::HashMap;

use crate::stream::client::{
    IntoPositionSelector, PositionSelector, StreamClient, StreamClientError, StreamConfigure,
    StreamConnection, StreamSender,
};
use crate::stream::proto::ServerMessage;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionHandle {
    pub position_id: u64,
    pub token_account: String,
    pub wallet_pubkey: String,
    pub mint: String,
    pub token_program: Option<String>,
    pub tokens: u64,
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

#[derive(Clone, Debug)]
pub enum StreamEvent {
    Message(ServerMessage),
    PositionOpened {
        handle: PositionHandle,
        message: ServerMessage,
    },
    PositionClosed {
        handle: Option<PositionHandle>,
        message: ServerMessage,
    },
    ExitSignalWithTx {
        handle: Option<PositionHandle>,
        message: ServerMessage,
    },
    PnlUpdate {
        handle: Option<PositionHandle>,
        message: ServerMessage,
    },
}

#[derive(Debug)]
pub struct StreamSession {
    connection: StreamConnection,
    positions: HashMap<u64, PositionHandle>,
}

impl StreamSession {
    pub async fn connect(
        client: &StreamClient,
        configure: StreamConfigure,
    ) -> Result<Self, StreamClientError> {
        let connection = client.connect(configure).await?;
        Ok(Self::from_connection(connection))
    }

    pub fn from_connection(connection: StreamConnection) -> Self {
        Self {
            connection,
            positions: HashMap::new(),
        }
    }

    pub fn sender(&self) -> StreamSender {
        self.connection.sender()
    }

    pub fn positions(&self) -> Vec<PositionHandle> {
        self.positions.values().cloned().collect()
    }

    pub fn positions_for_wallet_mint(&self, wallet: &str, mint: &str) -> Vec<PositionHandle> {
        self.positions
            .values()
            .filter(|handle| handle.wallet_pubkey == wallet && handle.mint == mint)
            .cloned()
            .collect()
    }

    pub fn close(&self, handle: &PositionHandle) -> Result<(), StreamClientError> {
        self.sender().close_position(handle)
    }

    pub fn request_exit_signal(
        &self,
        handle: &PositionHandle,
        slippage_bps: Option<u16>,
    ) -> Result<(), StreamClientError> {
        self.sender().request_exit_signal(handle, slippage_bps)
    }

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
