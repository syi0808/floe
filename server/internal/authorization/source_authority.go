package authorization

import (
	"errors"

	"floe/server/internal/connections"
)

func ValidateCurrentSource(principal, paired Principal, reference SourceReference, record connections.Record, executionOwner string) error {
	if !principal.Authenticated || !paired.Authenticated || paired.ClientID != principal.ClientID || paired.PersonID != principal.PersonID || paired.DeviceID != principal.DeviceID {
		return errors.New("pairing unavailable")
	}
	if record.ConnectionID != reference.ConnectionID || record.ConnectorID != reference.ConnectorID || record.PersonID != principal.PersonID || record.Incarnation != reference.Incarnation || record.Epoch != reference.Epoch || record.Epoch == 0 || record.Incarnation == "" || record.ProviderIdentity == "" || record.IdentityUnverified || record.Device != nil && record.Device.DeviceID != principal.DeviceID || reference.ExecutionOwner != executionOwner {
		return errors.New("source unavailable")
	}
	return nil
}
