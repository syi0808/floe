package authority

import (
	"testing"
	"time"
)

func TestClaimCleanupCannotMutateReplacementEntry(t *testing.T) {
	const id = "same-challenge-id"
	oldPending := &pendingChallenge{}
	newPending := &pendingChallenge{}
	oldAdmission := &admission{}
	newAdmission := &admission{}
	oldStage := &stage{id: id, result: []byte("old")}
	newStage := &stage{id: id, result: []byte("new")}
	engine := &Engine{
		pending:     map[string]*pendingChallenge{id: newPending},
		admissions:  map[string]*admission{id: newAdmission},
		stages:      map[string]*stage{id: newStage},
		stagedBytes: len(newStage.result),
	}

	engine.cancelAdmissionClaim(id, oldPending, oldAdmission)
	if engine.pending[id] != newPending || engine.admissions[id] != newAdmission {
		t.Fatal("stale admission cleanup changed the replacement entry")
	}
	engine.cancelReleaseClaim(id, oldStage)
	if engine.stages[id] != newStage || engine.stagedBytes != len(newStage.result) {
		t.Fatal("stale release cleanup changed the replacement stage or byte count")
	}
}

func TestExpiredClaimRetryCannotDeleteReplacementEntry(t *testing.T) {
	const id = "same-challenge-id"
	clock := &fixedClock{}
	clock.Advance(2)
	oldPending := &pendingChallenge{deadline: 1, state: challengeChecking}
	newPending := &pendingChallenge{deadline: 10}
	oldStage := &stage{id: id, deadline: 1, result: []byte("old"), state: challengeChecking}
	newStage := &stage{id: id, deadline: 10, result: []byte("new")}
	engine := &Engine{
		clock:       clock,
		pending:     map[string]*pendingChallenge{id: newPending},
		admissions:  map[string]*admission{},
		stages:      map[string]*stage{id: newStage},
		stagedBytes: len(newStage.result),
	}

	engine.retryAdmissionClaim(id, oldPending)
	engine.retryReleaseClaim(id, oldStage)
	if engine.pending[id] != newPending || engine.stages[id] != newStage || engine.stagedBytes != len(newStage.result) {
		t.Fatal("expired stale claim cleanup changed a replacement entry")
	}

	engine.pending[id] = oldPending
	engine.stages[id] = oldStage
	engine.stagedBytes = len(oldStage.result)
	engine.retryAdmissionClaim(id, oldPending)
	engine.retryReleaseClaim(id, oldStage)
	if _, ok := engine.pending[id]; ok {
		t.Fatal("expired admission claim was restored")
	}
	if _, ok := engine.stages[id]; ok || engine.stagedBytes != 0 {
		t.Fatal("expired release claim was restored or retained its staged bytes")
	}
}

type fixedClock struct{ elapsed int64 }

func (clock *fixedClock) Now() time.Time { return time.Unix(clock.elapsed, 0) }
func (clock *fixedClock) Monotonic() time.Duration {
	return time.Duration(clock.elapsed) * time.Second
}
func (clock *fixedClock) Advance(seconds int64) { clock.elapsed += seconds }
