package googledrive

import (
	"floe/server/internal/integrations"
	"floe/server/internal/views"
)

const observeScope = "https://www.googleapis.com/auth/drive.readonly"

func ConnectorDescriptor() integrations.Descriptor {
	return integrations.Descriptor{
		SchemaVersion: 1, ID: "google_drive.files", Version: "1.0.0", Provider: "google_drive", Execution: map[string]any{"kind": "server"},
		Capabilities: []integrations.Capability{{SchemaVersion: 1, ID: "work.files.read", Version: "1.0.0", Authority: "observe", RequiredScopes: []string{observeScope}, OutputViewID: "work.context"}},
		Views:        []views.ViewDescriptor{{SchemaVersion: 1, ID: "work.context", Version: "1.0.0", DataClass: "personal", Retention: "ephemeral", FreshnessTTLMS: 300_000, MaxItems: maxFiles, MaxBytes: 65_536, ProvenanceRequired: true}},
	}
}
