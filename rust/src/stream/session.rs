//! Higher-level stream session wrapper with position tracking.
//!
//! `StreamSession` consumes raw stream messages and emits typed events while
//! maintaining an in-memory map of known positions.
//! `StreamConfigure.deadline_timeout_sec` is enforced by SDK timers in this
//! type.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::stream::client::{
    strategy_config_from_optional, validate_strategy_thresholds, IntoPositionSelector,
    PositionSelector, StreamClient, StreamClientError, StreamConfigure, StreamConnection,
    StreamSender,
};
use crate::stream::proto::{ServerMessage, StrategyConfigMsg};

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
    strategy: StrategyConfigMsg,
    deadline_timeout_sec: u64,
    deadline_tasks: HashMap<String, JoinHandle<()>>,
    opened_at: HashMap<String, Instant>,
    open_tokens: Arc<RwLock<HashSet<String>>>,
}

impl StreamSession {
    /// Connects a new stream session and initializes empty position state.
    pub async fn connect(
        client: &StreamClient,
        configure: StreamConfigure,
    ) -> Result<Self, StreamClientError> {
        let deadline_timeout_sec = configure.deadline_timeout_sec;
        let strategy = configure.strategy.clone();
        let connection = client.connect(configure).await?;
        Ok(Self::from_connection_with_strategy(
            connection,
            strategy,
            deadline_timeout_sec,
        ))
    }

    /// Creates a session from an existing low-level connection.
    pub fn from_connection(connection: StreamConnection) -> Self {
        Self::from_connection_with_strategy(connection, default_strategy(), 0)
    }

