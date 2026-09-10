package gmail

import (
	"crypto/sha256"
	"encoding/hex"
	"time"
)

const readonlyScope = "https://www.googleapis.com/auth/gmail.readonly"

type Capability struct {
	SchemaVersion  int      `json:"schema_version"`
	ID             string   `json:"id"`
	Version        string   `json:"version"`
	Authority      string   `json:"authority"`
	RequiredScopes []string `json:"required_scopes"`
	OutputViewID   string   `json:"output_view_id"`
}

type ViewDescriptor struct {
	SchemaVersion      int    `json:"schema_version"`
	ID                 string `json:"id"`
	Version            string `json:"version"`
	DataClass          string `json:"data_class"`
	Retention          string `json:"retention"`
	FreshnessTTLMS     int64  `json:"freshness_ttl_ms"`
	MaxItems           int    `json:"max_items"`
	MaxBytes           int    `json:"max_bytes"`
	ProvenanceRequired bool   `json:"provenance_required"`
}

type Descriptor struct {
	SchemaVersion int              `json:"schema_version"`
	ID            string           `json:"id"`
	Version       string           `json:"version"`
	Provider      string           `json:"provider"`
	Execution     map[string]any   `json:"execution"`
	Capabilities  []Capability     `json:"capabilities"`
	Views         []ViewDescriptor `json:"views"`
}

type Failure struct {
	Kind             string `json:"kind"`
	ObservedAtUnixMS int64  `json:"observed_at_unix_ms"`
}

type Connection struct {
	SchemaVersion       int      `json:"schema_version"`
	ConnectorID         string   `json:"connector_id"`
	State               string   `json:"state"`
	GrantedScopes       []string `json:"granted_scopes"`
	ObservedAtUnixMS    int64    `json:"observed_at_unix_ms"`
	LastSuccessAtUnixMS *int64   `json:"last_success_at_unix_ms,omitempty"`
	LastFailure         *Failure `json:"last_failure,omitempty"`
}

type ViewSnapshot struct {
	SchemaVersion    int    `json:"schema_version"`
	ViewID           string `json:"view_id"`
	SourceHandle     string `json:"source_handle"`
	ObservedAtUnixMS int64  `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS  int64  `json:"expires_at_unix_ms"`
	ItemCount        int    `json:"item_count"`
	ByteCount        int    `json:"byte_count"`
	ProvenanceCount  int    `json:"provenance_count"`
}

type Snapshot struct {
	Descriptor Descriptor     `json:"descriptor"`
	Connection Connection     `json:"connection"`
	Views      []ViewSnapshot `json:"views"`
}

func ConnectorDescriptor() Descriptor {
	capability := func(id, output string) Capability {
		return Capability{SchemaVersion: 1, ID: id, Version: "1.0.0", Authority: "observe", RequiredScopes: []string{readonlyScope}, OutputViewID: output}
	}
	return Descriptor{
		SchemaVersion: 1, ID: "gmail", Version: "1.0.0", Provider: "gmail",
		Execution: map[string]any{"kind": "server"},
		Capabilities: []Capability{
			capability("mail.search", "mail.communication"),
			capability("mail.threads.read", "mail.communication"),
			capability("mail.messages.read", "mail.body"),
			capability("mail.changes", "mail.communication"),
			capability("mail.logistics.read", "life.logistics"),
		},
		Views: []ViewDescriptor{
			{SchemaVersion: 1, ID: "mail.communication", Version: "1.0.0", DataClass: "personal", Retention: "index_on_demand", FreshnessTTLMS: 300_000, MaxItems: 100, MaxBytes: 65_536, ProvenanceRequired: true},
			{SchemaVersion: 1, ID: "mail.body", Version: "1.0.0", DataClass: "personal", Retention: "ephemeral", FreshnessTTLMS: 60_000, MaxItems: 1, MaxBytes: MaxBodyBytes, ProvenanceRequired: true},
			{SchemaVersion: 1, ID: "life.logistics", Version: "1.0.0", DataClass: "personal", Retention: "short_lived_cache", FreshnessTTLMS: 300_000, MaxItems: maxLogisticsItems, MaxBytes: 65_536, ProvenanceRequired: true},
		},
	}
}

func ConnectionSnapshot(connectionID, state string, now time.Time, lastSuccess *time.Time, failureKind string) (Snapshot, error) {
	if !validID(connectionID) || !validState(state) || (failureKind != "" && !validFailure(failureKind)) {
		return Snapshot{}, ErrInvalidInput
	}
	observed := now.UnixMilli()
	connection := Connection{SchemaVersion: 1, ConnectorID: "gmail", State: state, ObservedAtUnixMS: observed}
	if state == "ready" || state == "degraded" {
		connection.GrantedScopes = []string{readonlyScope}
	}
	if lastSuccess != nil {
		value := lastSuccess.UnixMilli()
		if value > observed {
			return Snapshot{}, ErrInvalidInput
		}
		connection.LastSuccessAtUnixMS = &value
	}
	if failureKind != "" {
		connection.LastFailure = &Failure{Kind: failureKind, ObservedAtUnixMS: observed}
	}
	if (state == "ready" && (connection.LastSuccessAtUnixMS == nil || connection.LastFailure != nil)) ||
		(state == "degraded" && connection.LastFailure == nil) ||
		((state == "unavailable" || state == "revoked" || state == "unsupported") && connection.LastFailure == nil) {
		return Snapshot{}, ErrInvalidInput
	}
	return Snapshot{Descriptor: ConnectorDescriptor(), Connection: connection, Views: []ViewSnapshot{}}, nil
}

func SourceHandle(connectionID, resourceID string) (string, error) {
	if !validID(connectionID) || !validID(resourceID) {
		return "", ErrInvalidInput
	}
	digest := sha256.Sum256([]byte(connectionID + "\x00" + resourceID))
	return "mail:" + hex.EncodeToString(digest[:16]), nil
}

func validState(value string) bool {
	switch value {
	case "pending", "ready", "degraded", "unavailable", "disconnected", "revoked", "unsupported":
		return true
	}
	return false
}

func validFailure(value string) bool {
	switch value {
	case "credential_expired", "permission_denied", "partial_fetch", "rate_limited", "stale", "no_data", "unsupported_entitlement", "unsupported_region", "source_disagreement", "unavailable":
		return true
	}
	return false
}
