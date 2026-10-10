package microsoftcalendar

import (
	"encoding/json"
	"floe/server/internal/integrations"
	"floe/server/internal/views"
)

func ConnectorDescriptor() integrations.Descriptor {
	return integrations.Descriptor{SchemaVersion: 1, ID: "calendar.microsoft", Version: "1.0.0", Provider: "microsoft_calendar", Execution: map[string]any{"kind": "server"}, Capabilities: []integrations.Capability{{SchemaVersion: 1, ID: "calendar.events.read", Version: "1.0.0", Authority: "observe", RequiredScopes: []string{observeScope}, OutputViewID: "calendar.timeline"}}, Views: []views.ViewDescriptor{{SchemaVersion: 1, ID: "calendar.timeline", Version: "1.0.0", DataClass: "personal", Retention: "ephemeral", FreshnessTTLMS: 300_000, MaxItems: maxItems, MaxBytes: 65_536, ProvenanceRequired: true}}}
}

func ConnectionSnapshot(view views.CalendarView) (integrations.Snapshot, error) {
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 || len(view.Items) > maxItems {
		return integrations.Snapshot{}, ErrInvalidResponse
	}
	lastSuccess := view.ObservedAtUnixMS
	return integrations.Snapshot{Descriptor: ConnectorDescriptor(), Connection: integrations.Connection{SchemaVersion: 1, ConnectorID: "calendar.microsoft", State: "ready", GrantedScopes: []string{observeScope}, ObservedAtUnixMS: view.ObservedAtUnixMS, LastSuccessAtUnixMS: &lastSuccess}, Views: []views.ViewSnapshot{{SchemaVersion: 1, ViewID: view.ViewID, SourceHandle: view.SourceHandle, ObservedAtUnixMS: view.ObservedAtUnixMS, ExpiresAtUnixMS: view.ExpiresAtUnixMS, ItemCount: len(view.Items), ByteCount: len(encoded), ProvenanceCount: len(view.Items)}}}, nil
}
