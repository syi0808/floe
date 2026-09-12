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
	if record.ConnectorID == "calendar.google" || record.ConnectorID == "calendar.microsoft" {
		identityRuntime := console.calendarIdentityRuntimeLocked(record.ConnectorID)
		fenceRuntime, ok := identityRuntime.(ProviderIdentityFenceRuntime)
		if !ok {
			return errors.New("source identity unavailable")
		}
		return fenceRuntime.WithVerifiedProviderIdentity(record.Credential, record.ProviderIdentity, func() error {
			snapshot := authorization.SourceSnapshot{SourceReference: reference, PersonID: record.PersonID, Active: true}
			return consume(snapshot)
		})
	}
	snapshot := authorization.SourceSnapshot{SourceReference: reference, PersonID: record.PersonID, Active: true}
	return consume(snapshot)
}

func (console *Console) calendarIdentityRuntimeLocked(connectorID string) ConnectorOAuthRuntime {
	switch connectorID {
	case "calendar.google":
		return console.calendarAuth
	case "calendar.microsoft":
		return console.microsoftCalendarAuth
	default:
		return nil
	}
}
