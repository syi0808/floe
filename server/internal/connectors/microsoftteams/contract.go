package microsoftteams

import (
	"encoding/json"

	"floe/server/internal/connectors/common"
)

const observeScope = "ChannelMessage.Read.All"

func ConnectorDescriptor() common.Descriptor {
	return common.Descriptor{
		SchemaVersion: 1,
		ID:            "microsoft.teams",
		Version:       "1.0.0",
		Provider:      "microsoft_teams",
		Execution:     map[string]any{"kind": "server"},
		Capabilities: []common.Capability{{
			SchemaVersion: 1, ID: "work.communication.read", Version: "1.0.0", Authority: "observe", RequiredScopes: []string{observeScope}, OutputViewID: "work.context",
		}},
		Views: []common.ViewDescriptor{{
			SchemaVersion: 1, ID: "work.context", Version: "1.0.0", DataClass: "personal", Retention: "short_lived_cache", FreshnessTTLMS: 300_000, MaxItems: maxMessages, MaxBytes: 65_536, ProvenanceRequired: true,
		}},
	}
}

func ConnectionSnapshot(view common.WorkContextView) (common.Snapshot, error) {
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 || len(view.Items) > maxMessages {
		return common.Snapshot{}, ErrInvalidResponse
	}
	lastSuccess := view.ObservedAtUnixMS
	return common.Snapshot{
		Descriptor: ConnectorDescriptor(),
		Connection: common.Connection{SchemaVersion: 1, ConnectorID: "microsoft.teams", State: "ready", GrantedScopes: []string{observeScope}, ObservedAtUnixMS: view.ObservedAtUnixMS, LastSuccessAtUnixMS: &lastSuccess},
		Views:      []common.ViewSnapshot{{SchemaVersion: 1, ViewID: view.ViewID, SourceHandle: view.SourceHandle, ObservedAtUnixMS: view.ObservedAtUnixMS, ExpiresAtUnixMS: view.ExpiresAtUnixMS, ItemCount: len(view.Items), ByteCount: len(encoded), ProvenanceCount: len(view.Items)}},
	}, nil
}
