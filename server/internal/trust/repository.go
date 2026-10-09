package trust

// ClientRecord and IssuerRecord are Trust-owned durable values. Their JSON
// shape is implemented by the storage adapter; their meaning and validation
// remain owned by Trust.
type ClientRecord struct {
	ClientID      string `json:"client_id"`
	PersonID      string `json:"person_id"`
	DeviceID      string `json:"device_id"`
	TokenHash     string `json:"token_hash"`
	PollProofHash string `json:"poll_proof_hash"`
}

type IssuerRecord struct {
	KeyID        string `json:"key_id"`
	ClientID     string `json:"client_id"`
	EnrollmentID string `json:"enrollment_id"`
	PublicKey    []byte `json:"public_key"`
	Generation   uint64 `json:"generation"`
}

// StateSnapshot is the complete Trust persistence unit. A repository commit
// must replace it atomically; Trust validates and adopts it only after commit.
type StateSnapshot struct {
	SchemaVersion  int                      `json:"schema_version"`
	Revision       uint64                   `json:"revision"`
	InstanceID     string                   `json:"instance_id"`
	ExecutionOwner string                   `json:"execution_owner"`
	Clients        map[string]ClientRecord  `json:"clients"`
	Issuers        map[string]IssuerRecord  `json:"issuers"`
	Revoked        map[string]bool          `json:"revoked_issuers"`
	Cleanup        map[string]CleanupTicket `json:"cleanup"`
}

type ReadDisposition uint8

const (
	ReadPresent ReadDisposition = iota + 1
	ReadAbsent
	ReadUnavailable
	ReadInvalid
)

type StateReadOutcome struct {
	Disposition ReadDisposition
	State       StateSnapshot
	Cause       error
}

type ProducerReadOutcome struct {
	Disposition ReadDisposition
	Identity    *ProducerIdentity
	Cause       error
}

type AdministratorCredentialReadOutcome struct {
	Disposition ReadDisposition
	Fingerprint string
	Cause       error
}

type RepositorySnapshot struct {
	State                   StateReadOutcome
	Producer                ProducerReadOutcome
	AdministratorCredential AdministratorCredentialReadOutcome
}

type WriteDisposition uint8

const (
	WriteCommitted WriteDisposition = iota + 1
	WriteRejected
	WriteIndeterminate
	WriteIntegrityFailure
)

// WriteOutcome distinguishes a known pre-commit failure from an ambiguous
// durable replacement and an integrity failure. The owner controls whether
// authority remains available after each outcome.
type WriteOutcome struct {
	Disposition WriteDisposition
	Cause       error
}

type RepositoryHealth uint8

const (
	RepositoryReady RepositoryHealth = iota + 1
	RepositoryUnavailable
)

// Repository is the Trust-specific persistence port. Implementations own
// file names, strict JSON encoding and encrypted atomic replacement.
type Repository interface {
	Load() RepositorySnapshot
	SaveState(StateSnapshot) WriteOutcome
	SaveProducerIdentity(*ProducerIdentity) WriteOutcome
	SaveAdministratorCredential(string) WriteOutcome
	Health() RepositoryHealth
}
