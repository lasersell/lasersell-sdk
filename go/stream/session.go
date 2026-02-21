package stream

import (
	"context"
	"sync"
	"time"
)

// PositionHandle is a snapshot of a tracked position.
type PositionHandle struct {
	PositionID      uint64
	TokenAccount    string
	WalletPubkey    string
	Mint            string
	TokenProgram    *string
	Tokens          uint64
	EntryQuoteUnits uint64
}

// StreamEventType classifies session-level events.
type StreamEventType string

const (
	StreamEventTypeMessage          StreamEventType = "message"
	StreamEventTypePositionOpened   StreamEventType = "position_opened"
	StreamEventTypePositionClosed   StreamEventType = "position_closed"
	StreamEventTypeExitSignalWithTx StreamEventType = "exit_signal_with_tx"
	StreamEventTypePnlUpdate        StreamEventType = "pnl_update"
)

// StreamEvent maps raw server messages into position-aware events.
type StreamEvent struct {
	Type    StreamEventType
	Handle  *PositionHandle
	Message ServerMessage
}

// StreamSession is a stateful wrapper around StreamConnection.
type StreamSession struct {
	connection *StreamConnection

	mu                 sync.RWMutex
	positions          map[uint64]PositionHandle
	strategy           StrategyConfigMsg
	deadlineTimeoutSec uint64
	openedAt           map[string]time.Time
	timers             map[string]*time.Timer
}

// ConnectSession opens a new stream session and initializes position state.
func ConnectSession(
	ctx context.Context,
	client *StreamClient,
	configure StreamConfigure,
) (*StreamSession, error) {
	connection, err := client.Connect(ctx, configure)
	if err != nil {
		return nil, err
	}
	return NewSessionFromConnectionWithStrategy(
		connection,
		configure.Strategy,
		configure.DeadlineTimeoutSec,
	), nil
}

// NewSessionFromConnection creates a session from an existing low-level connection.
func NewSessionFromConnection(connection *StreamConnection) *StreamSession {
	return NewSessionFromConnectionWithStrategy(connection, StrategyConfigMsg{}, 0)
}

// NewSessionFromConnectionWithStrategy creates a session with explicit initial strategy.
func NewSessionFromConnectionWithStrategy(
	connection *StreamConnection,
	strategy StrategyConfigMsg,
	deadlineTimeoutSec uint64,
) *StreamSession {
	return &StreamSession{
		connection:         connection,
		positions:          make(map[uint64]PositionHandle),
		strategy:           strategy,
		deadlineTimeoutSec: deadlineTimeoutSec,
		openedAt:           make(map[string]time.Time),
		timers:             make(map[string]*time.Timer),
	}
}

// Sender returns a cloneable sender for outbound stream commands.
func (s *StreamSession) Sender() *StreamSender {
	return s.connection.Sender()
}

// Positions returns all currently tracked positions.
func (s *StreamSession) Positions() []PositionHandle {
	s.mu.RLock()
	defer s.mu.RUnlock()

	out := make([]PositionHandle, 0, len(s.positions))
	for _, handle := range s.positions {
		out = append(out, handle)
	}
	return out
}

// PositionsForWalletMint filters tracked positions by wallet + mint.
func (s *StreamSession) PositionsForWalletMint(wallet string, mint string) []PositionHandle {
	s.mu.RLock()
	defer s.mu.RUnlock()

	out := make([]PositionHandle, 0)
	for _, handle := range s.positions {
		if handle.WalletPubkey == wallet && handle.Mint == mint {
			out = append(out, handle)
		}
	}
	return out
}

// Close closes the session when called without arguments.
//
// For backward compatibility, passing one position handle preserves the legacy
// behavior and requests close for that position.
func (s *StreamSession) Close(handles ...PositionHandle) error {
	if len(handles) > 1 {
		return protocolError("close accepts zero or one position handle")
	}
	if len(handles) == 1 {
		return s.Sender().ClosePosition(TokenAccountSelector(handles[0].TokenAccount))
	}

	s.mu.Lock()
	s.cancelAllTimersLocked()
	s.openedAt = make(map[string]time.Time)
	s.mu.Unlock()
	s.connection.Close()
	return nil
}

// ClosePosition requests close for a tracked position.
func (s *StreamSession) ClosePosition(handle PositionHandle) error {
	return s.Sender().ClosePosition(TokenAccountSelector(handle.TokenAccount))
}

