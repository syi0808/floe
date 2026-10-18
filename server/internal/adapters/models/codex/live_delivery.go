package codexauth

import (
	"context"
	"errors"
	"floe/server/internal/inference"
	"sync"
	"time"
)

// This bounds queued events, plus at most one already-received frame retained
// during cancellation of a backpressured owner. It is not a provider limit.
const maxLivePendingDeliveries = 64

var errLiveDelivery = errors.New("invalid live delivery acknowledgement")

// Sequence identifies local custody inside this exact call, never provider
// acceptance, a Conversation message, or a crash-persistent event log.
type LiveDelivery struct {
	Sequence uint64
	Event    inference.LiveEvent
}

func (LiveDelivery) String() string     { return "LiveDelivery[redacted]" }
func (d LiveDelivery) GoString() string { return d.String() }
func copyLiveEvent(e inference.LiveEvent) inference.LiveEvent {
	if e.UtteranceRef != nil {
		v := *e.UtteranceRef
		e.UtteranceRef = &v
	}
	if e.InputHint != nil {
		v := *e.InputHint
		e.InputHint = &v
	}
	return e
}

type liveDeliveryQueue struct {
	mu                   sync.Mutex
	changed              chan struct{}
	pending              []LiveDelivery
	issued, acknowledged uint64
	terminal             error
}

func newLiveDeliveryQueue() *liveDeliveryQueue {
	return &liveDeliveryQueue{changed: make(chan struct{})}
}
func (q *liveDeliveryQueue) signal() { close(q.changed); q.changed = make(chan struct{}) }
func (q *liveDeliveryQueue) push(ctx context.Context, event inference.LiveEvent) error {
	if err := event.Validate(); err != nil {
		return invalidOutput
	}
	for {
		q.mu.Lock()
		if q.terminal != nil {
			err := q.terminal
			q.mu.Unlock()
			return err
		}
		if len(q.pending) > maxLivePendingDeliveries {
			q.mu.Unlock()
			return errLiveCapacity
		}
		if len(q.pending) < maxLivePendingDeliveries || ctx.Err() != nil {
			if q.issued == 9007199254740991 {
				q.mu.Unlock()
				return errLiveCapacity
			}
			q.issued++
			q.pending = append(q.pending, LiveDelivery{q.issued, copyLiveEvent(event)})
			q.signal()
			q.mu.Unlock()
			// This event was already read from the wire. Retain it even when
			// cancellation arrives between receipt and queue admission.
			return ctx.Err()
		}
		changed := q.changed
		q.mu.Unlock()
		select {
		case <-ctx.Done():
			// Loop once to retain the single in-flight tail before stopping.
		case <-changed:
		}
	}
}
func (q *liveDeliveryQueue) observe(ctx context.Context) (LiveDelivery, error) {
	for {
		if err := ctx.Err(); err != nil {
			return LiveDelivery{}, err
		}
		q.mu.Lock()
		if len(q.pending) != 0 {
			d := q.pending[0]
			d.Event = copyLiveEvent(d.Event)
			q.mu.Unlock()
			return d, nil
		}
		if q.terminal != nil {
			err := q.terminal
			q.mu.Unlock()
			return LiveDelivery{}, err
		}
		changed := q.changed
		q.mu.Unlock()
		select {
		case <-ctx.Done():
			return LiveDelivery{}, ctx.Err()
		case <-changed:
		}
	}
}
func (q *liveDeliveryQueue) acknowledge(sequence uint64) error {
	q.mu.Lock()
	defer q.mu.Unlock()
	if sequence == 0 {
		return errLiveDelivery
	}
	if sequence <= q.acknowledged {
		return nil
	}
	if len(q.pending) == 0 || q.pending[0].Sequence != sequence {
		return errLiveDelivery
	}
	q.pending[0] = LiveDelivery{} // Do not retain text in the backing array.
	q.pending = q.pending[1:]
	q.acknowledged = sequence
	q.signal()
	return nil
}
func (q *liveDeliveryQueue) finish(err error) {
	q.mu.Lock()
	defer q.mu.Unlock()
	if q.terminal == nil {
		q.terminal = err
		q.signal()
	}
}
func (c *LiveCall) pumpDeliveries() {
	for {
		event, err := c.readNextWire()
		if err == nil {
			err = c.deliveries.push(c.ctx, *event)
		}
		if err != nil {
			c.deliveries.finish(err)
			c.CloseLocal()
			return
		}
	}
}

// The exact credential generation still gates retained data after transport
// loss. Local closure can drain previously received events, never restart I/O.
func (c *LiveCall) deliveryAuthorized() bool {
	if !time.Now().Before(c.expiresAt) {
		return false
	}
	r := c.runtime
	r.mu.RLock()
	defer r.mu.RUnlock()
	return !r.closed && !r.credentialMutation && r.credentialReadError == nil &&
		r.credentialGeneration == c.generation && accountIdentity(r.tokens) == c.identity
}

// Observe never removes an event. A cancelled UI/HTTP observer cannot cancel the
// owner reader or lose its pending event. Callers ACK only after local custody.
// Authenticated per-client routing and source release remain owner obligations;
// this adapter is not a public endpoint or a source authorization capability.
func (c *LiveCall) Observe(ctx context.Context) (LiveDelivery, error) {
	if !c.deliveryAuthorized() {
		return LiveDelivery{}, errLiveClosed
	}
	c.mu.Lock()
	attached := c.conn != nil
	c.mu.Unlock()
	if !attached {
		return LiveDelivery{}, unavailable
	}
	delivery, err := c.deliveries.observe(ctx)
	if !c.deliveryAuthorized() {
		return LiveDelivery{}, errLiveClosed
	}
	return delivery, err
}
func (c *LiveCall) Acknowledge(sequence uint64) error {
	if !c.deliveryAuthorized() {
		return errLiveClosed
	}
	return c.deliveries.acknowledge(sequence)
}

// Done signals the immediate local transport fence independently of queued
// custody. A composed owner must propagate this to native Stop without waiting
// for delivery ACKs; it does not prove provider close or final usage.
func (c *LiveCall) Done() <-chan struct{} { return c.ctx.Done() }
