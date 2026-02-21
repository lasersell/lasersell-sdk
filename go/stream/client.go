package stream

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"sync"
	"time"
	"unicode"

	"nhooyr.io/websocket"
)

const (
	minReconnectBackoff = 100 * time.Millisecond
	maxReconnectBackoff = 2 * time.Second
	ioWriteTimeout      = 5 * time.Second
	ioReadTimeout       = 60 * time.Second
	ioDialTimeout       = 10 * time.Second
	outboundQueueSize   = 1024
	inboundQueueSize    = 1024
)

const (
	// StreamEndpoint is the production websocket endpoint.
	StreamEndpoint = "wss://stream.lasersell.io/v1/ws"
	// LocalStreamEndpoint is the local development websocket endpoint.
	LocalStreamEndpoint = "ws://localhost:8082/v1/ws"
)

// StreamClientErrorKind classifies stream transport/protocol errors.
type StreamClientErrorKind string

const (
	StreamClientErrorWebSocket       StreamClientErrorKind = "websocket"
	StreamClientErrorJSON            StreamClientErrorKind = "json"
	StreamClientErrorInvalidAPIKey   StreamClientErrorKind = "invalid_api_key_header"
	StreamClientErrorSendQueueClosed StreamClientErrorKind = "send_queue_closed"
	StreamClientErrorProtocol        StreamClientErrorKind = "protocol"
)

// StreamClientError is returned for websocket transport and protocol failures.
type StreamClientError struct {
	Kind    StreamClientErrorKind
	Message string
	Err     error
}

func (e *StreamClientError) Error() string {
	switch e.Kind {
	case StreamClientErrorWebSocket:
		return fmt.Sprintf("websocket error: %v", e.Err)
	case StreamClientErrorJSON:
		return fmt.Sprintf("json error: %v", e.Err)
	case StreamClientErrorInvalidAPIKey:
		return fmt.Sprintf("invalid api-key header: %v", e.Err)
	case StreamClientErrorSendQueueClosed:
		return "send queue is closed"
	case StreamClientErrorProtocol:
		if e.Message != "" {
			return fmt.Sprintf("protocol error: %s", e.Message)
		}
		return "protocol error"
	default:
		return "stream client error"
	}
}

// Unwrap returns wrapped cause.
func (e *StreamClientError) Unwrap() error {
	return e.Err
}

func sendQueueClosedError() error {
	return &StreamClientError{Kind: StreamClientErrorSendQueueClosed}
}

func protocolError(message string) error {
	return &StreamClientError{Kind: StreamClientErrorProtocol, Message: message}
}

func websocketError(err error) error {
	return &StreamClientError{Kind: StreamClientErrorWebSocket, Err: err}
}

func jsonError(err error) error {
	return &StreamClientError{Kind: StreamClientErrorJSON, Err: err}
}

func invalidAPIKeyHeaderError(err error) error {
	return &StreamClientError{Kind: StreamClientErrorInvalidAPIKey, Err: err}
}

// StreamConfigure configures wallets and strategy for a stream connection.
type StreamConfigure struct {
	WalletPubkeys      []string          `json:"wallet_pubkeys"`
	Strategy           StrategyConfigMsg `json:"strategy"`
	DeadlineTimeoutSec uint64            `json:"deadline_timeout_sec,omitempty"`
}

// SingleWalletStreamConfigure creates configuration for a single wallet.
func SingleWalletStreamConfigure(walletPubkey string, strategy StrategyConfigMsg) StreamConfigure {
	return StreamConfigure{
		WalletPubkeys: []string{walletPubkey},
		Strategy:      strategy,
	}
}

// PositionSelector selects a target position by token account or position id.
type PositionSelector struct {
	TokenAccount *string
	PositionID   *uint64
}

// TokenAccountSelector creates a selector from token account.
func TokenAccountSelector(tokenAccount string) PositionSelector {
	return PositionSelector{TokenAccount: &tokenAccount}
}

// PositionIDSelector creates a selector from position id.
func PositionIDSelector(positionID uint64) PositionSelector {
	return PositionSelector{PositionID: &positionID}
}

func (s PositionSelector) validate() error {
	if s.TokenAccount == nil && s.PositionID == nil {
		return protocolError("position selector must include token_account or position_id")
	}
	if s.TokenAccount != nil && s.PositionID != nil {
		return protocolError("position selector must include only one of token_account or position_id")
	}
	return nil
}

