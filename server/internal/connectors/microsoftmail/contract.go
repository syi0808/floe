package microsoftmail

import (
	"encoding/json"
	"floe/server/internal/integrations"
	"floe/server/internal/views"
)

func ConnectorDescriptor() integrations.Descriptor {
	return integrations.Descriptor{
		SchemaVersion: 1, ID: "microsoft.mail", Version: "1.0.0", Provider: "microsoft", Execution: map[string]any{"kind": "server"},
		Capabilities: []integrations.Capability{{SchemaVersion: 1, ID: "mail.communication.read", Version: "1.0.0", Authority: "observe", RequiredScopes: []string{observeScope}, OutputViewID: "mail.communication"}},
		Views:        []views.ViewDescriptor{{SchemaVersion: 1, ID: "mail.communication", Version: "1.0.0", DataClass: "personal", Retention: "ephemeral", FreshnessTTLMS: 300_000, MaxItems: maxItems, MaxBytes: 65_536, ProvenanceRequired: true}},
	}
}

func ConnectionSnapshot(view views.CommunicationView) (integrations.Snapshot, error) {
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 || len(view.Items) > maxItems {
		return integrations.Snapshot{}, ErrInvalidResponse
	}
	lastSuccess := view.ObservedAtUnixMS
	return integrations.Snapshot{Descriptor: ConnectorDescriptor(), Connection: integrations.Connection{SchemaVersion: 1, ConnectorID: "microsoft.mail", State: "ready", GrantedScopes: []string{observeScope}, ObservedAtUnixMS: view.ObservedAtUnixMS, LastSuccessAtUnixMS: &lastSuccess}, Views: []views.ViewSnapshot{{SchemaVersion: 1, ViewID: view.ViewID, SourceHandle: view.SourceHandle, ObservedAtUnixMS: view.ObservedAtUnixMS, ExpiresAtUnixMS: view.ExpiresAtUnixMS, ItemCount: len(view.Items), ByteCount: len(encoded), ProvenanceCount: len(view.Items)}}}, nil
}
