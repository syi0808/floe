package pairing

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"time"

	"floe/server/internal/authorization"
)

func (console *Console) commitPairingActivation(record authorization.IssuerRecord, tokenHash string, pending pairing) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if console.pair == nil || console.pair.ID != pending.ID || !console.pair.LocalConfirmed || console.pair.IssuerKeyID != record.KeyID || console.pair.IssuerFingerprint != issuerFingerprint(record.PublicKey) || record.EnrollmentID != pending.enrollmentID || console.producer.fingerprint() != pending.ProducerFingerprint {
		return authorization.ErrConflict
	}
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

func issuerFingerprint(publicKey ed25519.PublicKey) string {
	digest := sha256.Sum256(publicKey)
	return hex.EncodeToString(digest[:])
}

func now() time.Time { return time.Now() }
