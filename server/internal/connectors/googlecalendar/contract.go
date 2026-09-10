package googlecalendar

import (
	"encoding/json"

	"floe/server/internal/connectors/common"
)

func ConnectorDescriptor() common.Descriptor {
	return common.Descriptor{SchemaVersion: 1, ID: "calendar.google", Version: "1.0.0", Provider: "google_calendar", Execution: map[string]any{"kind": "server"}, Capabilities: []common.Capability{{SchemaVersion: 1, ID: "calendar.events.read", Version: "1.0.0", Authority: "observe", RequiredScopes: []string{observeScope}, OutputViewID: "calendar.timeline"}}, Views: []common.ViewDescriptor{{SchemaVersion: 1, ID: "calendar.timeline", Version: "1.0.0", DataClass: "personal", Retention: "ephemeral", FreshnessTTLMS: 300_000, MaxItems: maxItems, MaxBytes: 65_536, ProvenanceRequired: true}}}
}

func ConnectionSnapshot(view CalendarView) (common.Snapshot, error) {
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 || len(view.Items) > maxItems {
		return common.Snapshot{}, ErrInvalidResponse
	}
	lastSuccess := view.ObservedAtUnixMS
	return common.Snapshot{Descriptor: ConnectorDescriptor(), Connection: common.Connection{SchemaVersion: 1, ConnectorID: "calendar.google", State: "ready", GrantedScopes: []string{observeScope}, ObservedAtUnixMS: view.ObservedAtUnixMS, LastSuccessAtUnixMS: &lastSuccess}, Views: []common.ViewSnapshot{{SchemaVersion: 1, ViewID: view.ViewID, SourceHandle: view.SourceHandle, ObservedAtUnixMS: view.ObservedAtUnixMS, ExpiresAtUnixMS: view.ExpiresAtUnixMS, ItemCount: len(view.Items), ByteCount: len(encoded), ProvenanceCount: len(view.Items)}}}, nil
}