// StreamClient creates stream websocket connections.
type StreamClient struct {
	apiKey              string
	local               bool
	endpointOverride    string
	hasEndpointOverride bool
}

// NewStreamClient creates a production-mode stream client.
func NewStreamClient(apiKey string) *StreamClient {
	return &StreamClient{apiKey: apiKey, local: false}
}

// WithLocalMode enables or disables local endpoint routing.
func (c *StreamClient) WithLocalMode(local bool) *StreamClient {
	c.local = local
	return c
}

// WithEndpoint sets an explicit stream endpoint override.
//
// The override takes precedence over local mode when set.
func (c *StreamClient) WithEndpoint(endpoint string) *StreamClient {
	c.endpointOverride = strings.TrimRightFunc(endpoint, unicode.IsSpace)
	c.hasEndpointOverride = true
	return c
}

// Connect opens and configures a stream connection.
func (c *StreamClient) Connect(ctx context.Context, configure StreamConfigure) (*StreamConnection, error) {
	worker := newStreamConnectionWorker(c.endpoint(), c.apiKey, configure)
	if err := worker.waitReady(ctx); err != nil {
		return nil, err
	}
	return &StreamConnection{worker: worker}, nil
}

func (c *StreamClient) endpoint() string {
	if c.hasEndpointOverride {
		return c.endpointOverride
	}
	if c.local {
		return LocalStreamEndpoint
	}
	return StreamEndpoint
}

// StreamConnection exposes sender and receiver sides of an active connection.
type StreamConnection struct {
	worker *streamConnectionWorker
}

// Sender returns a cloneable sender for outbound stream commands.
func (c *StreamConnection) Sender() *StreamSender {
	return &StreamSender{worker: c.worker}
}

// Split returns sender plus raw inbound receiver channel.
func (c *StreamConnection) Split() (*StreamSender, <-chan ServerMessage) {
	return c.Sender(), c.worker.inbound
}

// Recv receives the next server message.
func (c *StreamConnection) Recv(ctx context.Context) (ServerMessage, error) {
	return c.worker.recv(ctx)
}

// Close shuts down the stream worker.
func (c *StreamConnection) Close() {
	c.worker.close()
}

// StreamSender queues outbound stream commands.
type StreamSender struct {
	worker *streamConnectionWorker
}

// Send enqueues a raw client message.
func (s *StreamSender) Send(message ClientMessage) error {
	return s.worker.enqueue(message)
}

// Ping sends a heartbeat ping.
func (s *StreamSender) Ping(clientTimeMS uint64) error {
	return s.Send(PingClientMessage{ClientTimeMS: clientTimeMS})
}

// UpdateStrategy updates strategy parameters.
func (s *StreamSender) UpdateStrategy(strategy StrategyConfigMsg) error {
	return s.Send(UpdateStrategyClientMessage{Strategy: strategy})
}

// ClosePosition requests a position close.
func (s *StreamSender) ClosePosition(selector PositionSelector) error {
	if err := selector.validate(); err != nil {
		return err
	}
	return s.Send(ClosePositionClientMessage{
		PositionID:   selector.PositionID,
		TokenAccount: selector.TokenAccount,
	})
}

// CloseByID requests close for position id.
func (s *StreamSender) CloseByID(positionID uint64) error {
	return s.ClosePosition(PositionIDSelector(positionID))
}

// RequestExitSignal requests exit signal generation.
func (s *StreamSender) RequestExitSignal(selector PositionSelector, slippageBps *uint16) error {
	if err := selector.validate(); err != nil {
		return err
	}
	return s.Send(RequestExitSignalClientMessage{
		PositionID:   selector.PositionID,
		TokenAccount: selector.TokenAccount,
		SlippageBps:  slippageBps,
	})
}

// RequestExitSignalByID requests exit signal for position id.
func (s *StreamSender) RequestExitSignalByID(positionID uint64, slippageBps *uint16) error {
	return s.RequestExitSignal(PositionIDSelector(positionID), slippageBps)
}

type sessionOutcome int

const (
	sessionOutcomeGracefulShutdown sessionOutcome = iota
	sessionOutcomeReconnect
)

type streamConnectionWorker struct {
	endpoint  string
	apiKey    string
	configure StreamConfigure

	ctx    context.Context
	cancel context.CancelFunc

	outbound chan ClientMessage
	inbound  chan ServerMessage

	done      chan struct{}
	ready     chan error
	readyOnce sync.Once
}

