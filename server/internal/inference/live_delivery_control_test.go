package inference

import (
	"context"
	"errors"
	"floe/server/internal/trust"
	"testing"
	"time"
)

func TestLiveEventCustodyRequiresObservedExactProofAndRetainsAfterLocalClose(t *testing.T) {
	s, a, _ := controlFixture(t, false, false)
	a.transport.deliveries = []LiveDelivery{{Sequence: 1, Event: LiveEvent{Kind: "ready"}}, {Sequence: 2, Event: LiveEvent{Kind: "person_utterance_final", UtteranceID: "utterance-1", Text: "synthetic private text"}}}
	ticket, in := controlTicket(t, s)
	if _, err := s.StartLiveCall(context.Background(), trust.Principal{}, in); err != nil {
		t.Fatal(err)
	}
	awaitControl(t, s, ticket, "ready")
	ctx := context.Background()
	p := trust.Principal{}
	if err := s.AcknowledgeLiveEvent(ctx, p, ticket.ID, 1, trust.Digest("unseen")); err == nil {
		t.Fatal("unseen event acknowledged")
	}
	first, err := s.ObserveLiveEvent(ctx, p, ticket.ID)
	if err != nil {
		t.Fatal(err)
	}
	again, err := s.ObserveLiveEvent(ctx, p, ticket.ID)
	if err != nil || again.Digest != first.Digest || a.transport.acks.Load() != 0 {
		t.Fatal("observation consumed or changed queue head", err)
	}
	if err = s.AcknowledgeLiveEvent(ctx, p, ticket.ID, 1, trust.Digest("different")); err == nil {
		t.Fatal("different proof acknowledged")
	}
	if err = s.AcknowledgeLiveEvent(ctx, p, ticket.ID, 2, first.Digest); err == nil {
		t.Fatal("unobserved sequence acknowledged")
	}
	for i := 0; i < 3; i++ {
		if err = s.AcknowledgeLiveEvent(ctx, p, ticket.ID, 1, first.Digest); err != nil {
			t.Fatal(err)
		}
	}
	if a.transport.acks.Load() != 1 {
		t.Fatal("ACK retry re-consumed transport")
	}
	a.transport.Fence()
	awaitControl(t, s, ticket, "failed")
	final, err := s.ObserveLiveEvent(ctx, p, ticket.ID)
	if err != nil || final.Delivery.Sequence != 2 {
		t.Fatal("local close discarded retained final", err)
	}
	if err = s.AcknowledgeLiveEvent(ctx, p, ticket.ID, 2, final.Digest); err != nil {
		t.Fatal(err)
	}
	observer, cancel := context.WithTimeout(ctx, 10*time.Millisecond)
	defer cancel()
	if _, err = s.ObserveLiveEvent(observer, p, ticket.ID); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatal("empty read did not remain bounded", err)
	}
	if a.starts.Load() != 1 {
		t.Fatal("event observation restarted provider")
	}
}

func TestLiveEventReadCancellationPressureAndChangedHead(t *testing.T) {
	s, a, _ := controlFixture(t, false, false)
	ticket, in := controlTicket(t, s)
	if _, err := s.StartLiveCall(context.Background(), trust.Principal{}, in); err != nil {
		t.Fatal(err)
	}
	awaitControl(t, s, ticket, "ready")
	observer, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := s.ObserveLiveEvent(observer, trust.Principal{}, ticket.ID); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	if a.transport.closed.Load() {
		t.Fatal("read cancellation fenced owner")
	}
	e, err := s.live.lookup(trust.Principal{}, ticket.ID)
	if err != nil {
		t.Fatal(err)
	}
	e.deliveryGate <- struct{}{}
	if _, err = s.ObserveLiveEvent(context.Background(), trust.Principal{}, ticket.ID); liveCause(err) != ModelBusy {
		t.Fatal("unbounded pending observers", err)
	}
	if _, err = s.LiveCallStatus(context.Background(), trust.Principal{}, ticket.ID); err != nil {
		t.Fatal("event pressure blocked status", err)
	}
	<-e.deliveryGate
	a.transport.deliveryMu.Lock()
	a.transport.deliveries = []LiveDelivery{{Sequence: 1, Event: LiveEvent{Kind: "ready"}}}
	a.transport.deliveryMu.Unlock()
	if _, err = s.ObserveLiveEvent(context.Background(), trust.Principal{}, ticket.ID); err != nil {
		t.Fatal(err)
	}
	a.transport.deliveryMu.Lock()
	a.transport.deliveries[0].Event = LiveEvent{Kind: "person_utterance_final", UtteranceID: "u", Text: "changed"}
	a.transport.deliveryMu.Unlock()
	if _, err = s.ObserveLiveEvent(context.Background(), trust.Principal{}, ticket.ID); liveCause(err) != InvalidOutput || !a.transport.closed.Load() {
		t.Fatal("changed retained head escaped", err)
	}
}
