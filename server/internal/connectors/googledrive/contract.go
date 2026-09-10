package googledrive

import (
	"encoding/json"

	"floe/server/internal/connectors/common"
)

const observeScope = "https://www.googleapis.com/auth/drive.readonly"

func ConnectorDescriptor() common.Descriptor {
	return common.Descriptor{
		SchemaVersion: 1, ID: "google_drive.files", Version: "1.0.0", Provider: "google_drive", Execution: map[string]any{"kind": "server"},
		Capabilities: []common.Capability{{SchemaVersion: 1, ID: "work.files.read", Version: "1.0.0", Authority: "observe", RequiredScopes: []string{observeScope}, OutputViewID: "work.context"}},
		Views:        []common.ViewDescriptor{{SchemaVersion: 1, ID: "work.context", Version: "1.0.0", DataClass: "personal", Retention: "ephemeral", FreshnessTTLMS: 300_000, MaxItems: maxFiles, MaxBytes: 65_536, ProvenanceRequired: true}},
	}
}

func ConnectionSnapshot(view common.WorkContextView) (common.Snapshot, error) {
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 || len(view.Items) > maxFiles {
		return common.Snapshot{}, ErrInvalidResponse
	}
	lastSuccess := view.ObservedAtUnixMS
	return common.Snapshot{
		Descriptor: ConnectorDescriptor(),
		Connection: common.Connection{SchemaVersion: 1, ConnectorID: "google_drive.files", State: "ready", GrantedScopes: []string{observeScope}, ObservedAtUnixMS: view.ObservedAtUnixMS, LastSuccessAtUnixMS: &lastSuccess},
		Views:      []common.ViewSnapshot{{SchemaVersion: 1, ViewID: view.ViewID, SourceHandle: view.SourceHandle, ObservedAtUnixMS: view.ObservedAtUnixMS, ExpiresAtUnixMS: view.ExpiresAtUnixMS, ItemCount: len(view.Items), ByteCount: len(encoded), ProvenanceCount: len(view.Items)}},
	}, nil
}
