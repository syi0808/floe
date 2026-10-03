package gmail

import (
	"crypto/sha256"
	"encoding/hex"
	"floe/server/internal/integrations"
	"floe/server/internal/views"
	"time"
)

const readonlyScope = "https://www.googleapis.com/auth/gmail.readonly"

func ConnectorDescriptor() integrations.Descriptor {
	capability := func(id, output string) integrations.Capability {
		return integrations.Capability{SchemaVersion: 1, ID: id, Version: "1.0.0", Authority: "observe", RequiredScopes: []string{readonlyScope}, OutputViewID: output}
	}
	return integrations.Descriptor{
		SchemaVersion: 1, ID: "gmail", Version: "1.0.0", Provider: "gmail",
		Execution: map[string]any{"kind": "server"},
		Capabilities: []integrations.Capability{
			capability("mail.search", "mail.communication"),
			capability("mail.threads.read", "mail.communication"),
			capability("mail.messages.read", "mail.body"),
			capability("mail.changes", "mail.communication"),
			capability("mail.logistics.read", "life.logistics"),
		},
		Views: []views.ViewDescriptor{
			{SchemaVersion: 1, ID: "mail.communication", Version: "1.0.0", DataClass: "personal", Retention: "index_on_demand", FreshnessTTLMS: 300_000, MaxItems: 100, MaxBytes: 65_536, ProvenanceRequired: true},
			{SchemaVersion: 1, ID: "mail.body", Version: "1.0.0", DataClass: "personal", Retention: "ephemeral", FreshnessTTLMS: 60_000, MaxItems: 1, MaxBytes: MaxBodyBytes, ProvenanceRequired: true},
			{SchemaVersion: 1, ID: "life.logistics", Version: "1.0.0", DataClass: "personal", Retention: "short_lived_cache", FreshnessTTLMS: 300_000, MaxItems: maxLogisticsItems, MaxBytes: 65_536, ProvenanceRequired: true},
		},
	}
}

func ConnectionSnapshot(connectionID, state string, now time.Time, lastSuccess *time.Time, failureKind string) (integrations.Snapshot, error) {
	if !validID(connectionID) || !validState(state) || (failureKind != "" && !validFailure(failureKind)) {
		return integrations.Snapshot{}, ErrInvalidInput
	}
	observed := now.UnixMilli()
	connection := integrations.Connection{SchemaVersion: 1, ConnectorID: "gmail", State: state, ObservedAtUnixMS: observed}
	if state == "ready" || state == "degraded" {
		connection.GrantedScopes = []string{readonlyScope}
	}
	if lastSuccess != nil {
		value := lastSuccess.UnixMilli()
		if value > observed {
			return integrations.Snapshot{}, ErrInvalidInput
		}
		connection.LastSuccessAtUnixMS = &value
	}
	if failureKind != "" {
		connection.LastFailure = &integrations.Failure{Kind: failureKind, ObservedAtUnixMS: observed}
	}
	if (state == "ready" && (connection.LastSuccessAtUnixMS == nil || connection.LastFailure != nil)) ||
		(state == "degraded" && connection.LastFailure == nil) ||
		((state == "unavailable" || state == "revoked" || state == "unsupported") && connection.LastFailure == nil) {
		return integrations.Snapshot{}, ErrInvalidInput
	}
	return integrations.Snapshot{Descriptor: ConnectorDescriptor(), Connection: connection, Views: []views.ViewSnapshot{}}, nil
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
