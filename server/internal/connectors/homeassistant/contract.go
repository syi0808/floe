package homeassistant

import (
	"encoding/json"
	"floe/server/internal/integrations"
	"floe/server/internal/views"
)

const observeScope = "home.states.read"

func ConnectorDescriptor() integrations.Descriptor {
	return integrations.Descriptor{
		SchemaVersion: 1,
		ID:            "home_assistant.states",
		Version:       "1.0.0",
		Provider:      "home_assistant",
		Execution:     map[string]any{"kind": "server"},
		Capabilities: []integrations.Capability{{
			SchemaVersion:  1,
			ID:             "home.states.read",
			Version:        "1.0.0",
			Authority:      "observe",
			RequiredScopes: []string{observeScope},
			OutputViewID:   "life.logistics",
		}},
		Views: []views.ViewDescriptor{{
			SchemaVersion:      1,
			ID:                 "life.logistics",
			Version:            "1.0.0",
			DataClass:          "personal",
			Retention:          "short_lived_cache",
			FreshnessTTLMS:     300_000,
			MaxItems:           maxEntities,
			MaxBytes:           65_536,
			ProvenanceRequired: true,
		}},
	}
}

func ConnectionSnapshot(view views.LogisticsView) (integrations.Snapshot, error) {
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 || len(view.Items) > maxEntities {
		return integrations.Snapshot{}, ErrInvalidResponse
	}
	lastSuccess := view.ObservedAtUnixMS
	return integrations.Snapshot{
		Descriptor: ConnectorDescriptor(),
		Connection: integrations.Connection{
			SchemaVersion:       1,
			ConnectorID:         "home_assistant.states",
			State:               "ready",
			GrantedScopes:       []string{observeScope},
			ObservedAtUnixMS:    view.ObservedAtUnixMS,
			LastSuccessAtUnixMS: &lastSuccess,
		},
		Views: []views.ViewSnapshot{{
			SchemaVersion:    1,
			ViewID:           view.ViewID,
			SourceHandle:     view.SourceHandle,
			ObservedAtUnixMS: view.ObservedAtUnixMS,
			ExpiresAtUnixMS:  view.ExpiresAtUnixMS,
			ItemCount:        len(view.Items),
			ByteCount:        len(encoded),
			ProvenanceCount:  len(view.Items),
		}},
	}, nil
}
