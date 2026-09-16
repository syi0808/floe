package pairing

import (
	"sync"
	"time"
)

// Operations owns the pairing Operation for this server.
//
// Only this package mutates the pending Operation, its rate window and the
// legacy-pairing allowance. No other owner shares this mutex.
type Operations struct {
	mu          sync.Mutex
	pending     *Pending
	lastPair    time.Time
	allowLegacy bool
}

// Pending is the pairing Operation currently awaiting approval.
type Pending struct {
	ID          string
	Code        string
	Proof       string
	Fingerprint string
	CreatedAt   time.Time
	Approved    bool
}

func NewOperations(allowLegacy bool) *Operations {
	return &Operations{allowLegacy: allowLegacy}
}

// Pending returns the Operation awaiting approval, if any.
func (operations *Operations) Pending() (Pending, bool) {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	if operations.pending == nil {
		return Pending{}, false
	}
	return *operations.pending, true
}

// Start replaces the pending Operation. The caller has already admitted the
// request; this only records the Operation this server is driving.
func (operations *Operations) Start(pending Pending, now time.Time) {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	operations.pending = &pending
	operations.lastPair = now
}

// Clear ends the pending Operation so a late approval cannot settle it.
func (operations *Operations) Clear() {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	operations.pending = nil
}

// LastStarted reports when a pairing Operation last started, for rate limiting.
func (operations *Operations) LastStarted() time.Time {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	return operations.lastPair
}

// AllowsLegacy reports whether unsigned legacy pairing is still accepted.
func (operations *Operations) AllowsLegacy() bool { return operations.allowLegacy }
