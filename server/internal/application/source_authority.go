package application

import (
	"errors"

	"floe/server/internal/authorization"
	"floe/server/internal/connections"
)

func (console *Console) WithCurrentSource(principal authorization.Principal, reference authorization.SourceReference, consume func(authorization.SourceSnapshot) error) error {
	if consume == nil {
		return errors.New("source consumer required")
	}
	console.mu.Lock()
	defer console.mu.Unlock()
	if console.admissions.TrustUnavailable() {
		return errors.New("source authority unavailable")
	}
	client, exists := console.state.Clients[principal.ClientID]
	paired := authorization.Principal{ClientID: principal.ClientID, PersonID: client.PersonID, DeviceID: client.DeviceID, Authenticated: exists}
	record := console.state.Connections[reference.ConnectionID]
	if err := authorization.ValidateCurrentSource(principal, paired, reference, record, console.state.ExecutionOwnerID); err != nil {
		return err
	}
	if record.ConnectorID == "calendar.google" || record.ConnectorID == "calendar.microsoft" {
		identityRuntime := console.calendarIdentityRuntimeLocked(record.ConnectorID)
		fenceRuntime, ok := identityRuntime.(connections.ProviderIdentityFenceRuntime)
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

func (console *Console) calendarIdentityRuntimeLocked(connectorID string) connections.ConnectorOAuthRuntime {
	switch connectorID {
	case "calendar.google":
		return console.calendarAuth
	case "calendar.microsoft":
		return console.microsoftCalendarAuth
	default:
		return nil
	}
}