func newStreamConnectionWorker(endpoint string, apiKey string, configure StreamConfigure) *streamConnectionWorker {
	ctx, cancel := context.WithCancel(context.Background())
	worker := &streamConnectionWorker{
		endpoint:  endpoint,
		apiKey:    apiKey,
		configure: configure,
		ctx:       ctx,
		cancel:    cancel,
		outbound:  make(chan ClientMessage, outboundQueueSize),
		inbound:   make(chan ServerMessage, inboundQueueSize),
		done:      make(chan struct{}),
		ready:     make(chan error, 1),
	}

	go worker.run()
	return worker
}

func (w *streamConnectionWorker) waitReady(ctx context.Context) error {
	select {
	case <-ctx.Done():
		w.close()
		return ctx.Err()
	case err := <-w.ready:
		return err
	}
}

func (w *streamConnectionWorker) signalReady(err error) {
	w.readyOnce.Do(func() {
		w.ready <- err
		close(w.ready)
	})
}

func (w *streamConnectionWorker) close() {
	w.cancel()
}

func (w *streamConnectionWorker) enqueue(message ClientMessage) error {
	if message == nil {
		return protocolError("client message cannot be nil")
	}

	select {
	case <-w.ctx.Done():
		return sendQueueClosedError()
	case w.outbound <- message:
		return nil
	}
}

func (w *streamConnectionWorker) recv(ctx context.Context) (ServerMessage, error) {
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case msg, ok := <-w.inbound:
		if !ok {
			return nil, io.EOF
		}
		return msg, nil
	}
}

func (w *streamConnectionWorker) run() {
	defer close(w.done)
	defer close(w.inbound)

	ready := false
	pending := make([]ClientMessage, 0)
	backoff := minReconnectBackoff

	for {
		if w.ctx.Err() != nil {
			if !ready {
				w.signalReady(sendQueueClosedError())
			}
			return
		}

		outcome, err := w.runConnectedSession(&pending, &ready)
		if err != nil {
			if !ready {
				w.signalReady(err)
				return
			}
			outcome = sessionOutcomeReconnect
		}

		if outcome == sessionOutcomeGracefulShutdown {
			if !ready {
				w.signalReady(sendQueueClosedError())
			}
			return
		}

		if outcome == sessionOutcomeReconnect {
			backoff = minReconnectBackoff
		}

		if !w.collectMessagesDuringDelay(backoff, &pending) {
			if !ready {
				w.signalReady(sendQueueClosedError())
			}
			return
		}
		backoff *= 2
		if backoff > maxReconnectBackoff {
			backoff = maxReconnectBackoff
		}
	}
}

func (w *streamConnectionWorker) collectMessagesDuringDelay(
	delay time.Duration,
	pending *[]ClientMessage,
) bool {
	timer := time.NewTimer(delay)
	defer timer.Stop()

	for {
		select {
		case <-w.ctx.Done():
			return false
		case <-timer.C:
			return true
		case message := <-w.outbound:
			*pending = append(*pending, message)
		}
	}
}

