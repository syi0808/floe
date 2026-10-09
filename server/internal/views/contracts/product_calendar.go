package contracts

import (
	"context"
	"encoding/json"
	"time"

	sourcecontract "floe/server/internal/contracts/source"
	"floe/server/internal/trust"
)

type CalendarReadLimits struct {
	MaxRecords     uint32 `json:"max_records"`
	MaxBytes       uint32 `json:"max_bytes"`
	MaxPageRecords uint32 `json:"max_page_records"`
	MaxPageBytes   uint32 `json:"max_page_bytes"`
}

type ProductCalendarSourceClaims struct {
	ConnectorID        string `json:"connector_id"`
	ConnectionID       string `json:"connection_id"`
	ExecutionOwner     string `json:"execution_owner"`
	LocalRevision      uint64 `json:"local_revision"`
	ProviderRevision   uint64 `json:"provider_revision"`
	Incarnation        string `json:"incarnation"`
	Epoch              uint64 `json:"epoch"`
	ProviderIdentity   string `json:"provider_identity"`
	IdentityGeneration uint64 `json:"identity_generation"`
}

type ProductCalendarClaims struct {
	PersonID               string                      `json:"person_id"`
	ClientID               string                      `json:"client_id"`
	DeviceID               string                      `json:"device_id"`
	Audience               string                      `json:"audience"`
	ProducerInstance       string                      `json:"producer_instance"`
	ProducerKeyFingerprint string                      `json:"producer_key_fingerprint"`
	EnrollmentID           string                      `json:"enrollment_id"`
	CredentialGeneration   uint64                      `json:"credential_generation"`
	Purpose                string                      `json:"purpose"`
	ResultKind             string                      `json:"result_kind"`
	RefreshOperationID     string                      `json:"refresh_operation_id"`
	ReadOperationID        string                      `json:"read_operation_id"`
	PageID                 string                      `json:"page_id"`
	Source                 ProductCalendarSourceClaims `json:"source"`
	Resources              []string                    `json:"resources"`
	Query                  json.RawMessage             `json:"query"`
	QuerySHA256            string                      `json:"query_sha256"`
	Limits                 CalendarReadLimits          `json:"limits"`
}

type ProductCalendarPreviewRequest struct {
	SchemaVersion int    `json:"schema_version"`
	ConnectorID   string `json:"connector_id"`
	ConnectionID  string `json:"connection_id"`
	LocalRevision uint64 `json:"local_revision"`
}

type ProductCalendarAdmissionRequest struct {
	SchemaVersion   int                   `json:"schema_version"`
	Claims          ProductCalendarClaims `json:"claims"`
	ExpiresAtUnixMS int64                 `json:"expires_at_unix_ms"`
}

type ProductCalendarSourcePreview struct {
	Version                int                         `json:"v"`
	Operation              string                      `json:"operation"`
	ChallengeID            string                      `json:"challenge_id"`
	Nonce                  string                      `json:"nonce"`
	PersonID               string                      `json:"person_id"`
	ClientID               string                      `json:"client_id"`
	DeviceID               string                      `json:"device_id"`
	Audience               string                      `json:"audience"`
	ProducerInstance       string                      `json:"producer_instance"`
	ProducerKeyFingerprint string                      `json:"producer_key_fingerprint"`
	Source                 ProductCalendarSourceClaims `json:"source"`
	Resources              []string                    `json:"resources"`
	IssuedAtUnixMS         int64                       `json:"issued_at_unix_ms"`
	ExpiresAtUnixMS        int64                       `json:"expires_at_unix_ms"`
}

func CalendarSourceClaims(snapshot sourcecontract.Snapshot, localRevision uint64) ProductCalendarSourceClaims {
	return ProductCalendarSourceClaims{snapshot.ConnectorID, snapshot.ConnectionID, snapshot.ExecutionOwner, localRevision, snapshot.ConnectionRevision, snapshot.Incarnation, snapshot.Epoch, snapshot.ProviderIdentity, snapshot.IdentityGeneration}
}

func CloneProductCalendarClaims(claims ProductCalendarClaims) ProductCalendarClaims {
	claims.Resources = append([]string(nil), claims.Resources...)
	claims.Query = append(json.RawMessage(nil), claims.Query...)
	return claims
}

// CalendarMirrorEnforcement is the Views-owned inward port for the product
// Calendar permit. Authority implements it without exposing Engine requests.
type CalendarMirrorEnforcement interface {
	IssueCalendarMirrorAdmission(trust.Principal, sourcecontract.Snapshot, ProductCalendarClaims, time.Time) (string, []byte, time.Time, error)
	CancelCalendarMirrorAdmission(string)
	ClaimCalendarMirrorAdmission(trust.Principal, trust.Proof) (string, sourcecontract.Snapshot, ProductCalendarClaims, time.Time, error)
	StageCalendarMirrorResult(string, trust.Principal, []byte, uint32) (string, []byte, time.Time, error)
	CancelCalendarMirrorRelease(string)
	ClaimCalendarMirrorRelease(context.Context, trust.Principal, trust.Proof) ([]byte, error)
}
