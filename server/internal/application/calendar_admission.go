package application

import (
	"context"
	"errors"
	"reflect"
	"time"

	"floe/server/internal/connections"
)

func (console *Console) preflightCalendarIdentity(ctx context.Context, expected connections.Record) error {
	console.mu.Lock()
	current, exists := console.state.Connections[expected.ConnectionID]
	identityRuntime := console.calendarIdentityRuntimeLocked(expected.ConnectorID)
	console.mu.Unlock()
	if !exists || !sameCalendarRecord(current, expected) || current.IdentityUnverified || current.ProviderIdentity == "" || identityRuntime == nil {
		return errors.New("source identity unavailable")
	}
	provider, ok := identityRuntime.(connections.ProviderIdentityRuntime)
	if !ok {
		return errors.New("source identity unavailable")
	}
	identityContext, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	identity, err := provider.ProviderIdentity(identityContext)
	if err != nil || identity == "" {
		return errors.New("source identity unavailable")
	}
	console.mu.Lock()
	defer console.mu.Unlock()
	latest, exists := console.state.Connections[expected.ConnectionID]
	if !exists || !sameCalendarRecord(latest, expected) || latest.IdentityUnverified || latest.ProviderIdentity != identity {
		return errors.New("source identity changed")
	}
	return nil
}
func sameCalendarRecord(first, second connections.Record) bool {
	return first.ConnectionID == second.ConnectionID && first.Revision == second.Revision && first.ConnectorID == second.ConnectorID && first.PersonID == second.PersonID && first.Credential == second.Credential && first.Incarnation == second.Incarnation && first.Epoch == second.Epoch && first.ProviderIdentity == second.ProviderIdentity && first.IdentityUnverified == second.IdentityUnverified && reflect.DeepEqual(first.Device, second.Device) && reflect.DeepEqual(first.Scope, second.Scope)
}
func (console *Console) executionOwner() string {
	console.mu.Lock()
	defer console.mu.Unlock()
	return console.state.ExecutionOwnerID
}
