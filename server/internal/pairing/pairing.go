package pairing

import (
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"regexp"
	"strings"
	"time"

	"floe/server/internal/authorization"
	"floe/server/internal/operation"
)

func (operations *Operations) Execute(action string, input Request) (outcome operation.Result) {

	if action == "start" && operations.allowLegacy && input.SchemaVersion == 0 && input.IssuerKeyID == "" && input.IssuerPublicKey == "" {
		if !validPersonID(input.PersonID) || !validDeviceID(input.DeviceID) {
			outcome = operation.Reject(operation.Invalid, "identity_required")
			return
		}
		operations.mu.Lock()
		now := operations.clock()
		if failure := operations.host.Prepare(input.PersonID); failure.Code != "" {
			operations.mu.Unlock()
			return failure
		}
		if now.Sub(operations.lastPair) < 10*time.Second || (operations.pending != nil && operations.pending.Expires.After(now) && operations.pending.status != "rejected") {
			operations.mu.Unlock()
			outcome = operation.Reject(operation.Limited, "pairing_in_progress")
			return
		}

		operations.lastPair = now
		operations.pending = &Pending{ID: randomToken(), Code: strings.ToUpper(randomToken()[:8]), Expires: now.Add(5 * time.Minute), PersonID: input.PersonID, DeviceID: input.DeviceID, proof: randomToken()}
		pending := *operations.pending
		operations.mu.Unlock()
		outcome = operation.Result{Category: operation.Ready, Value: map[string]any{"id": pending.ID, "code": pending.Code, "proof": pending.proof, "expires": pending.Expires}}
		return
	}
	if action == "start" {
		if input.SchemaVersion != 1 || !validPersonID(input.PersonID) || !validDeviceID(input.DeviceID) || !validConnectionID(input.IssuerKeyID) {
			outcome = operation.Reject(operation.Invalid, "identity_required")
			return
		}
		publicKey, err := base64.RawURLEncoding.DecodeString(input.IssuerPublicKey)
		if err != nil || len(publicKey) != ed25519.PublicKeySize || base64.RawURLEncoding.EncodeToString(publicKey) != input.IssuerPublicKey {
			outcome = operation.Reject(operation.Invalid, "validation")
			return
		}
		metadata, err := operations.host.Metadata()
		if err != nil {
			outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
			return
		}
		audience, ok := metadata["audience"].(string)
		if !ok || audience == "" {
			outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
			return
		}
		operations.mu.Lock()
		now := operations.clock()
		if failure := operations.host.Prepare(input.PersonID); failure.Code != "" {
			operations.mu.Unlock()
			return failure
		}
		if now.Sub(operations.lastPair) < 10*time.Second || (operations.pending != nil && operations.pending.Expires.After(now) && operations.pending.status != "rejected") {
			operations.mu.Unlock()
			outcome = operation.Reject(operation.Limited, "pairing_in_progress")
			return
		}

		operations.lastPair = now
		pairingID, err := newConnectionID()
		if err != nil {
			operations.mu.Unlock()
			outcome = operation.Reject(operation.Unavailable, "pairing_unavailable")
			return
		}
		pollingProof := randomToken()
		operations.mu.Unlock()
		principal := authorization.Principal{ClientID: pairingID, PersonID: input.PersonID, DeviceID: input.DeviceID, Authenticated: true}
		engine := operations.host.Engine()
		if engine == nil {
			outcome = operation.Reject(operation.Unavailable, "authority_unavailable")
			return
		}
		enrollment, challenge, err := engine.BeginEnrollment(principal, input.IssuerKeyID, ed25519.PublicKey(publicKey), audience)
		if err != nil {
			outcome = operation.Reject(operation.Conflict, "pairing_denied")
			return
		}
		producerFingerprint, _ := metadata["fingerprint"].(string)
		pending := &Pending{
			ID: pairingID, Code: strings.ToUpper(randomToken()[:8]), Expires: challenge.ExpiresAt,
			PersonID: input.PersonID, DeviceID: input.DeviceID, IssuerKeyID: enrollment.KeyID,
			IssuerPublicKey:   base64.RawURLEncoding.EncodeToString(publicKey),
			IssuerFingerprint: enrollment.Fingerprint, ProducerFingerprint: producerFingerprint, ProducerAudience: audience,
			enrollmentID: enrollment.ID, challengeID: challenge.ID, challengeBytes: append([]byte(nil), challenge.Bytes...),
			challengeB64: challenge.BytesB64, producerSignature: operations.host.Sign(challenge.Bytes), proof: pollingProof,
		}
		operations.mu.Lock()
		if operations.pending != nil && operations.pending.Expires.After(operations.clock()) {
			operations.mu.Unlock()
			_ = engine.ApproveEnrollment(enrollment.ID, enrollment.Fingerprint, false)
			outcome = operation.Reject(operation.Limited, "pairing_in_progress")
			return
		}
		operations.pending = pending
		operations.mu.Unlock()
		outcome = operation.Result{Category: operation.Ready, Value: map[string]any{
			"schema_version": 1, "pairing_id": pending.ID, "code": pending.Code, "proof": pending.proof,
			"expires_at_unix_ms": pending.Expires.UnixMilli(), "person_id": pending.PersonID, "device_id": pending.DeviceID,
			"producer":     metadata,
			"issuer":       map[string]any{"key_id": pending.IssuerKeyID, "public_key": pending.IssuerPublicKey, "fingerprint": pending.IssuerFingerprint},
			"challenge_id": pending.challengeID, "challenge_b64url": pending.challengeB64,
			"producer_signature": base64.RawURLEncoding.EncodeToString(pending.producerSignature),
		}}
		return
	}
	operations.mu.Lock()
	now := operations.clock()
	if operations.pending == nil || digest(input.Proof) != digest(operations.pending.proof) {
		operations.mu.Unlock()
		outcome = operation.Reject(operation.Unauthenticated, "pairing_expired")
		return
	}
	if operations.pending.IssuerKeyID != "" && (action == "poll" || action == "cancel") && (input.SchemaVersion != 1 || input.PairingID != operations.pending.ID) {
		operations.mu.Unlock()
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	if !operations.pending.Expires.After(now) {
		if action == "poll" {
			pending := *operations.pending
			operations.mu.Unlock()
			outcome = operation.Result{Category: operation.Ready, Value: map[string]any{"schema_version": 1, "pairing_id": pending.ID, "status": "expired", "person_id": pending.PersonID, "device_id": pending.DeviceID}}
			return
		}
		operations.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "pairing_expired")
		return
	}
	if action == "cancel" {
		enrollmentID, fingerprint := operations.pending.enrollmentID, operations.pending.IssuerFingerprint
		operations.pending = nil
		operations.mu.Unlock()
		if engine := operations.host.Engine(); engine != nil {
			_ = engine.ApproveEnrollment(enrollmentID, fingerprint, false)
		}
		outcome = operation.Result{Category: operation.Ready, Value: map[string]bool{"ok": true}}
		return
	}
	if action == "confirm" {
		pending := *operations.pending
		operations.mu.Unlock()
		if input.SchemaVersion != 1 || input.PairingID != pending.ID || input.ChallengeID != pending.challengeID || input.KeyID != pending.IssuerKeyID || input.Signature == "" {
			outcome = operation.Reject(operation.Invalid, "validation")
			return
		}
		engine := operations.host.Engine()
		if engine == nil {
			outcome = operation.Reject(operation.Unavailable, "authority_unavailable")
			return
		}
		principal := authorization.Principal{ClientID: pending.ID, PersonID: pending.PersonID, DeviceID: pending.DeviceID, Authenticated: true}
		if err := engine.CompleteEnrollment(pending.enrollmentID, principal, authorization.Proof{ChallengeID: input.ChallengeID, KeyID: input.KeyID, Signature: input.Signature}); err != nil {
			outcome = operation.Reject(operation.Denied, "pairing_denied")
			return
		}
		operations.mu.Lock()
		if operations.pending == nil || operations.pending.ID != pending.ID {
			operations.mu.Unlock()
			_ = engine.ApproveEnrollment(pending.enrollmentID, pending.IssuerFingerprint, false)
			outcome = operation.Reject(operation.Conflict, "pairing_expired")
			return
		}
		operations.pending.LocalConfirmed = true
		operations.pending.status = "local_confirmed"
		operations.mu.Unlock()
		outcome = operation.Result{Category: operation.Ready, Value: map[string]any{"schema_version": 1, "pairing_id": pending.ID, "status": "local_confirmed"}}
		return
	}
	if action == "poll" {
		pending := *operations.pending
		operations.mu.Unlock()
		status := pending.status
		if status == "" {
			status = "pending"
		}
		if pending.token != "" {
			status = "approved"
		}
		response := map[string]any{"schema_version": 1, "pairing_id": pending.ID, "status": status, "person_id": pending.PersonID, "device_id": pending.DeviceID}
		if pending.token != "" {
			response["issuer"] = map[string]any{"key_id": pending.IssuerKeyID, "public_key": pending.IssuerPublicKey, "fingerprint": pending.IssuerFingerprint}
			response["issuer_fingerprint"] = pending.IssuerFingerprint
			response["client_id"], response["token"] = pending.ID, pending.token
			if producer, err := operations.host.Metadata(); err == nil {
				response["producer"] = producer
			}
		}
		outcome = operation.Result{Category: operation.Ready, Value: response}
		return
	}
	operations.mu.Unlock()
	outcome = operation.Reject(operation.Missing, "not_found")
	return
}
func (operations *Operations) Approve(input ApprovalRequest) (outcome operation.Result) {

	if input.SchemaVersion == 0 && input.PairingID == "" && input.ID != "" && input.Fingerprint == "" {
		outcome = operations.approveLegacy(input.ID)
		return
	}
	if input.SchemaVersion != 1 || input.PairingID == "" || input.ID != "" {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	if input.Fingerprint == "" {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}

	operations.mu.Lock()
	pending := operations.pending
	if pending == nil || pending.ID != input.PairingID {
		operations.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "pairing_expired")
		return
	}
	if !pending.Expires.After(operations.clock()) {
		operations.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "pairing_expired")
		return
	}
	if !pending.LocalConfirmed {
		operations.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "pairing_not_confirmed")
		return
	}
	if pending.IssuerFingerprint != input.Fingerprint {
		operations.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "fingerprint_mismatch")
		return
	}
	pendingCopy := *pending
	token := randomToken()
	operations.mu.Unlock()

	engine := operations.host.Engine()
	if engine == nil {
		outcome = operation.Reject(operation.Unavailable, "authority_unavailable")
		return
	}
	err := engine.ApproveEnrollmentWithCommit(
		pendingCopy.enrollmentID,
		pendingCopy.IssuerFingerprint,
		func(record authorization.IssuerRecord) error {
			return operations.commit(record, digest(token), pendingCopy)
		},
	)
	if err != nil {
		outcome = operation.Reject(operation.Conflict, "pairing_conflict")
		return
	}
	operations.mu.Lock()
	if operations.pending == nil || operations.pending.ID != pendingCopy.ID {
		operations.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "pairing_expired")
		return
	}
	operations.pending.AdminApproved = true
	operations.pending.status = "approved"
	operations.pending.token = token
	operations.mu.Unlock()
	outcome = operation.Result{Category: operation.Ready, Value: map[string]any{"schema_version": 1, "pairing_id": pendingCopy.ID, "status": "approved"}}
	return
}
func (operations *Operations) Reject(input RejectionRequest) (outcome operation.Result) {
	if input.PairingID != "" && input.ID != "" && input.PairingID != input.ID {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	if input.PairingID == "" {
		input.PairingID = input.ID
	}
	if input.PairingID == "" || input.SchemaVersion != 1 && input.ID == "" {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}

	operations.mu.Lock()
	if operations.pending == nil || operations.pending.ID != input.PairingID {
		operations.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "pairing_expired")
		return
	}
	pending := *operations.pending
	operations.pending.status = "rejected"
	operations.pending.LocalConfirmed = false
	operations.pending.AdminApproved = false
	operations.pending.token = ""
	operations.mu.Unlock()

	if engine := operations.host.Engine(); engine != nil {
		_ = engine.ApproveEnrollment(pending.enrollmentID, pending.IssuerFingerprint, false)
	}
	outcome = operation.Result{Category: operation.Ready, Value: map[string]any{"schema_version": 1, "pairing_id": pending.ID, "status": "rejected"}}
	return
}
func (operations *Operations) approveLegacy(pairingID string) (outcome operation.Result) {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	if operations.pending == nil || operations.pending.ID != pairingID || !operations.pending.Expires.After(operations.clock()) || operations.pending.token != "" {
		outcome = operation.Reject(operation.Conflict, "pairing_expired")
		return
	}
	if operations.pending.IssuerKeyID != "" {
		outcome = operation.Reject(operation.Conflict, "pairing_confirmation_required")
		return
	}
	token := randomToken()
	if err := operations.host.CommitLegacy(*operations.pending, digest(token)); err != nil {
		return operation.Reject(operation.Internal, "save_failed")
	}
	operations.pending.token = token
	outcome = operation.Result{Category: operation.Ready, Value: map[string]bool{"ok": true}}
	return
}
func (operations *Operations) commit(record authorization.IssuerRecord, tokenHash string, pending Pending) error {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	if operations.pending == nil || operations.pending.ID != pending.ID || !operations.pending.LocalConfirmed || operations.pending.IssuerKeyID != record.KeyID || operations.pending.IssuerFingerprint != issuerFingerprint(record.PublicKey) || record.EnrollmentID != pending.enrollmentID || operations.host.Fingerprint() != pending.ProducerFingerprint {
		return authorization.ErrConflict
	}
	return operations.host.Commit(record, tokenHash, pending)
}
func randomToken() string {
	return rand.Text() + rand.Text()
}

func digest(value string) string {
	hash := sha256.Sum256([]byte(value))
	return hex.EncodeToString(hash[:])
}

func newConnectionID() (string, error) {
	bytes := make([]byte, 16)
	if _, err := rand.Read(bytes); err != nil {
		return "", err
	}
	bytes[6] = bytes[6]&0x0f | 0x40
	bytes[8] = bytes[8]&0x3f | 0x80
	hexadecimal := hex.EncodeToString(bytes)
	return hexadecimal[:8] + "-" + hexadecimal[8:12] + "-" + hexadecimal[12:16] + "-" + hexadecimal[16:20] + "-" + hexadecimal[20:], nil
}

var personIDPattern = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$`)
var deviceIDPattern = regexp.MustCompile(`^[A-Za-z0-9._:-]{1,128}$`)
var connectionIDPattern = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-4[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$`)

func validPersonID(value string) bool     { return personIDPattern.MatchString(value) }
func validDeviceID(value string) bool     { return deviceIDPattern.MatchString(value) }
func validConnectionID(value string) bool { return connectionIDPattern.MatchString(value) }
func issuerFingerprint(publicKey ed25519.PublicKey) string {
	hash := sha256.Sum256(publicKey)
	return hex.EncodeToString(hash[:])
}
