package httptransport

import (
	"bytes"
	"encoding/json"
	"testing"

	"floe/server/internal/integrations"
	"floe/server/internal/pairing"
)

func TestEmptyCatalogAndConnectionsProjectAsArrays(t *testing.T) {
	catalogBytes, err := json.Marshal(catalogProjection(integrations.CatalogResult{}))
	if err != nil {
		t.Fatalf("marshal empty catalog projection: %v", err)
	}
	var catalog struct {
		Connectors json.RawMessage `json:"connectors"`
	}
	if err := json.Unmarshal(catalogBytes, &catalog); err != nil {
		t.Fatalf("decode empty catalog projection: %v", err)
	}
	if !bytes.Equal(bytes.TrimSpace(catalog.Connectors), []byte("[]")) {
		t.Errorf("empty catalog connectors encoded as %s, want []", catalog.Connectors)
	}

	connectionsBytes, err := json.Marshal(connectionsProjection(integrations.ConnectionsResult{}))
	if err != nil {
		t.Fatalf("marshal empty connections projection: %v", err)
	}
	var connections struct {
		Connections json.RawMessage `json:"connections"`
	}
	if err := json.Unmarshal(connectionsBytes, &connections); err != nil {
		t.Fatalf("decode empty connections projection: %v", err)
	}
	if !bytes.Equal(bytes.TrimSpace(connections.Connections), []byte("[]")) {
		t.Errorf("empty connections encoded as %s, want []", connections.Connections)
	}
}

func TestCatalogProjectionPreservesConnectionOmissionAndNulls(t *testing.T) {
	projected := catalogProjection(integrations.CatalogResult{
		SchemaVersion: 1,
		Connectors: []integrations.ConnectorCatalogEntry{
			{ID: "disconnected", Status: "disconnected"},
			{ID: "connected", Status: "error", HasConnection: true, Scope: nil},
		},
	})
	data, err := json.Marshal(projected)
	if err != nil {
		t.Fatalf("marshal catalog projection: %v", err)
	}
	var response struct {
		Connectors []map[string]json.RawMessage `json:"connectors"`
	}
	if err := json.Unmarshal(data, &response); err != nil {
		t.Fatalf("decode catalog projection: %v", err)
	}
	if len(response.Connectors) != 2 {
		t.Fatalf("catalog projected %d connectors, want 2", len(response.Connectors))
	}
	for _, key := range []string{"connection_id", "connection_revision", "incarnation", "epoch", "execution_owner", "identity_unverified", "scope"} {
		if _, exists := response.Connectors[0][key]; exists {
			t.Errorf("disconnected connector unexpectedly included %q", key)
		}
		if _, exists := response.Connectors[1][key]; !exists {
			t.Errorf("connected connector omitted %q", key)
		}
	}
	if string(response.Connectors[1]["scope"]) != "null" {
		t.Errorf("connected nil scope encoded as %s, want null", response.Connectors[1]["scope"])
	}
	if string(response.Connectors[1]["identity_unverified"]) != "false" {
		t.Errorf("connected false identity state encoded as %s, want false", response.Connectors[1]["identity_unverified"])
	}
	if string(response.Connectors[1]["connection_revision"]) != "0" || string(response.Connectors[1]["epoch"]) != "0" {
		t.Errorf("connected zero revisions were omitted or changed: revision=%s epoch=%s", response.Connectors[1]["connection_revision"], response.Connectors[1]["epoch"])
	}
}

func TestPairingStatusProjectionOmitsUndeliveredCredentials(t *testing.T) {
	projected, err := pairingProjection(pairing.StatusResult{
		SchemaVersion: 1, PairingID: "synthetic-pairing", Status: "pending",
		PersonID: "synthetic-person", DeviceID: "synthetic-device",
	})
	if err != nil {
		t.Fatalf("project pairing status: %v", err)
	}
	data, err := json.Marshal(projected)
	if err != nil {
		t.Fatalf("marshal pairing projection: %v", err)
	}
	var response map[string]json.RawMessage
	if err := json.Unmarshal(data, &response); err != nil {
		t.Fatalf("decode pairing projection: %v", err)
	}
	for _, key := range []string{"issuer", "issuer_fingerprint", "client_id", "token", "producer"} {
		if _, exists := response[key]; exists {
			t.Errorf("pending pairing unexpectedly included %q", key)
		}
	}
}
