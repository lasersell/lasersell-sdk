package stream

import (
	"context"
	"sync"
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

	mu        sync.RWMutex
	positions map[uint64]PositionHandle
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
	return NewSessionFromConnection(connection), nil
}

// NewSessionFromConnection creates a session from an existing low-level connection.
func NewSessionFromConnection(connection *StreamConnection) *StreamSession {
	return &StreamSession{
		connection: connection,
		positions:  make(map[uint64]PositionHandle),
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

// Close requests close for a tracked position.
func (s *StreamSession) Close(handle PositionHandle) error {
	return s.Sender().ClosePosition(TokenAccountSelector(handle.TokenAccount))
}

// RequestExitSignal requests an exit signal for a tracked position.
func (s *StreamSession) RequestExitSignal(handle PositionHandle, slippageBps *uint16) error {
	return s.Sender().RequestExitSignal(TokenAccountSelector(handle.TokenAccount), slippageBps)
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

		return StreamEvent{
			Type:    StreamEventTypePositionOpened,
			Handle:  cloneHandlePtr(handle),
			Message: message,
		}
	case PositionClosedServerMessage:
		handle := s.removePosition(msg.PositionID, msg.TokenAccount)
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
