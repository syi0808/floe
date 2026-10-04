package pairing

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"strings"
	"time"

	"floe/server/internal/operation"
	"floe/server/internal/trust"
)

func (o *Operations) Execute(ctx context.Context, action string, in Request) operation.Result {
	ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	if err := ctx.Err(); err != nil {
		return trust.Result(err)
	}
	if in.SchemaVersion != 1 {
		return operation.Reject(operation.Invalid, "validation")
	}
	if action == "start" {
		return o.start(ctx, in)
	}
	if err := o.lock(ctx); err != nil {
		return trust.Result(err)
	}
	defer o.unlock()
	p, loadErr := o.find(ctx, in.PairingID)
	if loadErr != nil {
		return trust.Result(loadErr)
	}
	if p == nil || in.PairingID != p.ID || subtle.ConstantTimeCompare([]byte(trust.Digest(in.Proof)), []byte(trust.Digest(p.proof))) != 1 {
		return operation.Reject(operation.Unauthenticated, "pairing_expired")
	}
	if action == "poll" {
		_, committed, err := o.trust.InspectPairing(ctx, p.ID, in.Proof)
		if err != nil {
			return trust.Result(err)
		}
		if committed {
			return o.readCommitted(ctx, p.ID, in.Proof)
		}
		if p.AdminApproved || p.status == "activating" {
			return operation.Reject(operation.Conflict, "pairing_repair_required")
		}
	}
	if p.status == "activating" || p.AdminApproved {
		return operation.Reject(operation.Conflict, "pairing_already_committed")
	}
	if action == "cancel" && (p.status == "cancelled" || p.status == "aborted") {
		return operation.Accept(map[string]bool{"ok": true})
	}
	if !p.Expires.After(o.clock()) && p.status != "aborted" && p.status != "cancelled" && p.status != "rejected" {
		copy := *p
		p = &copy
		p.status = "expired"
		if action == "poll" {
			return operation.Accept(status(p))
		}
		return operation.Reject(operation.Conflict, "pairing_expired")
	}
	switch action {
	case "cancel":
		if p.status == "rejected" {
			return operation.Reject(operation.Conflict, "pairing_denied")
		}
		copy := *p
		copy.status = "cancelled"
		if err := o.save(ctx, &copy); err != nil {
			return trust.Result(err)
		}
		o.pending = &copy
		return operation.Accept(map[string]bool{"ok": true})
	case "confirm":
		if p.status == "rejected" || p.status == "cancelled" || p.status == "aborted" || in.ChallengeID != p.challengeID || in.KeyID != p.IssuerKeyID {
			return operation.Reject(operation.Conflict, "pairing_denied")
		}
		publicKey, err := trust.DecodeBase64(p.IssuerPublicKey, ed25519.PublicKeySize)
		proof := trust.Proof{ChallengeID: in.ChallengeID, KeyID: in.KeyID, Signature: in.Signature}
		if err != nil || trust.VerifyProof(proof, p.challengeID, p.IssuerKeyID, p.challengeBytes, publicKey) != nil {
			return operation.Reject(operation.Denied, "pairing_denied")
		}
		if p.LocalConfirmed {
			if p.localProof != proof {
				return operation.Reject(operation.Conflict, "pairing_denied")
			}
		} else {
			copy := *p
			copy.LocalConfirmed = true
			copy.localProof = proof
			copy.status = "local_confirmed"
			if err := o.save(ctx, &copy); err != nil {
				return trust.Result(err)
			}
			p = &copy
			o.pending = p
		}
		return operation.Accept(map[string]any{"schema_version": 1, "pairing_id": p.ID, "status": p.status})
	case "poll":
		return operation.Accept(status(p))
	default:
		return operation.Reject(operation.Missing, "not_found")
	}
}
func (o *Operations) start(ctx context.Context, in Request) operation.Result {
	if !trust.ValidID(in.OperationID) || !trust.ValidID(in.PersonID) || !trust.ValidDevice(in.DeviceID) || !trust.ValidID(in.IssuerKeyID) {
		return operation.Reject(operation.Invalid, "identity_required")
	}
	if _, err := trust.DecodeBase64(in.Proof, 32); err != nil {
		return operation.Reject(operation.Invalid, "validation")
	}
	publicKey, err := trust.DecodeBase64(in.IssuerPublicKey, ed25519.PublicKeySize)
	if err != nil {
		return operation.Reject(operation.Invalid, "validation")
	}
	if err := o.lock(ctx); err != nil {
		return trust.Result(err)
	}
	defer o.unlock()
	index, err := o.index(ctx)
	if err != nil {
		return trust.Result(err)
	}
	for _, entry := range index.Entries {
		if entry.OperationID == in.OperationID {
			p, loadErr := o.load(ctx, entry)
			if loadErr != nil {
				return trust.Result(loadErr)
			}
			if p.PersonID != in.PersonID || p.DeviceID != in.DeviceID || p.IssuerKeyID != in.IssuerKeyID || p.IssuerPublicKey != in.IssuerPublicKey || subtle.ConstantTimeCompare([]byte(trust.Digest(p.proof)), []byte(trust.Digest(in.Proof))) != 1 {
				return operation.Reject(operation.Conflict, "pairing_operation_conflict")
			}
			if p.Expires.After(o.clock()) && p.status != "cancelled" && p.status != "rejected" && p.status != "aborted" {
				o.pending = p
			}
			return startResult(p)
		}
	}
	revision, err := o.trust.PreparePairing(in.PersonID)
	if err != nil {
		return trust.Result(err)
	}
	producer, err := o.trust.ProducerMetadata()
	if err != nil {
		return trust.Result(err)
	}
	now := o.clock()
	if now.Sub(o.lastPair) < 10*time.Second {
		return operation.Reject(operation.Limited, "pairing_in_progress")
	}
	for _, entry := range index.Entries {
		p, loadErr := o.load(ctx, entry)
		if loadErr != nil {
			return trust.Result(loadErr)
		}
		if p.Expires.After(now) && p.status != "rejected" && p.status != "cancelled" && p.status != "aborted" && p.status != "approved" {
			return operation.Reject(operation.Limited, "pairing_in_progress")
		}
	}
	id, challengeID := trust.NewID(), trust.NewID()
	if err := o.reserve(ctx, index, in.OperationID, id); err != nil {
		return trust.Result(err)
	}
	nonce := make([]byte, 32)
	if _, err = rand.Read(nonce); err != nil {
		return operation.Reject(operation.Unavailable, "pairing_unavailable")
	}
	expires := now.Add(trust.EnrollmentLifetime)
	challenge := struct {
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
	}{1, "enrollment", challengeID, base64.RawURLEncoding.EncodeToString(nonce), in.IssuerKeyID, in.PersonID, id, in.DeviceID, producer.Audience, "owner_enrollment", "owner", now.UnixMilli(), expires.UnixMilli()}
	encoded, err := json.Marshal(challenge)
	if err != nil {
		return operation.Reject(operation.Internal, "pairing_unavailable")
	}
	signature, err := o.trust.SignProducerChallenge(encoded)
	if err != nil {
		return trust.Result(err)
	}
	p := &Pending{operationID: in.OperationID, ID: id, Code: strings.ToUpper(trust.Token()[:8]), Expires: expires, PersonID: in.PersonID, DeviceID: in.DeviceID, IssuerKeyID: in.IssuerKeyID, IssuerPublicKey: in.IssuerPublicKey, IssuerFingerprint: trust.Digest(string(publicKey)), ProducerFingerprint: producer.Fingerprint, ProducerAudience: producer.Audience, challengeID: challengeID, challengeBytes: encoded, challengeB64: base64.RawURLEncoding.EncodeToString(encoded), producerSignature: signature, proof: in.Proof, producer: producer, expectedRevision: revision, status: "pending"}
	if err := o.save(ctx, p); err != nil {
		return trust.Result(err)
	}
	o.pending = p
	o.lastPair = now
	return startResult(p)
}
func issuer(p *Pending) map[string]string {
	return map[string]string{"key_id": p.IssuerKeyID, "public_key": p.IssuerPublicKey, "fingerprint": p.IssuerFingerprint}
}
func status(p *Pending) map[string]any {
	phase := p.status
	if phase == "aborted" {
		phase = "cancelled"
	}
	out := map[string]any{"schema_version": 1, "pairing_id": p.ID, "status": phase, "person_id": p.PersonID, "device_id": p.DeviceID}
	if p.token != "" && p.AdminApproved {
		out["issuer"] = issuer(p)
		out["issuer_fingerprint"] = p.IssuerFingerprint
		out["client_id"] = p.ID
		out["token"] = p.token
		out["producer"] = p.producer
	}
	return out
}
func (o *Operations) Approve(ctx context.Context, operator trust.OperatorPrincipal, in ApprovalRequest) operation.Result {
	ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	if in.SchemaVersion != 1 || !trust.ValidID(in.PairingID) || len(in.Fingerprint) != 64 {
		return operation.Reject(operation.Invalid, "validation")
	}
	// Session fencing is short; the durable activation holds only pairing+trust transition locks.
	if err := o.trust.WithCurrentOperator(operator, func() error { return nil }); err != nil {
		return trust.Result(err)
	}
	if err := o.lock(ctx); err != nil {
		return trust.Result(err)
	}
	defer o.unlock()
	p, loadErr := o.find(ctx, in.PairingID)
	if loadErr != nil {
		return trust.Result(loadErr)
	}
	if p == nil || p.ID != in.PairingID || !p.Expires.After(o.clock()) || p.AdminApproved || p.status != "local_confirmed" {
		return operation.Reject(operation.Conflict, "pairing_expired")
	}
	if !p.LocalConfirmed || p.IssuerFingerprint != in.Fingerprint {
		return operation.Reject(operation.Conflict, "pairing_not_confirmed")
	}
	key, err := trust.DecodeBase64(p.IssuerPublicKey, ed25519.PublicKeySize)
	if err != nil {
		return operation.Reject(operation.Invalid, "validation")
	}
	token := trust.Token()
	activation := trust.PairingActivation{PairingID: p.ID, PersonID: p.PersonID, DeviceID: p.DeviceID, Producer: p.producer, IssuerKeyID: p.IssuerKeyID, IssuerFingerprint: p.IssuerFingerprint, IssuerPublicKey: key, ChallengeID: p.challengeID, ChallengeBytes: append([]byte(nil), p.challengeBytes...), LocalProof: p.localProof, AdminFingerprint: in.Fingerprint, ExpectedRevision: p.expectedRevision, TokenHash: trust.Digest(token), PollProofHash: trust.Digest(p.proof), Operator: operator}
	staged := *p
	staged.status = "activating"
	staged.activationTokenHash = trust.Digest(token)
	if err := o.save(ctx, &staged); err != nil {
		return trust.Result(err)
	}
	o.pending = &staged
	p = &staged
	if o.credentials == nil || o.credentials.Put(ctx, "FLOE_PAIRING_"+p.ID, token) != nil {
		return operation.Reject(operation.Unavailable, "pairing_credential_unavailable")
	}
	readback, readErr := o.credentials.Get(ctx, "FLOE_PAIRING_"+p.ID)
	if readErr != nil || trust.Digest(readback) != p.activationTokenHash {
		return operation.Reject(operation.Unavailable, "pairing_credential_unavailable")
	}
	if _, err = o.trust.ActivatePairing(ctx, activation); err != nil {
		return trust.Result(err)
	}
	p.AdminApproved = true
	p.status = "approved"
	p.token = token
	if err := o.save(ctx, p); err != nil {
		return trust.Result(err)
	}
	return operation.Accept(map[string]any{"schema_version": 1, "pairing_id": p.ID, "status": "approved"})
}
func (o *Operations) Reject(ctx context.Context, operator trust.OperatorPrincipal, in RejectionRequest) operation.Result {
	ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	if in.SchemaVersion != 1 || !trust.ValidID(in.PairingID) {
		return operation.Reject(operation.Invalid, "validation")
	}
	if err := o.trust.WithCurrentOperator(operator, func() error { return nil }); err != nil {
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
	if p == nil || p.AdminApproved {
		return operation.Reject(operation.Conflict, "pairing_expired")
	}
	if p.status == "rejected" {
		return operation.Accept(map[string]any{"schema_version": 1, "pairing_id": in.PairingID, "status": "rejected"})
	}
	if p.status != "pending" && p.status != "local_confirmed" {
		return operation.Reject(operation.Conflict, "pairing_denied")
	}
	p.status = "rejected"
	p.LocalConfirmed = false
	p.token = ""
	if err := o.save(ctx, p); err != nil {
		return trust.Result(err)
	}
	o.pending = p
	return operation.Accept(map[string]any{"schema_version": 1, "pairing_id": in.PairingID, "status": "rejected"})
}

func (o *Operations) readCommitted(ctx context.Context, id, proof string) operation.Result {
	if !trust.ValidID(id) || o.credentials == nil {
		return operation.Reject(operation.Conflict, "pairing_repair_required")
	}
	receipt, err := o.trust.ReadPairing(ctx, id, proof)
	if err != nil {
		return trust.Result(err)
	}
	token, err := o.credentials.Get(ctx, "FLOE_PAIRING_"+id)
	if err != nil || trust.Digest(token) != receipt.TokenHash {
		return operation.Reject(operation.Conflict, "pairing_repair_required")
	}
	return operation.Accept(map[string]any{"schema_version": 1, "pairing_id": id, "status": "approved", "client_id": id, "person_id": receipt.PersonID, "device_id": receipt.DeviceID, "token": token, "producer": receipt.Producer, "issuer_fingerprint": receipt.IssuerFingerprint, "issuer": map[string]string{"key_id": receipt.IssuerKeyID, "public_key": receipt.IssuerPublicKey, "fingerprint": receipt.IssuerFingerprint}})
}
