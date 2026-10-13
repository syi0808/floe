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
// challenge receipts. The index atomically retains the complete initial challenge
// while its separate attempt slot is being published, so a retry can recover it.
const maxPairingReceipts = 128

type receiptIndexEntry struct {
	OperationID string `json:"operation_id"`
	PairingID   string `json:"pairing_id"`
}
type receiptIndex struct {
	Version     int                 `json:"version"`
	Entries     []receiptIndexEntry `json:"entries"`
	Reservation *privateReceipt     `json:"reservation"`
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
	if index.Reservation != nil {
		if len(index.Entries) == 0 {
			return receiptIndex{}, repair()
		}
		entry := index.Entries[len(index.Entries)-1]
		pending, err := pendingFromRecord(*index.Reservation, entry)
		if err != nil || pending.status != "pending" || pending.LocalConfirmed || pending.AdminApproved || pending.activationTokenHash != "" || pending.localProof != (trust.Proof{}) {
			return receiptIndex{}, repair()
		}
	}
	return index, nil
}
func (o *Operations) reserve(ctx context.Context, index receiptIndex, p *Pending) error {
	if len(index.Entries) >= maxPairingReceipts {
		return operation.Fail(operation.Limited, "pairing_receipt_capacity")
	}
	if index.Reservation != nil {
		return repair()
	}
	index.Entries = append(index.Entries, receiptIndexEntry{p.operationID, p.ID})
	record := recordFromPending(p)
	if _, err := pendingFromRecord(record, index.Entries[len(index.Entries)-1]); err != nil {
		return err
	}
	index.Reservation = &record
	if err := o.storeIndex(ctx, index); err != nil {
		return err
	}
	_, err := o.publishReservation(ctx, index)
	return err
}
func (o *Operations) storeIndex(ctx context.Context, index receiptIndex) error {
	encoded, err := json.Marshal(index)
	if err != nil || len(encoded) > 32768 {
		return operation.Fail(operation.Limited, "pairing_receipt_capacity")
	}
	if o.credentials.StoreReceiptIndex(ctx, string(encoded)) != nil {
		return operation.Fail(operation.Unavailable, "pairing_credential_unavailable")
	}
	return nil
}

// Only explicit mutation paths publish a reservation. Queries can inspect its
// immutable index payload without writing or creating another challenge.
func (o *Operations) publishReservation(ctx context.Context, index receiptIndex) (receiptIndex, error) {
	if index.Reservation == nil {
		return index, nil
	}
	record := *index.Reservation
	encoded, err := json.Marshal(record)
	if err != nil || len(encoded) > 32768 {
		return receiptIndex{}, repair()
	}
	raw, err := o.credentials.ReadPairingAttempt(ctx, OperationID(record.OperationID))
	if err != nil {
		return receiptIndex{}, operation.Fail(operation.Unavailable, "pairing_credential_unavailable")
	}
	if raw != "" {
		var existing privateReceipt
		if decodeReceipt([]byte(raw), &existing) != nil {
			return receiptIndex{}, repair()
		}
		canonical, err := json.Marshal(existing)
		if err != nil || !bytes.Equal(canonical, encoded) {
			return receiptIndex{}, repair()
		}
	} else if o.credentials.StorePairingAttempt(ctx, OperationID(record.OperationID), string(encoded)) != nil {
		return receiptIndex{}, operation.Fail(operation.Unavailable, "pairing_credential_unavailable")
	}
	index.Reservation = nil
	if err := o.storeIndex(ctx, index); err != nil {
		return receiptIndex{}, err
	}
	return index, nil
}
func recordFromPending(p *Pending) privateReceipt {
	return privateReceipt{Version: 1, OperationID: p.operationID, Pending: *p, Status: p.status, ChallengeID: p.challengeID, ChallengeBytes: p.challengeBytes, ChallengeB64: p.challengeB64, ProducerSignature: p.producerSignature, Proof: p.proof, LocalProof: p.localProof, Producer: p.producer, ExpectedRevision: p.expectedRevision, ActivationTokenHash: p.activationTokenHash}
}
func (o *Operations) save(ctx context.Context, p *Pending) error {
	index, err := o.index(ctx)
	if err != nil {
		return err
	}
	if _, err = o.publishReservation(ctx, index); err != nil {
		return err
	}
	encoded, err := json.Marshal(recordFromPending(p))
	if err != nil || len(encoded) > 32768 {
		return repair()
	}
	if o.credentials.StorePairingAttempt(ctx, OperationID(p.operationID), string(encoded)) != nil {
		return operation.Fail(operation.Unavailable, "pairing_credential_unavailable")
	}
	return nil
}
func (o *Operations) load(ctx context.Context, index receiptIndex, entry receiptIndexEntry) (*Pending, error) {
	if index.Reservation != nil && index.Reservation.OperationID == entry.OperationID {
		return pendingFromRecord(*index.Reservation, entry)
	}
	raw, err := o.credentials.ReadPairingAttempt(ctx, OperationID(entry.OperationID))
	if err != nil {
		return nil, operation.Fail(operation.Unavailable, "pairing_credential_unavailable")
	}
	var record privateReceipt
	if raw == "" || len(raw) > 32768 || decodeReceipt([]byte(raw), &record) != nil {
		return nil, repair()
	}
	return pendingFromRecord(record, entry)
}
func pendingFromRecord(record privateReceipt, entry receiptIndexEntry) (*Pending, error) {
	if record.Version != 1 || record.OperationID != entry.OperationID || record.Pending.ID != entry.PairingID || record.Proof == "" || record.ExpectedRevision == 0 {
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
			return o.load(ctx, index, entry)
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
