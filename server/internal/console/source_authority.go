package console

import (
	"errors"

	"floe/server/internal/authorization"
)

func (console *Console) WithCurrentSource(principal authorization.Principal, reference authorization.SourceReference, consume func(authorization.SourceSnapshot) error) error {
	if consume == nil {
		return errors.New("source consumer required")
	}
	console.mu.Lock()
	defer console.mu.Unlock()
	if console.trustUnavailable.Load() {
		return errors.New("source authority unavailable")
	}
	client, exists := console.state.Clients[principal.ClientID]
	if !exists || client.PersonID != principal.PersonID || client.DeviceID != principal.DeviceID || !principal.Authenticated {
		return errors.New("pairing unavailable")
	}
	record, exists := console.state.Connections[reference.ConnectionID]
	if !exists || record.ConnectorID != reference.ConnectorID || record.PersonID != principal.PersonID || record.Incarnation != reference.Incarnation || record.Epoch != reference.Epoch || record.Epoch == 0 || record.Incarnation == "" || record.ProviderIdentity == "" || record.IdentityUnverified || record.Device != nil && record.Device.DeviceID != principal.DeviceID || reference.ExecutionOwner != console.state.ExecutionOwnerID {
		return errors.New("source unavailable")
	}
	snapshot := authorization.SourceSnapshot{SourceReference: reference, PersonID: record.PersonID, Active: true}
	return consume(snapshot)
}