// RequestExitSignal requests an exit signal for a tracked position.
func (s *StreamSession) RequestExitSignal(handle PositionHandle, slippageBps *uint16) error {
	return s.Sender().RequestExitSignal(TokenAccountSelector(handle.TokenAccount), slippageBps)
}

// UpdateStrategy updates local strategy state, sends the update to stream, and
// reschedules all deadline timers.
func (s *StreamSession) UpdateStrategy(
	ctx context.Context,
	strategy StrategyConfigMsg,
	deadlineTimeoutSec ...uint64,
) error {
	if ctx == nil {
		ctx = context.Background()
	}

	select {
	case <-ctx.Done():
		return ctx.Err()
	default:
	}

	s.mu.RLock()
	nextDeadline := s.deadlineTimeoutSec
	s.mu.RUnlock()
	if len(deadlineTimeoutSec) > 0 {
		nextDeadline = deadlineTimeoutSec[0]
	}
	if err := validateStrategyAndDeadline(strategy, nextDeadline); err != nil {
		return err
	}

	if err := s.Sender().UpdateStrategy(strategy); err != nil {
		return err
	}

	s.mu.Lock()
	s.strategy = strategy
	if len(deadlineTimeoutSec) > 0 {
		s.deadlineTimeoutSec = deadlineTimeoutSec[0]
	}
	immediate := s.rescheduleAllDeadlinesLocked(time.Now())
	s.mu.Unlock()

	for _, tokenAccount := range immediate {
		s.tryRequestExitSignal(tokenAccount)
	}
	return nil
}

// UpdateStrategyOptional updates strategy from optional TP/SL settings and optionally updates deadline.
func (s *StreamSession) UpdateStrategyOptional(
	ctx context.Context,
	optional OptionalStrategyConfig,
	deadlineTimeoutSec ...uint64,
) error {
	return s.UpdateStrategy(ctx, StrategyConfigFromOptional(optional), deadlineTimeoutSec...)
}

// Recv receives the next server message and maps it into StreamEvent.
func (s *StreamSession) Recv(ctx context.Context) (*StreamEvent, error) {
	message, err := s.connection.Recv(ctx)
	if err != nil {
		return nil, err
	}

	event := s.applyMessage(message)
	return &event, nil
}

func (s *StreamSession) applyMessage(message ServerMessage) StreamEvent {
	s.mu.Lock()
	defer s.mu.Unlock()

	switch msg := message.(type) {
	case PositionOpenedServerMessage:
		handle := PositionHandle{
			PositionID:      msg.PositionID,
			TokenAccount:    msg.TokenAccount,
			WalletPubkey:    msg.WalletPubkey,
			Mint:            msg.Mint,
			TokenProgram:    msg.TokenProgram,
			Tokens:          msg.Tokens,
			EntryQuoteUnits: msg.EntryQuoteUnits,
		}
		s.positions[msg.PositionID] = handle
		s.openedAt[handle.TokenAccount] = time.Now()
		s.armDeadlineLocked(handle.TokenAccount)

		return StreamEvent{
			Type:    StreamEventTypePositionOpened,
			Handle:  cloneHandlePtr(handle),
			Message: message,
		}
	case PositionClosedServerMessage:
		handle := s.removePosition(msg.PositionID, msg.TokenAccount)
		if handle != nil {
			s.cancelDeadlineForLocked(handle.TokenAccount)
		} else if msg.TokenAccount != nil {
			s.cancelDeadlineForLocked(*msg.TokenAccount)
		}
		return StreamEvent{
			Type:    StreamEventTypePositionClosed,
			Handle:  cloneHandlePtrFromPtr(handle),
			Message: message,
		}
	case ExitSignalWithTxServerMessage:
		handle := s.findPosition(msg.PositionID, msg.TokenAccount)
		return StreamEvent{
			Type:    StreamEventTypeExitSignalWithTx,
			Handle:  cloneHandlePtrFromPtr(handle),
			Message: message,
		}
	case PnlUpdateServerMessage:
		handle := s.findPosition(msg.PositionID, nil)
		return StreamEvent{
			Type:    StreamEventTypePnlUpdate,
			Handle:  cloneHandlePtrFromPtr(handle),
			Message: message,
		}
	default:
		return StreamEvent{
			Type:    StreamEventTypeMessage,
			Message: message,
		}
	}
}

