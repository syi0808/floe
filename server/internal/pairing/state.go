// Package pairing owns bounded temporary enrollment; trust owns the durable commit.
package pairing

import (
	"context"
	"floe/server/internal/trust"
	"time"
)

// PairingID and OperationID identify the two keyed Pairing receipt records.
// CredentialAccess deliberately exposes record operations instead of arbitrary
// credential slots.
type PairingID string
type OperationID string

type CredentialAccess interface {
	ReadPairingToken(context.Context, PairingID) (string, error)
	StorePairingToken(context.Context, PairingID, string) error
	DeletePairingToken(context.Context, PairingID) error
	ReadReceiptIndex(context.Context) (string, error)
	StoreReceiptIndex(context.Context, string) error
	ReadPairingAttempt(context.Context, OperationID) (string, error)
	StorePairingAttempt(context.Context, OperationID, string) error
}

type Trust interface {
	PreparePairing(string) (uint64, error)
	ReadPairing(context.Context, string, string) (trust.PairingReadback, error)
	InspectPairing(context.Context, string, string) (trust.PairingReadback, bool, error)
	ProducerMetadata() (trust.ProducerMetadata, error)
	SignProducerChallenge([]byte) ([]byte, error)
	ActivatePairing(context.Context, trust.PairingActivation) (trust.PairingCommit, error)
	WithCurrentOperator(trust.OperatorPrincipal, func() error) error
}
type Operations struct {
	gate        chan struct{}
	pending     *Pending
	lastPair    time.Time
	clock       func() time.Time
	trust       Trust
	credentials CredentialAccess
}

func NewOperations(service Trust, store CredentialAccess, clock func() time.Time) *Operations {
	if clock == nil {
		clock = time.Now
	}
	return &Operations{gate: make(chan struct{}, 1), trust: service, clock: clock, credentials: store}
}
func (o *Operations) lock(ctx context.Context) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	select {
	case o.gate <- struct{}{}:
		if err := ctx.Err(); err != nil {
			<-o.gate
			return err
		}
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}
func (o *Operations) unlock() { <-o.gate }
func (o *Operations) ClearClient(ctx context.Context, id string) error {
	if err := o.lock(ctx); err != nil {
		return err
	}
	defer o.unlock()
	if o.credentials == nil {
		return repair()
	}
	if err := o.credentials.DeletePairingToken(ctx, PairingID(id)); err != nil {
		return err
	}
	if o.pending != nil && o.pending.ID == id {
		o.pending = nil
	}
	return nil
}

type OperatorPending struct {
	*Pending
	Phase          string   `json:"phase"`
	AllowedActions []string `json:"allowed_actions"`
}

func (o *Operations) Pending(ctx context.Context) (*OperatorPending, error) {
	ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	if err := o.lock(ctx); err != nil {
		return nil, err
	}
	defer o.unlock()
	index, err := o.index(ctx)
	if err != nil {
		return nil, err
	}
	for i := len(index.Entries) - 1; i >= 0; i-- {
		p, err := o.load(ctx, index.Entries[i])
		if err != nil {
			return nil, err
		}
		if p.AdminApproved || p.status == "approved" || p.status == "rejected" || p.status == "cancelled" || p.status == "aborted" {
			continue
		}
		actions := []string{}
		if p.status == "activating" {
			// Recovery is shown even after challenge expiry. Only the explicit
			// action authenticates Trust and determines resume versus abort.
			actions = append(actions, "abort")
			if p.Expires.After(o.clock()) && p.activationTokenHash != "" {
				actions = append(actions, "resume")
			}
		} else {
			if !p.Expires.After(o.clock()) {
				continue
			}
			actions = append(actions, "reject")
			if p.LocalConfirmed {
				actions = append(actions, "approve")
			}
		}
		copy := *p
		copy.challengeBytes = nil
		copy.producerSignature = nil
		copy.localProof = trust.Proof{}
		copy.proof = ""
		copy.token = ""
		return &OperatorPending{&copy, p.status, actions}, nil
	}
	return nil, nil
}

type Pending struct {
	ID                  string    `json:"id"`
	Code                string    `json:"code"`
	Expires             time.Time `json:"expires"`
	PersonID            string    `json:"person_id"`
	DeviceID            string    `json:"device_id"`
	IssuerKeyID         string    `json:"issuer_key_id"`
	IssuerPublicKey     string    `json:"issuer_public_key"`
	IssuerFingerprint   string    `json:"issuer_fingerprint"`
	ProducerFingerprint string    `json:"producer_fingerprint"`
	ProducerAudience    string    `json:"producer_audience"`
	// LocalConfirmed records cryptographic key possession, not a human code comparison.
	LocalConfirmed                                  bool `json:"local_confirmed"`
	AdminApproved                                   bool `json:"admin_approved"`
	status, challengeID, challengeB64, proof, token string
	challengeBytes, producerSignature               []byte
	localProof                                      trust.Proof
	producer                                        trust.ProducerMetadata
	expectedRevision                                uint64
	operationID                                     string
	activationTokenHash                             string
}
type Request struct {
	SchemaVersion   int    `json:"schema_version"`
	OperationID     string `json:"operation_id"`
	PairingID       string `json:"pairing_id"`
	Proof           string `json:"proof"`
	PersonID        string `json:"person_id"`
	DeviceID        string `json:"device_id"`
	IssuerKeyID     string `json:"issuer_key_id"`
	IssuerPublicKey string `json:"issuer_public_key"`
	ChallengeID     string `json:"challenge_id"`
	KeyID           string `json:"key_id"`
	Signature       string `json:"signature"`
}
type ApprovalRequest struct {
	SchemaVersion int    `json:"schema_version"`
	PairingID     string `json:"pairing_id"`
	Fingerprint   string `json:"issuer_fingerprint"`
}
type RejectionRequest struct {
	SchemaVersion int    `json:"schema_version"`
	PairingID     string `json:"pairing_id"`
}

type RecoveryRequest struct {
	SchemaVersion int    `json:"schema_version"`
	PairingID     string `json:"pairing_id"`
	Fingerprint   string `json:"issuer_fingerprint"`
	Action        string `json:"action"`
}
