package microsoftteams

import (
 "floe/server/internal/integrations"
 "floe/server/internal/views"
	"encoding/json"

	
)

const observeScope = "ChannelMessage.Read.All"

func ConnectorDescriptor() integrations.Descriptor {
	return integrations.Descriptor{
		SchemaVersion: 1,
		ID:            "microsoft.teams",
		Version:       "1.0.0",
		Provider:      "microsoft_teams",
		Execution:     map[string]any{"kind": "server"},
		Capabilities: []integrations.Capability{{
			SchemaVersion: 1, ID: "work.communication.read", Version: "1.0.0", Authority: "observe", RequiredScopes: []string{observeScope}, OutputViewID: "work.context",
		}},
		Views: []views.ViewDescriptor{{
			SchemaVersion: 1, ID: "work.context", Version: "1.0.0", DataClass: "personal", Retention: "short_lived_cache", FreshnessTTLMS: 300_000, MaxItems: maxMessages, MaxBytes: 65_536, ProvenanceRequired: true,
		}},
	}
}

func ConnectionSnapshot(view views.WorkContextView) (integrations.Snapshot, error) {
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 || len(view.Items) > maxMessages {
		return integrations.Snapshot{}, ErrInvalidResponse
	}
	lastSuccess := view.ObservedAtUnixMS
	return integrations.Snapshot{
		Descriptor: ConnectorDescriptor(),
		Connection: integrations.Connection{SchemaVersion: 1, ConnectorID: "microsoft.teams", State: "ready", GrantedScopes: []string{observeScope}, ObservedAtUnixMS: view.ObservedAtUnixMS, LastSuccessAtUnixMS: &lastSuccess},
		Views:      []views.ViewSnapshot{{SchemaVersion: 1, ViewID: view.ViewID, SourceHandle: view.SourceHandle, ObservedAtUnixMS: view.ObservedAtUnixMS, ExpiresAtUnixMS: view.ExpiresAtUnixMS, ItemCount: len(view.Items), ByteCount: len(encoded), ProvenanceCount: len(view.Items)}},
	}, nil
}
