package application

import (
	"errors"

	"floe/server/internal/authorization"
)

type consoleTrustStore struct{ console *Console }

func (store consoleTrustStore) LoadIssuers() ([]authorization.IssuerRecord, error) {
	store.console.mu.Lock()
	defer store.console.mu.Unlock()
	if store.console.admissions.TrustUnavailable() {
		return nil, errors.New("trust unavailable")
	}
	if len(store.console.state.RevokedIssuerKeys) > maxRetainedIssuerIdentities || len(store.console.state.TrustedIssuers) > maxRetainedIssuerIdentities || len(store.console.state.RevokedIssuerKeys)+len(store.console.state.TrustedIssuers) > maxRetainedIssuerIdentities {
		return nil, errors.New("invalid trusted issuer state")
	}
	records := make([]authorization.IssuerRecord, 0, len(store.console.state.TrustedIssuers))
	for mapKey, trusted := range store.console.state.TrustedIssuers {
		client, paired := store.console.state.Clients[trusted.ClientID]
		if mapKey != trusted.KeyID || trusted.KeyID == "" || len(trusted.PublicKey) == 0 || store.console.state.RevokedIssuerKeys[trusted.KeyID] || !paired || client.ClientID != trusted.ClientID || client.PersonID != trusted.PersonID || client.DeviceID != trusted.DeviceID {
			return nil, errors.New("invalid trusted issuer state")
		}
		records = append(records, authorization.IssuerRecord{KeyID: trusted.KeyID, EnrollmentID: trusted.EnrollmentID, Principal: authorization.Principal{ClientID: trusted.ClientID, PersonID: trusted.PersonID, DeviceID: trusted.DeviceID, Authenticated: true}, PublicKey: append([]byte(nil), trusted.PublicKey...)})
	}
	return records, nil
}

func (store consoleTrustStore) IsIssuerKeyRevoked(keyID string) (bool, error) {
	store.console.mu.Lock()
	defer store.console.mu.Unlock()
	if store.console.admissions.TrustUnavailable() {
		return false, errors.New("trust unavailable")
	}
	return store.console.state.RevokedIssuerKeys[keyID], nil
}

func (store consoleTrustStore) CommitIssuerActivation(record authorization.IssuerRecord) error {
	store.console.mu.Lock()
	defer store.console.mu.Unlock()
	if store.console.admissions.TrustUnavailable() {
		return errors.New("trust unavailable")
	}
	if store.console.state.RevokedIssuerKeys[record.KeyID] {
		return errors.New("issuer key is revoked")
	}
	client, paired := store.console.state.Clients[record.Principal.ClientID]
	if !paired || client.ClientID != record.Principal.ClientID || client.PersonID != record.Principal.PersonID || client.DeviceID != record.Principal.DeviceID {
		return errors.New("paired client unavailable")
	}
	if _, exists := store.console.state.TrustedIssuers[record.KeyID]; exists {
		return errors.New("issuer key already enrolled")
	}
	if len(store.console.state.TrustedIssuers)+len(store.console.state.RevokedIssuerKeys) >= maxRetainedIssuerIdentities {
		return errors.New("issuer identity retention limit")
	}
	for _, trusted := range store.console.state.TrustedIssuers {
		if trusted.ClientID == record.Principal.ClientID && trusted.PersonID == record.Principal.PersonID && trusted.DeviceID == record.Principal.DeviceID {
			return errors.New("principal already enrolled")
		}
	}
	next := cloneState(store.console.state)
	next.TrustedIssuers[record.KeyID] = trustedIssuerRecord{KeyID: record.KeyID, EnrollmentID: record.EnrollmentID, ClientID: record.Principal.ClientID, PersonID: record.Principal.PersonID, DeviceID: record.Principal.DeviceID, PublicKey: append([]byte(nil), record.PublicKey...)}
	if err := store.console.save(next); err != nil {
		if isIndeterminatePrivateWrite(err) {
			store.console.latchTrustUnavailable()
		}
		return err
	}
	store.console.state = next
	return nil
}

func (store consoleTrustStore) CommitIssuerRevocation(record authorization.IssuerRecord) error {
	store.console.mu.Lock()
	defer store.console.mu.Unlock()
	if store.console.admissions.TrustUnavailable() {
		return errors.New("trust unavailable")
	}
	if _, exists := store.console.state.TrustedIssuers[record.KeyID]; !exists {
		return errors.New("issuer key not enrolled")
	}
	next := cloneState(store.console.state)
	delete(next.TrustedIssuers, record.KeyID)
	next.RevokedIssuerKeys[record.KeyID] = true
	if err := store.console.save(next); err != nil {
		if isIndeterminatePrivateWrite(err) {
			store.console.latchTrustUnavailable()
		}
		return err
	}
	store.console.state = next
	return nil
}
