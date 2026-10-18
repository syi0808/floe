package inference

import (
	"context"
	"floe/server/internal/trust"
	"sync"
	"testing"
	"time"
)

func awaitWrite(t *testing.T, s *Service, ticket, operation, phase string) LiveWriteStatus {
	t.Helper()
	deadline := time.Now().Add(time.Second)
	for time.Now().Before(deadline) {
		out, err := s.LiveWriteStatus(context.Background(), trust.Principal{}, ticket, operation)
		if err == nil && out.Phase == phase {
			return out
		}
		time.Sleep(time.Millisecond)
	}
	t.Fatal("write did not settle", phase)
	return LiveWriteStatus{}
}
func TestLiveAppendExactConcurrentReplayAndObserverCancellation(t *testing.T) {
	s, a, _ := controlFixture(t, false, false)
	a.transport.writeRelease = make(chan struct{})
	ticket, start := controlTicket(t, s)
	if _, err := s.StartLiveCall(context.Background(), trust.Principal{}, start); err != nil {
		t.Fatal(err)
	}
	awaitControl(t, s, ticket, "ready")
	in := LiveAppend{OperationID: trust.NewID(), Kind: "presentable_result", DelegationID: "delegation", Text: "canonical synthetic result"}
	observer, cancel := context.WithCancel(context.Background())
	if _, err := s.AppendLiveCall(observer, trust.Principal{}, ticket.ID, in); err != nil {
		t.Fatal(err)
	}
	cancel()
	var wg sync.WaitGroup
	for i := 0; i < 24; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := s.AppendLiveCall(context.Background(), trust.Principal{}, ticket.ID, in); err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	changed := in
	changed.Text = "changed"
	if _, err := s.AppendLiveCall(context.Background(), trust.Principal{}, ticket.ID, changed); err == nil {
		t.Fatal("changed replay admitted")
	}
	other := in
	other.OperationID = trust.NewID()
	if _, err := s.AppendLiveCall(context.Background(), trust.Principal{}, ticket.ID, other); liveCause(err) != ModelBusy {
		t.Fatal("concurrent writer admitted", err)
	}
	close(a.transport.writeRelease)
	settled := awaitWrite(t, s, ticket.ID, in.OperationID, "written")
	if !settled.Attempted || settled.CompletedFrames != 1 || a.transport.sentWrites.Load() != 1 || a.transport.preparedWrites.Load() != 1 {
		t.Fatal("duplicate provider append")
	}
}
func TestLiveUncertainAppendNeverRetriesAndCloseIsOneShot(t *testing.T) {
	t.Run("uncertain", func(t *testing.T) {
		s, a, _ := controlFixture(t, false, false)
		a.transport.writeFailure = true
		ticket, start := controlTicket(t, s)
		if _, err := s.StartLiveCall(context.Background(), trust.Principal{}, start); err != nil {
			t.Fatal(err)
		}
		awaitControl(t, s, ticket, "ready")
		in := LiveAppend{OperationID: trust.NewID(), Kind: "presentable_result", Text: "synthetic result"}
		if _, err := s.AppendLiveCall(context.Background(), trust.Principal{}, ticket.ID, in); err != nil {
			t.Fatal(err)
		}
		status := awaitWrite(t, s, ticket.ID, in.OperationID, "uncertain")
		if !status.Attempted || status.CompletedFrames != 1 || !a.transport.closed.Load() {
			t.Fatal("uncertain write erased actual frame/fence evidence")
		}
		replay, err := s.AppendLiveCall(context.Background(), trust.Principal{}, ticket.ID, in)
		if err != nil || replay != status || a.transport.sentWrites.Load() != 1 {
			t.Fatal("uncertain append reissued", err)
		}
	})
	t.Run("close", func(t *testing.T) {
		s, a, _ := controlFixture(t, false, false)
		ticket, start := controlTicket(t, s)
		if _, err := s.StartLiveCall(context.Background(), trust.Principal{}, start); err != nil {
			t.Fatal(err)
		}
		awaitControl(t, s, ticket, "ready")
		operation := trust.NewID()
		for i := 0; i < 4; i++ {
			if _, err := s.CloseLiveCall(context.Background(), trust.Principal{}, ticket.ID, operation); err != nil {
				t.Fatal(err)
			}
		}
		status := awaitWrite(t, s, ticket.ID, operation, "written")
		if !status.Attempted || a.transport.closeWrites.Load() != 1 || !a.transport.closed.Load() {
			t.Fatal("close write evidence invalid")
		}
		if _, err := s.CloseLiveCall(context.Background(), trust.Principal{}, ticket.ID, trust.NewID()); err == nil {
			t.Fatal("second close operation admitted")
		}
		if _, err := s.AppendLiveCall(context.Background(), trust.Principal{}, ticket.ID, LiveAppend{OperationID: trust.NewID(), Kind: "progress", Text: "late"}); err == nil {
			t.Fatal("append admitted after close")
		}
		awaitControl(t, s, ticket, "closed_local")
	})
}

func TestLiveAppendIdentityCapacityNeverEvictsPriorReceipts(t *testing.T) {
	s, a, _ := controlFixture(t, false, false)
	ticket, start := controlTicket(t, s)
	if _, err := s.StartLiveCall(context.Background(), trust.Principal{}, start); err != nil {
		t.Fatal(err)
	}
	awaitControl(t, s, ticket, "ready")
	e, err := s.live.lookup(trust.Principal{}, ticket.ID)
	if err != nil {
		t.Fatal(err)
	}
	original := LiveAppend{OperationID: trust.NewID(), Kind: "progress", Text: "synthetic progress"}
	fingerprint, err := writeFingerprint(original)
	if err != nil {
		t.Fatal(err)
	}
	e.mu.Lock()
	e.writes[original.OperationID] = &liveWriteEntry{fingerprint: fingerprint, status: LiveWriteStatus{OperationID: original.OperationID, Phase: "written", Attempted: true, CompletedFrames: 1}}
	for len(e.writes) < maxLiveWriteRecords {
		id := trust.NewID()
		e.writes[id] = &liveWriteEntry{status: LiveWriteStatus{OperationID: id, Phase: "written"}}
	}
	e.mu.Unlock()
	next := original
	next.OperationID = trust.NewID()
	if _, err = s.AppendLiveCall(context.Background(), trust.Principal{}, ticket.ID, next); liveCause(err) != QuotaExceeded {
		t.Fatal("identity capacity did not fail explicitly", err)
	}
	replay, err := s.AppendLiveCall(context.Background(), trust.Principal{}, ticket.ID, original)
	if err != nil || replay.Phase != "written" || a.transport.sentWrites.Load() != 0 || a.transport.preparedWrites.Load() != 0 {
		t.Fatal("capacity evicted/reissued old operation", err)
	}
}
