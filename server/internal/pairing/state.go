package pairing

import (
	"sync"
	"time"

	"floe/server/internal/authorization"
	"floe/server/internal/operation"
)

type Host struct {
	Prepare      func(string) operation.Result
	Commit       func(authorization.IssuerRecord, string, Pending) error
	CommitLegacy func(Pending, string) error
	Engine       func() *authorization.Engine
	Metadata     func() (map[string]any, error)
	Sign         func([]byte) []byte
	Fingerprint  func() string
}

type Operations struct {
	mu          sync.Mutex
	pending     *Pending
	lastPair    time.Time
	allowLegacy bool
	clock       func() time.Time
	host        Host
}

func NewOperations(host Host, allowLegacy bool, clock func() time.Time) *Operations {
	if clock == nil {
		clock = time.Now
	}
	return &Operations{host: host, allowLegacy: allowLegacy, clock: clock}
}

func (operations *Operations) ClearClient(clientID string) {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	if operations.pending != nil && operations.pending.ID == clientID {
		operations.pending = nil
	}
}

func (operations *Operations) Pending() *Pending {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	if operations.pending == nil || operations.pending.token != "" || operations.pending.status == "rejected" || !operations.pending.Expires.After(operations.clock()) {
		return nil
	}
	pending := *operations.pending
	return &pending
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
	LocalConfirmed      bool      `json:"local_confirmed"`
	AdminApproved       bool      `json:"admin_approved"`
	status              string
	enrollmentID        string
	challengeID         string
	challengeBytes      []byte
	challengeB64        string
	producerSignature   []byte
	proof               string
	token               string
}

type Request struct {
	SchemaVersion   int    `json:"schema_version"`
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
	ID            string `json:"id"`
	Fingerprint   string `json:"issuer_fingerprint"`
}
type RejectionRequest struct {
	SchemaVersion int    `json:"schema_version"`
	PairingID     string `json:"pairing_id"`
	ID            string `json:"id"`
}
