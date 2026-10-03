package authority

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"floe/server/internal/trust"
	"floe/server/internal/views"
	"reflect"
	"time"
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
type ProductCalendarChallenge struct {
	Operation       string                `json:"operation"`
	Version         int                   `json:"v"`
	ChallengeID     string                `json:"challenge_id"`
	Nonce           string                `json:"nonce"`
	KeyID           string                `json:"key_id"`
	Claims          ProductCalendarClaims `json:"claims"`
	AdmissionID     string                `json:"admission_id,omitempty"`
	ResultSHA256    string                `json:"result_sha256,omitempty"`
	IssuedAtUnixMS  int64                 `json:"issued_at_unix_ms"`
	ExpiresAtUnixMS int64                 `json:"expires_at_unix_ms"`
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
type productCalendarPolicy struct {
	claims  ProductCalendarClaims
	expires time.Time
}

func (p productCalendarPolicy) requestBounds() views.Bounds {
	return views.Bounds{MaxItems: p.claims.Limits.MaxPageRecords, MaxBytes: p.claims.Limits.MaxPageBytes}
}
func (p productCalendarPolicy) clonePolicy() requestPolicy {
	p.claims.Resources = append([]string(nil), p.claims.Resources...)
	p.claims.Query = append(json.RawMessage(nil), p.claims.Query...)
	return p
}
func (p productCalendarPolicy) validateSource(source views.SourceSnapshot) error {
	c := p.claims
	l := c.Limits
	if c.Purpose != "day_refresh" || c.ResultKind != "calendar.mirror" || !trust.ValidID(c.RefreshOperationID) || !trust.ValidID(c.ReadOperationID) || !trust.ValidID(c.PageID) || !trust.ValidID(c.EnrollmentID) || c.CredentialGeneration == 0 || c.Source.LocalRevision == 0 || !reflect.DeepEqual(c.Source, sourceClaims(source, c.Source.LocalRevision)) || c.PersonID != source.PersonID || !reflect.DeepEqual(c.Resources, source.Resources) || len(c.Resources) == 0 || len(c.Resources) > 256 || l.MaxRecords == 0 || l.MaxRecords > 10000 || l.MaxBytes == 0 || l.MaxBytes > 4<<20 || l.MaxPageRecords == 0 || l.MaxPageRecords > 128 || l.MaxPageRecords > l.MaxRecords || l.MaxPageBytes == 0 || l.MaxPageBytes > 1<<20 || l.MaxPageBytes > l.MaxBytes {
		return ErrInvalid
	}
	query, err := views.ParseMirrorQuery(c.Query)
	digest := sha256.Sum256(c.Query)
	if err != nil || hex.EncodeToString(digest[:]) != c.QuerySHA256 || query.Limit > l.MaxPageRecords {
		return ErrInvalid
	}
	found := false
	previous := ""
	for _, resource := range c.Resources {
		if validateBoundString(resource, 256) != nil || previous != "" && previous >= resource {
			return ErrInvalid
		}
		previous = resource
		found = found || resource == query.CalendarID
	}
	if !found {
		return ErrInvalid
	}
	return nil
}
func sourceClaims(source views.SourceSnapshot, local uint64) ProductCalendarSourceClaims {
	return ProductCalendarSourceClaims{source.ConnectorID, source.ConnectionID, source.ExecutionOwner, local, source.ConnectionRevision, source.Incarnation, source.Epoch, source.ProviderIdentity, source.IdentityGeneration}
}
func productClaimsBinding(c ProductCalendarClaims) [32]byte {
	c.PageID = ""
	c.Query = nil
	c.QuerySHA256 = ""
	raw, _ := json.Marshal(c)
	return sha256.Sum256(raw)
}

type CalendarProductPage struct {
	SchemaVersion      int                         `json:"schema_version"`
	ResultKind         string                      `json:"result_kind"`
	RefreshOperationID string                      `json:"refresh_operation_id"`
	ReadOperationID    string                      `json:"read_operation_id"`
	PageID             string                      `json:"page_id"`
	PersonID           string                      `json:"person_id"`
	DeviceID           string                      `json:"device_id"`
	Source             ProductCalendarSourceClaims `json:"source"`
	CalendarID         string                      `json:"calendar_id"`
	RangeStartUnixMS   int64                       `json:"range_start_unix_ms"`
	RangeEndUnixMS     int64                       `json:"range_end_unix_ms"`
	ObservedAtUnixMS   int64                       `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS    int64                       `json:"expires_at_unix_ms"`
	Outcome            calendarPageOutcome         `json:"outcome"`
}

// MarshalJSON enforces the exact closed Complete/More/Failed wire variants.
type calendarPageOutcome struct {
	state          string
	records        []views.CalendarRecord
	cursor, reason string
}

func (o calendarPageOutcome) MarshalJSON() ([]byte, error) {
	switch o.state {
	case "complete":
		if o.records == nil || o.cursor != "" || o.reason != "" {
			return nil, ErrInvalid
		}
		return json.Marshal(struct {
			State   string                 `json:"state"`
			Records []views.CalendarRecord `json:"records"`
		}{o.state, o.records})
	case "more":
		if o.records == nil || o.cursor == "" || o.reason != "" {
			return nil, ErrInvalid
		}
		return json.Marshal(struct {
			State   string                 `json:"state"`
			Records []views.CalendarRecord `json:"records"`
			Cursor  string                 `json:"cursor"`
		}{o.state, o.records, o.cursor})
	case "failed":
		if o.records != nil || o.cursor != "" || !calendarFailure(o.reason) {
			return nil, ErrInvalid
		}
		return json.Marshal(struct {
			State  string `json:"state"`
			Reason string `json:"reason"`
		}{o.state, o.reason})
	default:
		return nil, ErrInvalid
	}
}
func calendarFailure(reason string) bool {
	switch reason {
	case "permission_denied", "calendar_unavailable", "provider_unavailable", "source_changed", "source_fenced", "vault_locked", "budget_exceeded", "deadline_exceeded", "cancelled":
		return true
	}
	return false
}
