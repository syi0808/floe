package codexauth

import (
	"context"
	"errors"
	"floe/server/internal/inference"
	"fmt"
	"testing"
	"time"
)

func deliveryEvent(id string) inference.LiveEvent {
	return inference.LiveEvent{Kind: "person_utterance_final", UtteranceID: id, Text: "synthetic retained text"}
}
func TestLiveDeliveryObservationCancellationAndAckReplayNeverConsumeAnotherEvent(t *testing.T) {
	q := newLiveDeliveryQueue()
	if err := q.push(context.Background(), deliveryEvent("one")); err != nil {
		t.Fatal(err)
	}
	if err := q.push(context.Background(), deliveryEvent("two")); err != nil {
		t.Fatal(err)
	}
	cancelled, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := q.observe(cancelled); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	first, err := q.observe(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	replay, err := q.observe(context.Background())
	if err != nil || replay.Sequence != first.Sequence {
		t.Fatal("observer consumed delivery")
	}
	if err := q.acknowledge(2); !errors.Is(err, errLiveDelivery) {
		t.Fatal("out-of-order ack accepted")
	}
	if err := q.acknowledge(0); !errors.Is(err, errLiveDelivery) {
		t.Fatal("zero ack accepted")
	}
	if err := q.acknowledge(first.Sequence); err != nil {
		t.Fatal(err)
	}
	if err := q.acknowledge(first.Sequence); err != nil {
		t.Fatal(err)
	}
	second, err := q.observe(context.Background())
	if err != nil || second.Event.UtteranceID != "two" {
		t.Fatal("ack replay removed next event")
	}
	q.finish(errLiveClosed)
	still, err := q.observe(context.Background())
	if err != nil || still.Sequence != second.Sequence {
		t.Fatal("close discarded pending custody")
	}
	if err := q.acknowledge(second.Sequence); err != nil {
		t.Fatal(err)
	}
	if _, err := q.observe(context.Background()); !errors.Is(err, errLiveClosed) {
		t.Fatal("missing terminal result")
	}
}
func TestLiveDeliveryRetainsOneInFlightTailWhenFullQueueIsCancelled(t *testing.T) {
	q := newLiveDeliveryQueue()
	for i := 0; i < maxLivePendingDeliveries; i++ {
		if err := q.push(context.Background(), deliveryEvent(fmt.Sprint(i))); err != nil {
			t.Fatal(err)
		}
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- q.push(ctx, deliveryEvent("tail")) }()
	select {
	case <-done:
		t.Fatal("full queue did not backpressure reader")
	case <-time.After(20 * time.Millisecond):
	}
	cancel()
	select {
	case err := <-done:
		if !errors.Is(err, context.Canceled) {
			t.Fatal(err)
		}
	case <-time.After(time.Second):
		t.Fatal("backpressure ignored cancellation")
	}
	q.finish(errLiveClosed)
	for i := 0; i <= maxLivePendingDeliveries; i++ {
		d, err := q.observe(context.Background())
		if err != nil {
			t.Fatal(err)
		}
		if i == maxLivePendingDeliveries && d.Event.UtteranceID != "tail" {
			t.Fatal("lost received tail")
		}
		if err := q.acknowledge(d.Sequence); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := q.observe(context.Background()); !errors.Is(err, errLiveClosed) {
		t.Fatal(err)
	}
}
func TestLiveDeliveryCopiesOptionalTextAndRedactsDebug(t *testing.T) {
	q := newLiveDeliveryQueue()
	hint := "synthetic private hint"
	event := inference.LiveEvent{Kind: "delegation_requested", DelegationID: "delegation", InputHint: &hint}
	if err := q.push(context.Background(), event); err != nil {
		t.Fatal(err)
	}
	hint = "changed producer"
	d, _ := q.observe(context.Background())
	*d.Event.InputHint = "changed observer"
	replay, _ := q.observe(context.Background())
	if *replay.Event.InputHint != "synthetic private hint" {
		t.Fatal("mutable pointer leaked into custody")
	}
	for _, format := range []string{"%v", "%+v", "%#v"} {
		if fmt.Sprintf(format, replay) != "LiveDelivery[redacted]" {
			t.Fatal("delivery debug disclosed content")
		}
	}
}
