package integrations

import (
	"context"
	"floe/server/internal/trust"
)

// AttemptSnapshot, CleanupSnapshot, DisconnectSnapshot, and StateSnapshot are
// Integrations-owned durable values. A repository persists one whole snapshot
// per commit; the service validates and adopts it only after a known commit.
type AttemptSnapshot struct {
	ID, ClientID, PersonID, DeviceID, ConnectorID string
	Record                                        Record
	Status                                        AuthorizationState
	AuthorizationURL, UserCode, ErrorCode         string
	CreatedAt                                     int64
	Revision                                      uint64
	Started                                       bool
	CatalogRevision                               uint64
	RequestedScope                                map[string]any
}

type CleanupSnapshot struct {
	ID          string
	Ticket      *trust.CleanupTicket
	Records     []Record
	RuntimeDone map[string]bool
	VaultDone   map[string]bool
}

type DisconnectSnapshot struct {
	OperationID, ClientID, PersonID, DeviceID, ConnectorID, ConnectionID string
	ConnectionRevision                                                   uint64
	CleanupState                                                         string
}

type StateSnapshot struct {
	SchemaVersion int                             `json:"schema_version"`
	Revision      uint64                          `json:"revision"`
	Connections   map[string]Record               `json:"connections"`
	Attempts      map[string]AttemptSnapshot      `json:"attempts"`
	Cleanup       map[string]CleanupSnapshot      `json:"cleanup"`
	Receipts      map[string]trust.CleanupReceipt `json:"receipts"`
	Disconnects   map[string]DisconnectSnapshot   `json:"disconnects"`
}

type LoadDisposition uint8

const (
	LoadPresent LoadDisposition = iota + 1
	LoadAbsent
	LoadUnavailable
	LoadInvalid
)

type LoadOutcome struct {
	Disposition LoadDisposition
	Snapshot    StateSnapshot
	Cause       error
}

type WriteDisposition uint8

const (
	WriteCommitted WriteDisposition = iota + 1
	WriteRejected
	WriteIndeterminate
	WriteIntegrityFailure
)

type WriteOutcome struct {
	Disposition WriteDisposition
	Cause       error
}

type RepositoryHealth uint8

const (
	RepositoryReady RepositoryHealth = iota + 1
	RepositoryUnavailable
)

// Repository is the Integrations-specific atomic state boundary.
type Repository interface {
	LoadState() LoadOutcome
	SaveState(StateSnapshot) WriteOutcome
	Health() RepositoryHealth
}

// CredentialAccess is limited to a single immutable connection binding. It
// never exposes a generic credential-slot API to the Integrations owner.
type CredentialAccess interface {
	StoreConnectionCredential(context.Context, CredentialBinding, string) error
	DeleteConnectionCredential(context.Context, CredentialBinding) error
}
