package inference

import (
	"context"
	"encoding/json"
	"errors"
	"floe/server/internal/trust"
	"time"
)

// ErrLiveObservationPending means only this owner read window elapsed.
// It never represents provider timeout or retired call authority.
var ErrLiveObservationPending = errors.New("live observation pending")

// This proof identifies one retained transport observation. It is not evidence
// that Voice has persisted it: only the client owner may acknowledge custody.
type LiveEventObservation struct {
	Delivery LiveDelivery
	Digest   string
}

func (LiveEventObservation) String() string     { return "LiveEventObservation[redacted]" }
func (v LiveEventObservation) GoString() string { return v.String() }

// acquireDelivery bounds each call to one pending queue observation/ACK while
// retaining independent status/release access. Provider queue mutation remains
// serialized, so a raced read cannot replace the proof for the current head.
func (r *liveRegistry) acquireDelivery(p trust.Principal, id string) (*liveEntry, LiveTransport, func(), error) {
	e, err := r.lookup(p, id)
	if err != nil {
		return nil, nil, nil, err
	}
	select {
	case e.deliveryGate <- struct{}{}:
	default:
		return nil, nil, nil, Failure{Code: ModelBusy}
	}
	r.mu.Lock()
	e.mu.Lock()
	if r.closed || e.retired || e.transport == nil || e.controlCtx == nil || e.controlCtx.Err() != nil {
		e.mu.Unlock()
		r.mu.Unlock()
		<-e.deliveryGate
		return nil, nil, nil, Failure{Code: ModelUnavailable}
	}
	transport := e.transport
	e.workers++
	r.wg.Add(1)
	e.mu.Unlock()
	r.mu.Unlock()
	done := func() { <-e.deliveryGate; r.workerDone(e) }
	return e, transport, done, nil
}

// ObserveLiveEvent never consumes queued data. Observer timeout/disconnection
// cancels only this bounded read; the transport and retained head survive.
// Local transport failure can still drain received events while authority lives.
func (s *Service) ObserveLiveEvent(ctx context.Context, p trust.Principal, id string) (LiveEventObservation, error) {
	r := s.live
	e, transport, done, err := r.acquireDelivery(p, id)
	if err != nil {
		return LiveEventObservation{}, err
	}
	defer done()
	observer, cancel := context.WithTimeout(ctx, 20*time.Second)
	stop := context.AfterFunc(e.controlCtx, cancel)
	defer stop()
	defer cancel()
	if err = r.source(observer, e, false); err != nil {
		return LiveEventObservation{}, err
	}
	delivery, err := transport.Observe(observer)
	if err != nil {
		if errors.Is(observer.Err(), context.DeadlineExceeded) && ctx.Err() == nil && e.controlCtx.Err() == nil {
			return LiveEventObservation{}, ErrLiveObservationPending
		}
		return LiveEventObservation{}, err
	}
	if err = delivery.Validate(); err != nil {
		r.fail(e, InvalidOutput)
		return LiveEventObservation{}, Failure{Code: InvalidOutput}
	}
	encoded, err := json.Marshal(delivery)
	if err != nil {
		r.fail(e, InvalidOutput)
		return LiveEventObservation{}, Failure{Code: InvalidOutput}
	}
	digest := trust.Digest(string(encoded))
	if err = r.source(observer, e, false); err != nil {
		return LiveEventObservation{}, err
	}
	if err = observer.Err(); err != nil {
		return LiveEventObservation{}, err
	}
	e.mu.Lock()
	current := !e.retired && e.controlCtx.Err() == nil && time.Now().Before(e.deadline)
	valid := delivery.Sequence == e.acknowledgedSequence+1 && (e.pendingSequence == 0 || e.pendingSequence == delivery.Sequence && e.pendingDigest == digest)
	if current && valid {
		e.pendingSequence = delivery.Sequence
		e.pendingDigest = digest
	}
	e.mu.Unlock()
	if !current {
		return LiveEventObservation{}, Failure{Code: ModelUnavailable}
	}
	if !valid {
		r.fail(e, InvalidOutput)
		return LiveEventObservation{}, Failure{Code: InvalidOutput}
	}
	return LiveEventObservation{Delivery: delivery, Digest: digest}, nil
}

// AcknowledgeLiveEvent advances only the exact head previously observed through
// this owner. An exact retry of the last completed ACK is read-only, including
// response loss. It cannot ACK an unseen sequence or different event payload.
func (s *Service) AcknowledgeLiveEvent(ctx context.Context, p trust.Principal, id string, sequence uint64, digest string) error {
	if sequence == 0 || sequence > 9007199254740991 || !ValidHex(digest, 32) {
		return Failure{Code: Validation}
	}
	r := s.live
	e, transport, done, err := r.acquireDelivery(p, id)
	if err != nil {
		return err
	}
	defer done()
	if err = r.source(ctx, e, false); err != nil {
		return err
	}
	if err = ctx.Err(); err != nil {
		return err
	}
	e.mu.Lock()
	if e.acknowledgedSequence == sequence && e.acknowledgedDigest == digest {
		e.mu.Unlock()
		return nil
	}
	match := !e.retired && e.controlCtx.Err() == nil && e.pendingSequence == sequence && e.pendingDigest == digest
	e.mu.Unlock()
	if !match {
		return Failure{Code: RequestRejected}
	}
	if err = transport.Acknowledge(sequence); err != nil {
		return err
	}
	// If authority retires concurrently, the already-issued queue ACK is still
	// recorded. This is local disposal only; it never creates external work.
	e.mu.Lock()
	e.acknowledgedSequence = sequence
	e.acknowledgedDigest = digest
	e.pendingSequence = 0
	e.pendingDigest = ""
	e.mu.Unlock()
	return nil
}
