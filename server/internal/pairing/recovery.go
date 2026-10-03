package pairing

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"encoding/base64"
	"encoding/hex"
	"floe/server/internal/operation"
	"floe/server/internal/trust"
	"time"
)

// Recover is an explicit authenticated operator decision. Polling never calls
// this path. A lost activation acknowledgment is read before any mutation.
func (o *Operations) Recover(ctx context.Context, operator trust.OperatorPrincipal, in RecoveryRequest) operation.Result {
	ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	if in.SchemaVersion != 1 || !trust.ValidID(in.PairingID) || len(in.Fingerprint) != 64 || (in.Action != "resume" && in.Action != "abort") {
		return operation.Reject(operation.Invalid, "validation")
	}
	if err := o.trust.WithCurrentOperator(operator, func() error { return ctx.Err() }); err != nil {
		return trust.Result(err)
	}
	if err := o.lock(ctx); err != nil {
		return trust.Result(err)
	}
	defer o.unlock()
	p, err := o.find(ctx, in.PairingID)
	if err != nil {
		return trust.Result(err)
	}
	if p == nil || p.IssuerFingerprint != in.Fingerprint {
		return operation.Reject(operation.Conflict, "pairing_operation_conflict")
	}
	receipt, committed, err := o.trust.InspectPairing(ctx, p.ID, p.proof)
	if err != nil {
		return trust.Result(err)
	}
	if committed {
		if !sameActivation(p, receipt) {
			return operation.Reject(operation.Conflict, "pairing_repair_required")
		}
		// Activation wins both recovery choices. Missing delivery credentials
		// remain an explicit repair fact, never permission to replace the token.
		token, readErr := o.credentials.Get(ctx, "FLOE_PAIRING_"+p.ID)
		if readErr != nil {
			return operation.Reject(operation.Unavailable, "pairing_credential_unavailable")
		}
		delivery := "available"
		if token == "" || trust.Digest(token) != receipt.TokenHash {
			delivery = "repair_required"
		}
		next := *p
		next.AdminApproved = true
		next.status = "approved"
		if err = o.save(ctx, &next); err != nil {
			return trust.Result(err)
		}
		o.pending = &next
		return operation.Accept(map[string]any{"schema_version": 1, "pairing_id": p.ID, "status": "approved", "credential_delivery": delivery})
	}
	if p.status == "aborted" && in.Action == "abort" {
		return operation.Accept(map[string]any{"schema_version": 1, "pairing_id": p.ID, "status": "aborted"})
	}
	if p.status != "activating" || p.AdminApproved {
		return operation.Reject(operation.Conflict, "pairing_repair_required")
	}
	if err = o.validateRecoveryReceipt(p); err != nil {
		return trust.Result(err)
	}
	if err = o.trust.WithCurrentOperator(operator, func() error { return ctx.Err() }); err != nil {
		return trust.Result(err)
	}
	if in.Action == "abort" {
		// A pending native write keeps the global credential lane. This save
		// cannot overtake it. No token is deleted on timeout or recovery.
		next := *p
		next.status = "aborted"
		if err = o.save(ctx, &next); err != nil {
			return trust.Result(err)
		}
		o.pending = &next
		return operation.Accept(map[string]any{"schema_version": 1, "pairing_id": p.ID, "status": "aborted"})
	}
	if !p.Expires.After(o.clock()) {
		return operation.Reject(operation.Conflict, "pairing_recovery_expired")
	}
	hash, err := hex.DecodeString(p.activationTokenHash)
	if err != nil || len(hash) != 32 || hex.EncodeToString(hash) != p.activationTokenHash {
		return operation.Reject(operation.Conflict, "pairing_recovery_token_missing")
	}
	token, err := o.credentials.Get(ctx, "FLOE_PAIRING_"+p.ID)
	if err != nil {
		return operation.Reject(operation.Unavailable, "pairing_credential_unavailable")
	}
	if token == "" || trust.Digest(token) != p.activationTokenHash {
		return operation.Reject(operation.Conflict, "pairing_recovery_token_missing")
	}
	key, err := trust.DecodeBase64(p.IssuerPublicKey, ed25519.PublicKeySize)
	if err != nil {
		return trust.Result(err)
	}
	activation := trust.PairingActivation{PairingID: p.ID, PersonID: p.PersonID, DeviceID: p.DeviceID, Producer: p.producer, IssuerKeyID: p.IssuerKeyID, IssuerFingerprint: p.IssuerFingerprint, IssuerPublicKey: key, ChallengeID: p.challengeID, ChallengeBytes: append([]byte(nil), p.challengeBytes...), LocalProof: p.localProof, AdminFingerprint: in.Fingerprint, ExpectedRevision: p.expectedRevision, TokenHash: p.activationTokenHash, PollProofHash: trust.Digest(p.proof), Operator: operator}
	if _, err = o.trust.ActivatePairing(ctx, activation); err != nil {
		return trust.Result(err)
	}
	next := *p
	next.AdminApproved = true
	next.status = "approved"
	if err = o.save(ctx, &next); err != nil {
		return trust.Result(err)
	}
	o.pending = &next
	return operation.Accept(map[string]any{"schema_version": 1, "pairing_id": p.ID, "status": "approved"})
}
func sameActivation(p *Pending, r trust.PairingReadback) bool {
	return r.ClientID == p.ID && r.PersonID == p.PersonID && r.DeviceID == p.DeviceID && r.Producer == p.producer && r.IssuerKeyID == p.IssuerKeyID && r.IssuerPublicKey == p.IssuerPublicKey && r.IssuerFingerprint == p.IssuerFingerprint && r.EnrollmentID == p.challengeID && (p.activationTokenHash == "" || r.TokenHash == p.activationTokenHash)
}
func (o *Operations) validateRecoveryReceipt(p *Pending) error {
	producer, err := o.trust.ProducerMetadata()
	if err != nil {
		return err
	}
	if producer != p.producer || !p.LocalConfirmed || p.expectedRevision == 0 || p.challengeB64 != base64.RawURLEncoding.EncodeToString(p.challengeBytes) {
		return repair()
	}
	signature, err := o.trust.SignProducerChallenge(p.challengeBytes)
	if err != nil {
		return err
	}
	if !bytes.Equal(signature, p.producerSignature) {
		return repair()
	}
	key, err := trust.DecodeBase64(p.IssuerPublicKey, ed25519.PublicKeySize)
	if err != nil || trust.Digest(string(key)) != p.IssuerFingerprint || trust.VerifyProof(p.localProof, p.challengeID, p.IssuerKeyID, p.challengeBytes, key) != nil {
		return repair()
	}
	var challenge struct {
		Version     int    `json:"v"`
		Operation   string `json:"operation"`
		ChallengeID string `json:"challenge_id"`
		Nonce       string `json:"nonce"`
		KeyID       string `json:"key_id"`
		PersonID    string `json:"person_id"`
		ClientID    string `json:"client_id"`
		DeviceID    string `json:"device_id"`
		Audience    string `json:"audience"`
		Purpose     string `json:"purpose"`
		Consumer    string `json:"consumer"`
		Issued      int64  `json:"issued_at_unix_ms"`
		Expires     int64  `json:"expires_at_unix_ms"`
	}
	if trust.DecodeStrict(p.challengeBytes, &challenge, 65536, 16) != nil || challenge.Version != 1 || challenge.Operation != "enrollment" || challenge.ChallengeID != p.challengeID || challenge.KeyID != p.IssuerKeyID || challenge.PersonID != p.PersonID || challenge.ClientID != p.ID || challenge.DeviceID != p.DeviceID || challenge.Audience != producer.Audience || challenge.Purpose != "owner_enrollment" || challenge.Consumer != "owner" || challenge.Expires != p.Expires.UnixMilli() || challenge.Issued <= 0 || challenge.Expires <= challenge.Issued || challenge.Expires-challenge.Issued > 30000 {
		return repair()
	}
	if _, err = trust.DecodeBase64(challenge.Nonce, 32); err != nil {
		return repair()
	}
	return nil
}
