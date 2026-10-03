package trust

import (
	"context"
	"floe/server/internal/operation"
	"sync"
	"time"
)

// RevocationCleanup is the durable handoff to the source lifecycle owner.
type RevocationCleanup interface {
	ApplyRevocation(context.Context, CleanupTicket) error
	ResumeCleanup(context.Context) error
}
type PairingCleanup interface {
	ClearClient(context.Context, string) error
}
type ClientAdministration struct {
	trust   *Service
	cleanup RevocationCleanup
	pairing PairingCleanup
	ctx     context.Context
	cancel  context.CancelFunc
	mu      sync.Mutex
	closed  bool
	active  sync.WaitGroup
}

func NewClientAdministration(t *Service, cleanup RevocationCleanup, pairing PairingCleanup) *ClientAdministration {
	ctx, cancel := context.WithCancel(context.Background())
	return &ClientAdministration{trust: t, cleanup: cleanup, pairing: pairing, ctx: ctx, cancel: cancel}
}
func (a *ClientAdministration) Revoke(ctx context.Context, p OperatorPrincipal, id string) operation.Result {
	a.mu.Lock()
	if a.closed {
		a.mu.Unlock()
		return operation.Reject(operation.Unavailable, "closing")
	}
	a.active.Add(1)
	a.mu.Unlock()
	defer a.active.Done()
	var receipt RevocationReceipt
	err := a.trust.WithCurrentOperator(p, func() error {
		var err error
		receipt, err = a.trust.RevokeClient(ctx, id)
		return err
	})
	if err != nil {
		return Result(err)
	}
	credentialErr := a.pairing.ClearClient(ctx, id)
	if err = a.cleanup.ApplyRevocation(ctx, receipt.Cleanup); err != nil {
		return operation.Reject(operation.Unavailable, "connection_cleanup_pending")
	}
	a.mu.Lock()
	if !a.closed {
		a.active.Add(1)
		go func() {
			defer a.active.Done()
			work, cancel := context.WithTimeout(a.ctx, 40*time.Second)
			defer cancel()
			_ = a.cleanup.ResumeCleanup(work)
		}()
	}
	a.mu.Unlock()
	if credentialErr != nil {
		return operation.Reject(operation.Unavailable, "pairing_credential_cleanup_pending")
	}
	return operation.Accept(map[string]any{"ok": true, "cleanup": "pending"})
}
func (a *ClientAdministration) Close() {
	a.mu.Lock()
	a.closed = true
	a.cancel()
	a.mu.Unlock()
	a.active.Wait()
}