func (w *streamConnectionWorker) runConnectedSession(
	pending *[]ClientMessage,
	ready *bool,
) (sessionOutcome, error) {
	dialCtx, cancelDial := context.WithTimeout(w.ctx, ioDialTimeout)
	defer cancelDial()

	header := http.Header{}
	if w.apiKey == "" {
		return sessionOutcomeGracefulShutdown, invalidAPIKeyHeaderError(errors.New("empty api key"))
	}
	header.Set("x-api-key", w.apiKey)

	conn, _, err := websocket.Dial(dialCtx, w.endpoint, &websocket.DialOptions{
		HTTPHeader: header,
	})
	if err != nil {
		if errors.Is(dialCtx.Err(), context.Canceled) || errors.Is(dialCtx.Err(), context.DeadlineExceeded) {
			if w.ctx.Err() != nil {
				return sessionOutcomeGracefulShutdown, nil
			}
		}
		return sessionOutcomeGracefulShutdown, websocketError(err)
	}
	defer conn.CloseNow()

	firstServerMessage, err := readServerMessage(w.ctx, conn)
	if err != nil {
		if w.ctx.Err() != nil {
			return sessionOutcomeGracefulShutdown, nil
		}
		return sessionOutcomeGracefulShutdown, err
	}

	if _, ok := firstServerMessage.(HelloOkServerMessage); !ok {
		return sessionOutcomeGracefulShutdown, protocolError("expected first server message to be hello_ok")
	}
	if !w.pushInbound(firstServerMessage) {
		return sessionOutcomeGracefulShutdown, nil
	}

	configureMessage := ConfigureClientMessage{
		WalletPubkeys: append([]string(nil), w.configure.WalletPubkeys...),
		Strategy:      w.configure.Strategy,
	}
	if err := writeClientMessage(w.ctx, conn, configureMessage); err != nil {
		if w.ctx.Err() != nil {
			return sessionOutcomeGracefulShutdown, nil
		}
		return sessionOutcomeGracefulShutdown, err
	}

	configuredMessage, err := readServerMessage(w.ctx, conn)
	if err != nil {
		if w.ctx.Err() != nil {
			return sessionOutcomeGracefulShutdown, nil
		}
		return sessionOutcomeGracefulShutdown, err
	}
	if !w.pushInbound(configuredMessage) {
		return sessionOutcomeGracefulShutdown, nil
	}

	if !*ready {
		w.signalReady(nil)
		*ready = true
	}

	for len(*pending) > 0 {
		next := (*pending)[0]
		if err := writeClientMessage(w.ctx, conn, next); err != nil {
			return sessionOutcomeReconnect, nil
		}
		*pending = (*pending)[1:]
	}

	readResults := make(chan readResult, 8)
	go streamReadLoop(w.ctx, conn, readResults)

	for {
		select {
		case <-w.ctx.Done():
			_ = conn.Close(websocket.StatusNormalClosure, "shutdown")
			return sessionOutcomeGracefulShutdown, nil
		case outbound := <-w.outbound:
			if err := writeClientMessage(w.ctx, conn, outbound); err != nil {
				*pending = prependPending(*pending, outbound)
				return sessionOutcomeReconnect, nil
			}
		case read, ok := <-readResults:
			if !ok {
				if w.ctx.Err() != nil {
					return sessionOutcomeGracefulShutdown, nil
				}
				return sessionOutcomeReconnect, nil
			}
			if read.err != nil {
				if w.ctx.Err() != nil {
					return sessionOutcomeGracefulShutdown, nil
				}
				return sessionOutcomeReconnect, nil
			}
			if !w.pushInbound(read.message) {
				return sessionOutcomeGracefulShutdown, nil
			}
		}
	}
}

type readResult struct {
	message ServerMessage
	err     error
}

func streamReadLoop(ctx context.Context, conn *websocket.Conn, out chan<- readResult) {
	defer close(out)
	for {
		message, err := readServerMessage(ctx, conn)
		if err != nil {
			select {
			case out <- readResult{err: err}:
			case <-ctx.Done():
			}
			return
		}

		select {
		case out <- readResult{message: message}:
		case <-ctx.Done():
			return
		}
	}
}

func readServerMessage(ctx context.Context, conn *websocket.Conn) (ServerMessage, error) {
	readCtx, cancel := context.WithTimeout(ctx, ioReadTimeout)
	defer cancel()

	typ, payload, err := conn.Read(readCtx)
	if err != nil {
		if websocket.CloseStatus(err) != -1 {
			return nil, websocketError(err)
		}
		if errors.Is(readCtx.Err(), context.Canceled) || errors.Is(readCtx.Err(), context.DeadlineExceeded) {
			return nil, websocketError(readCtx.Err())
		}
		return nil, websocketError(err)
	}

	if typ != websocket.MessageText {
		return nil, protocolError("received non-text websocket frame")
	}

	message, err := ServerMessageFromJSON(payload)
	if err != nil {
		return nil, jsonError(err)
	}
	return message, nil
}

func writeClientMessage(ctx context.Context, conn *websocket.Conn, message ClientMessage) error {
	encoded, err := ClientMessageToJSON(message)
	if err != nil {
		return jsonError(err)
	}

	writeCtx, cancel := context.WithTimeout(ctx, ioWriteTimeout)
	defer cancel()
	if err := conn.Write(writeCtx, websocket.MessageText, encoded); err != nil {
		return websocketError(err)
	}
	return nil
}

func (w *streamConnectionWorker) pushInbound(message ServerMessage) bool {
	select {
	case <-w.ctx.Done():
		return false
	case w.inbound <- message:
		return true
	}
}

func prependPending(pending []ClientMessage, next ClientMessage) []ClientMessage {
	pending = append(pending, nil)
	copy(pending[1:], pending[:len(pending)-1])
	pending[0] = next
	return pending
}