    /// Creates a session from an existing connection with explicit strategy.
    pub fn from_connection_with_strategy(
        connection: StreamConnection,
        strategy: StrategyConfigMsg,
        deadline_timeout_sec: u64,
    ) -> Self {
        Self {
            connection,
            positions: HashMap::new(),
            strategy,
            deadline_timeout_sec,
            deadline_tasks: HashMap::new(),
            opened_at: HashMap::new(),
            open_tokens: Arc::new(RwLock::new(HashSet::new())),
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
    pub fn close_position(&self, handle: &PositionHandle) -> Result<(), StreamClientError> {
        self.sender().close_position(handle)
    }

    /// Cancels deadline timers and closes the underlying stream connection.
    ///
    /// This consumes the session. Any cloned [`StreamSender`] handles can keep
    /// the worker alive until they are dropped.
    pub fn close(mut self) {
        self.cancel_all_deadlines();
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

    /// Updates strategy parameters and reschedules local deadline timers.
    pub fn update_strategy(
        &mut self,
        strategy: StrategyConfigMsg,
    ) -> Result<(), StreamClientError> {
        validate_strategy_thresholds(&strategy, self.deadline_timeout_sec)?;
        self.sender().update_strategy(strategy.clone())?;
        self.strategy = strategy;
        self.reschedule_all_deadlines();
        Ok(())
    }

    /// Updates server strategy and SDK-local deadline timeout together.
    pub fn update_strategy_with_deadline(
        &mut self,
        strategy: StrategyConfigMsg,
        deadline_timeout_sec: u64,
    ) -> Result<(), StreamClientError> {
        validate_strategy_thresholds(&strategy, deadline_timeout_sec)?;
        self.sender().update_strategy(strategy.clone())?;
        self.strategy = strategy;
        self.deadline_timeout_sec = deadline_timeout_sec;
        self.reschedule_all_deadlines();
        Ok(())
    }

    /// Updates strategy/deadline from optional settings.
    ///
    /// Unset TP/SL values default to `0.0` (disabled). When
    /// `deadline_timeout_sec` is `None`, the current local deadline value is
    /// retained.
    pub fn update_strategy_optional(
        &mut self,
        target_profit_pct: Option<f64>,
        stop_loss_pct: Option<f64>,
        deadline_timeout_sec: Option<u64>,
    ) -> Result<(), StreamClientError> {
        let strategy = strategy_config_from_optional(target_profit_pct, stop_loss_pct);
        match deadline_timeout_sec {
            Some(deadline) => self.update_strategy_with_deadline(strategy, deadline),
            None => self.update_strategy(strategy),
        }
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
                self.opened_at
                    .insert(handle.token_account.clone(), Instant::now());
                self.sync_open_tokens();
                self.arm_deadline_for(&handle.token_account);
                StreamEvent::PositionOpened { handle, message }
            }
            ServerMessage::PositionClosed {
                position_id,
                token_account,
                ..
            } => {
                let handle = self.remove_position(*position_id, token_account.as_deref());
                let token = handle
                    .as_ref()
                    .map(|position| position.token_account.clone())
                    .or_else(|| token_account.clone());
                if let Some(token_account) = token {
                    self.cancel_deadline_for(&token_account);
                }
                self.sync_open_tokens();
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

    fn deadline_duration(&self) -> Option<Duration> {
        (self.deadline_timeout_sec > 0).then_some(Duration::from_secs(self.deadline_timeout_sec))
    }

    fn arm_deadline_for(&mut self, token_account: &str) {
        self.cancel_deadline_task_for(token_account);

        let Some(deadline) = self.deadline_duration() else {
            return;
        };

        let opened_at = *self
            .opened_at
            .entry(token_account.to_string())
            .or_insert_with(Instant::now);
        let now = Instant::now();
        let remaining = opened_at
            .checked_add(deadline)
            .and_then(|deadline_at| deadline_at.checked_duration_since(now))
            .unwrap_or_default();

        if remaining.is_zero() {
            self.try_request_exit_signal(token_account);
            return;
        }

        self.schedule_deadline_task(token_account.to_string(), remaining);
    }

    fn reschedule_all_deadlines(&mut self) {
        self.cancel_all_deadline_tasks();

        let Some(deadline) = self.deadline_duration() else {
            return;
        };

        let now = Instant::now();
        let mut token_accounts = HashSet::new();
        for handle in self.positions.values() {
            token_accounts.insert(handle.token_account.clone());
        }

        for token_account in token_accounts {
            let opened_at = *self.opened_at.entry(token_account.clone()).or_insert(now);
            let remaining = opened_at
                .checked_add(deadline)
                .and_then(|deadline_at| deadline_at.checked_duration_since(now))
                .unwrap_or_default();

            if remaining.is_zero() {
                self.try_request_exit_signal(&token_account);
                continue;
            }

            self.schedule_deadline_task(token_account, remaining);
        }
    }

    fn schedule_deadline_task(&mut self, token_account: String, remaining: Duration) {
        let sender = self.sender();
        let open_tokens = Arc::clone(&self.open_tokens);
        let token_for_map = token_account.clone();
        let token_for_check = token_account.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(remaining).await;
            let is_open = open_tokens
                .read()
                .map(|tokens| tokens.contains(&token_for_check))
                .unwrap_or(false);
            if is_open {
                let _ = sender.request_exit_signal(token_account, None);
            }
        });
        self.deadline_tasks.insert(token_for_map, task);
    }

    fn try_request_exit_signal(&self, token_account: &str) {
        if !self.has_open_position_for_token(token_account) {
            return;
        }
        let _ = self
            .sender()
            .request_exit_signal(token_account.to_string(), None);
    }

    fn has_open_position_for_token(&self, token_account: &str) -> bool {
        self.positions
            .values()
            .any(|handle| handle.token_account == token_account)
    }

    fn cancel_deadline_for(&mut self, token_account: &str) {
        self.cancel_deadline_task_for(token_account);
        self.opened_at.remove(token_account);
    }

    fn cancel_deadline_task_for(&mut self, token_account: &str) {
        if let Some(handle) = self.deadline_tasks.remove(token_account) {
            handle.abort();
        }
    }

    fn cancel_all_deadline_tasks(&mut self) {
        for (_, handle) in self.deadline_tasks.drain() {
            handle.abort();
        }
    }

    fn cancel_all_deadlines(&mut self) {
        self.cancel_all_deadline_tasks();
        self.opened_at.clear();
        self.sync_open_tokens();
    }

    fn sync_open_tokens(&self) {
        if let Ok(mut guard) = self.open_tokens.write() {
            guard.clear();
            for handle in self.positions.values() {
                guard.insert(handle.token_account.clone());
            }
        }
    }
}

impl Drop for StreamSession {
    fn drop(&mut self) {
        self.cancel_all_deadline_tasks();
    }
}

fn default_strategy() -> StrategyConfigMsg {
    StrategyConfigMsg {
        target_profit_pct: 0.0,
        stop_loss_pct: 0.0,
    }
}
