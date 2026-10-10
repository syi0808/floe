package pairing

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"floe/server/internal/operation"
	"floe/server/internal/trust"
	"io"
)

// The encrypted credential store retains immutable start identities and private
// challenge receipts. Index reservation precedes receipt generation; an uncertain
// write can therefore never turn a retry into a second enrollment challenge.
const maxPairingReceipts = 128

type receiptIndexEntry struct {
	OperationID string `json:"operation_id"`
	PairingID   string `json:"pairing_id"`
}
type receiptIndex struct {
	Version int                 `json:"version"`
	Entries []receiptIndexEntry `json:"entries"`
}
type privateReceipt struct {
	Version             int                    `json:"version"`
	OperationID         string                 `json:"operation_id"`
	Pending             Pending                `json:"pending"`
	Status              string                 `json:"status"`
	ChallengeID         string                 `json:"challenge_id"`
	ChallengeBytes      []byte                 `json:"challenge_bytes"`
	ChallengeB64        string                 `json:"challenge_b64"`
	ProducerSignature   []byte                 `json:"producer_signature"`
	Proof               string                 `json:"proof"`
	LocalProof          trust.Proof            `json:"local_proof"`
	Producer            trust.ProducerMetadata `json:"producer"`
	ActivationTokenHash string                 `json:"activation_token_hash,omitempty"`
	ExpectedRevision    uint64                 `json:"expected_revision"`
}

func repair() error { return operation.Fail(operation.Conflict, "pairing_repair_required") }
func (o *Operations) index(ctx context.Context) (receiptIndex, error) {
	if o.credentials == nil {
		return receiptIndex{}, repair()
	}
	raw, err := o.credentials.ReadReceiptIndex(ctx)
	if err != nil {
		return receiptIndex{}, operation.Fail(operation.Unavailable, "pairing_credential_unavailable")
	}
	if raw == "" {
		return receiptIndex{Version: 1, Entries: []receiptIndexEntry{}}, nil
	}
	var index receiptIndex
	if len(raw) > 32768 || decodeReceipt([]byte(raw), &index) != nil || index.Version != 1 || len(index.Entries) > maxPairingReceipts {
		return receiptIndex{}, repair()
	}
	seen := map[string]bool{}
	pairs := map[string]bool{}
	for _, entry := range index.Entries {
		if !trust.ValidID(entry.OperationID) || !trust.ValidID(entry.PairingID) || seen[entry.OperationID] || pairs[entry.PairingID] {
			return receiptIndex{}, repair()
		}
		seen[entry.OperationID] = true
		pairs[entry.PairingID] = true
	}
	return index, nil
}
func (o *Operations) reserve(ctx context.Context, index receiptIndex, op, id string) error {
	if len(index.Entries) >= maxPairingReceipts {
		return operation.Fail(operation.Limited, "pairing_receipt_capacity")
	}
	index.Entries = append(index.Entries, receiptIndexEntry{op, id})
	encoded, err := json.Marshal(index)
	if err != nil {
		return repair()
	}
	if o.credentials.StoreReceiptIndex(ctx, string(encoded)) != nil {
		return operation.Fail(operation.Unavailable, "pairing_credential_unavailable")
	}
	return nil
}
func (o *Operations) save(ctx context.Context, p *Pending) error {
	record := privateReceipt{Version: 1, OperationID: p.operationID, Pending: *p, Status: p.status, ChallengeID: p.challengeID, ChallengeBytes: p.challengeBytes, ChallengeB64: p.challengeB64, ProducerSignature: p.producerSignature, Proof: p.proof, LocalProof: p.localProof, Producer: p.producer, ExpectedRevision: p.expectedRevision, ActivationTokenHash: p.activationTokenHash}
	encoded, err := json.Marshal(record)
	if err != nil || len(encoded) > 32768 {
		return repair()
	}
	if o.credentials == nil || o.credentials.StorePairingAttempt(ctx, OperationID(p.operationID), string(encoded)) != nil {
		return operation.Fail(operation.Unavailable, "pairing_credential_unavailable")
	}
	return nil
}
func (o *Operations) load(ctx context.Context, entry receiptIndexEntry) (*Pending, error) {
	raw, err := o.credentials.ReadPairingAttempt(ctx, OperationID(entry.OperationID))
	if err != nil {
		return nil, operation.Fail(operation.Unavailable, "pairing_credential_unavailable")
	}
	var record privateReceipt
	if raw == "" || len(raw) > 32768 || decodeReceipt([]byte(raw), &record) != nil || record.Version != 1 || record.OperationID != entry.OperationID || record.Pending.ID != entry.PairingID || record.Proof == "" || record.ExpectedRevision == 0 {
		return nil, repair()
	}
	p := record.Pending
	p.operationID = record.OperationID
	p.status = record.Status
	p.challengeID = record.ChallengeID
	p.challengeBytes = record.ChallengeBytes
	p.challengeB64 = record.ChallengeB64
	p.producerSignature = record.ProducerSignature
	p.proof = record.Proof
	p.localProof = record.LocalProof
	p.producer = record.Producer
	p.expectedRevision = record.ExpectedRevision
	p.activationTokenHash = record.ActivationTokenHash
	if !trust.ValidID(p.PersonID) || !trust.ValidDevice(p.DeviceID) || !trust.ValidID(p.IssuerKeyID) || !trust.ValidID(p.challengeID) || len(p.challengeBytes) == 0 || len(p.producerSignature) != 64 {
		return nil, repair()
	}
	switch p.status {
	case "pending", "local_confirmed", "activating", "approved", "rejected", "cancelled", "expired", "aborted":
	default:
		return nil, repair()
	}
	return &p, nil
}
func (o *Operations) find(ctx context.Context, id string) (*Pending, error) {
	index, err := o.index(ctx)
	if err != nil {
		return nil, err
	}
	for _, entry := range index.Entries {
		if entry.PairingID == id {
			return o.load(ctx, entry)
		}
	}
	return nil, nil
}
func startResult(p *Pending) StartResult {
	return StartResult{SchemaVersion: 1, PairingID: p.ID, Code: p.Code, Proof: p.proof, ExpiresAtUnixMS: p.Expires.UnixMilli(), PersonID: p.PersonID, DeviceID: p.DeviceID, Producer: p.producer, Issuer: issuer(p), ChallengeID: p.challengeID, Challenge: p.challengeB64, ProducerSignature: base64.RawURLEncoding.EncodeToString(p.producerSignature)}
}

func decodeReceipt(raw []byte, out any) error {
	if err := trust.StrictJSON(raw, 32768, 16); err != nil {
		return err
	}
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(out); err != nil {
		return err
	}
	if decoder.Decode(new(any)) != io.EOF {
		return repair()
	}
	return nil
}