func (s *StreamSession) findPosition(positionID uint64, tokenAccount *string) *PositionHandle {
	if handle, ok := s.positions[positionID]; ok {
		return cloneHandlePtr(handle)
	}

	if tokenAccount == nil {
		return nil
	}

	for _, handle := range s.positions {
		if handle.TokenAccount == *tokenAccount {
			return cloneHandlePtr(handle)
		}
	}

	return nil
}

func (s *StreamSession) removePosition(positionID uint64, tokenAccount *string) *PositionHandle {
	if handle, ok := s.positions[positionID]; ok {
		delete(s.positions, positionID)
		return cloneHandlePtr(handle)
	}

	if tokenAccount == nil {
		return nil
	}

	for id, handle := range s.positions {
		if handle.TokenAccount == *tokenAccount {
			delete(s.positions, id)
			return cloneHandlePtr(handle)
		}
	}

	return nil
}

func cloneHandlePtr(handle PositionHandle) *PositionHandle {
	clone := handle
	return &clone
}

func cloneHandlePtrFromPtr(handle *PositionHandle) *PositionHandle {
	if handle == nil {
		return nil
	}
	clone := *handle
	return &clone
}

func (s *StreamSession) deadlineDurationLocked() (time.Duration, bool) {
	if s.deadlineTimeoutSec == 0 {
		return 0, false
	}
	return time.Duration(s.deadlineTimeoutSec) * time.Second, true
}

func (s *StreamSession) armDeadlineLocked(tokenAccount string) {
	s.cancelTimerLocked(tokenAccount)

	deadline, enabled := s.deadlineDurationLocked()
	if !enabled {
		return
	}

	openedAt, ok := s.openedAt[tokenAccount]
	if !ok {
		openedAt = time.Now()
		s.openedAt[tokenAccount] = openedAt
	}

	remaining := time.Until(openedAt.Add(deadline))
	if remaining <= 0 {
		go s.tryRequestExitSignal(tokenAccount)
		return
	}

	s.timers[tokenAccount] = time.AfterFunc(remaining, func() {
		s.onDeadlineTimerFired(tokenAccount)
	})
}

func (s *StreamSession) rescheduleAllDeadlinesLocked(now time.Time) []string {
	s.cancelAllTimersLocked()

	deadline, enabled := s.deadlineDurationLocked()
	if !enabled {
		return nil
	}

	openByToken := make(map[string]struct{})
	for _, handle := range s.positions {
		openByToken[handle.TokenAccount] = struct{}{}
	}

	immediate := make([]string, 0)
	for tokenAccount := range openByToken {
		openedAt, ok := s.openedAt[tokenAccount]
		if !ok {
			openedAt = now
			s.openedAt[tokenAccount] = openedAt
		}

		remaining := openedAt.Add(deadline).Sub(now)
		if remaining <= 0 {
			immediate = append(immediate, tokenAccount)
			continue
		}

		tokenAccount := tokenAccount
		s.timers[tokenAccount] = time.AfterFunc(remaining, func() {
			s.onDeadlineTimerFired(tokenAccount)
		})
	}

	return immediate
}

func (s *StreamSession) onDeadlineTimerFired(tokenAccount string) {
	s.mu.Lock()
	delete(s.timers, tokenAccount)
	open := s.hasOpenTokenAccountLocked(tokenAccount)
	s.mu.Unlock()
	if !open {
		return
	}

	s.tryRequestExitSignal(tokenAccount)
}

func (s *StreamSession) tryRequestExitSignal(tokenAccount string) {
	s.mu.RLock()
	open := s.hasOpenTokenAccountLocked(tokenAccount)
	s.mu.RUnlock()
	if !open {
		return
	}
	_ = s.Sender().RequestExitSignal(TokenAccountSelector(tokenAccount), nil)
}

func (s *StreamSession) hasOpenTokenAccountLocked(tokenAccount string) bool {
	for _, handle := range s.positions {
		if handle.TokenAccount == tokenAccount {
			return true
		}
	}
	return false
}

func (s *StreamSession) cancelDeadlineForLocked(tokenAccount string) {
	s.cancelTimerLocked(tokenAccount)
	delete(s.openedAt, tokenAccount)
}

func (s *StreamSession) cancelTimerLocked(tokenAccount string) {
	timer, ok := s.timers[tokenAccount]
	if !ok {
		return
	}
	timer.Stop()
	delete(s.timers, tokenAccount)
}

func (s *StreamSession) cancelAllTimersLocked() {
	for tokenAccount, timer := range s.timers {
		timer.Stop()
		delete(s.timers, tokenAccount)
	}
}
