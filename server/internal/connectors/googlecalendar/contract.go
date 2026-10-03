package googlecalendar

import (
	"floe/server/internal/integrations"
	"floe/server/internal/views"
)

func ConnectorDescriptor() integrations.Descriptor {
	return integrations.Descriptor{SchemaVersion: 1, ID: "calendar.google", Version: "1.0.0", Provider: "google_calendar", Execution: map[string]any{"kind": "server"}, Capabilities: []integrations.Capability{{SchemaVersion: 1, ID: "calendar.events.read", Version: "1.0.0", Authority: "observe", RequiredScopes: []string{observeScope}, OutputViewID: "calendar.timeline"}}, Views: []views.ViewDescriptor{{SchemaVersion: 1, ID: "calendar.timeline", Version: "1.0.0", DataClass: "personal", Retention: "ephemeral", FreshnessTTLMS: 300_000, MaxItems: maxItems, MaxBytes: 65_536, ProvenanceRequired: true}}}
}
