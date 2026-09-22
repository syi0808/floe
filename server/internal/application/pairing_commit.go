package application

import (
	"time"

	"floe/server/internal/authorization"
	"floe/server/internal/operation"
	"floe/server/internal/pairing"
)

func (console *Console) commitPairingActivation(record authorization.IssuerRecord, tokenHash string, pending pairing.Pending) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if _, exists := console.state.Clients[pending.ID]; exists {
		return authorization.ErrConflict
	}
	if _, exists := console.state.TrustedIssuers[record.KeyID]; exists || console.state.RevokedIssuerKeys[record.KeyID] {
		return authorization.ErrConflict
	}
	if len(console.state.TrustedIssuers)+len(console.state.RevokedIssuerKeys) >= maxRetainedIssuerIdentities {
		return authorization.ErrConflict
	}
	next := cloneState(console.state)
	next.Clients[pending.ID] = pairedClient{
		ClientID: pending.ID, TokenHash: tokenHash, PersonID: pending.PersonID, DeviceID: pending.DeviceID,
		ProducerInstanceID: console.state.InstanceID, ProducerFingerprint: pending.ProducerFingerprint,
		ProducerAudience: pending.ProducerAudience,
	}
	next.TrustedIssuers[record.KeyID] = trustedIssuerRecord{
		KeyID: record.KeyID, EnrollmentID: record.EnrollmentID, ClientID: pending.ID,
		PersonID: pending.PersonID, DeviceID: pending.DeviceID, PublicKey: append([]byte(nil), record.PublicKey...),
	}
	if err := console.save(next); err != nil {
		return err
	}
	console.state = next
	return nil
}

func (console *Console) preparePairing(person string) operation.Result {
	console.mu.Lock()
	defer console.mu.Unlock()
	for personID := range console.state.Cleanups {
		if err := console.retryPersonCleanupLocked(personID); err != nil {
			return operation.Reject(operation.Unavailable, "person_cleanup_pending")
		}
	}
	if len(console.state.Clients) >= 16 {
		return operation.Reject(operation.Conflict, "too_many_clients")
	}
	for _, client := range console.state.Clients {
		if client.PersonID != person {
			return operation.Reject(operation.Conflict, "person_mismatch")
		}
	}
	return operation.Accept(nil)
}

func (console *Console) commitLegacyPairing(pending pairing.Pending, tokenHash string) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	next := cloneState(console.state)
	next.Clients[pending.ID] = pairedClient{ClientID: pending.ID, TokenHash: tokenHash, PersonID: pending.PersonID, DeviceID: pending.DeviceID}
	if err := console.save(next); err != nil {
		return err
	}
	console.state = next
	return nil
}

func (console *Console) newPairing(allowLegacy bool, clock func() time.Time) *pairing.Operations {
	return pairing.NewOperations(pairing.Host{
		Prepare: console.preparePairing, Commit: console.commitPairingActivation,
		CommitLegacy: console.commitLegacyPairing, Engine: console.authorityEngine,
		Metadata:    console.producerMetadata,
		Sign:        func(value []byte) []byte { return console.admissions.Producer().SignChallenge(value) },
		Fingerprint: func() string { return console.admissions.Producer().Fingerprint() },
	}, allowLegacy, clock)
}
