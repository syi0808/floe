package homeassistant

import (
	"encoding/json"

	"floe/server/internal/connectors/common"
)

const observeScope = "home.states.read"

func ConnectorDescriptor() common.Descriptor {
	return common.Descriptor{
		SchemaVersion: 1,
		ID:            "home_assistant.states",
		Version:       "1.0.0",
		Provider:      "home_assistant",
		Execution:     map[string]any{"kind": "server"},
		Capabilities: []common.Capability{{
			SchemaVersion:  1,
			ID:             "home.states.read",
			Version:        "1.0.0",
			Authority:      "observe",
			RequiredScopes: []string{observeScope},
			OutputViewID:   "life.logistics",
		}},
		Views: []common.ViewDescriptor{{
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

func ConnectionSnapshot(view LogisticsView) (common.Snapshot, error) {
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 || len(view.Items) > maxEntities {
		return common.Snapshot{}, ErrInvalidResponse
	}
	lastSuccess := view.ObservedAtUnixMS
	return common.Snapshot{
		Descriptor: ConnectorDescriptor(),
		Connection: common.Connection{
			SchemaVersion:       1,
			ConnectorID:         "home_assistant.states",
			State:               "ready",
			GrantedScopes:       []string{observeScope},
			ObservedAtUnixMS:    view.ObservedAtUnixMS,
			LastSuccessAtUnixMS: &lastSuccess,
		},
		Views: []common.ViewSnapshot{{
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
