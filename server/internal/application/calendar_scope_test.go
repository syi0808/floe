package application

import (
	"crypto/ed25519"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"reflect"
	"testing"

	"floe/server/internal/connections"
	"floe/server/internal/operation"
)

func TestCalendarResourceSetEditAdvancesOnlySourceAuthority(t *testing.T) {
	fixture := setup(t)
	clientID, token := fixture.pair()
	fixture.ownConnector("calendar.google")
	connectionID := fixtureConnectionID("calendar.google")
	fixture.console.calendarAuth = &calendarAuthorityIdentityRuntime{identity: "google:subject-a"}
	fixture.console.mu.Lock()
	record := fixture.console.state.Connections[connectionID]
	record.Scope = map[string]any{"calendar_ids": []string{"A"}}
	record.Incarnation = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"
	record.Epoch = 7
	record.Credential = "calendar-test-credential"
	record.ProviderIdentity = "google:subject-a"
	fixture.console.state.Connections[connectionID] = record
	fixture.console.mu.Unlock()
	definition, _ := connections.DefinitionFor("calendar.google")
	scope := connections.Scope{ClientID: clientID, PersonID: fixturePersonID, DeviceID: fixtureDeviceID}
	update := func(revision uint64, calendarIDs []any) {
		t.Helper()
		result := fixture.console.updateConnectorScope(scope, definition, connections.ScopeRequest{SchemaVersion: 1, ConnectionID: connectionID, ConnectionRevision: revision, Scope: map[string]any{"calendar_ids": calendarIDs}})
		if result.Category != operation.Ready {
			t.Fatalf("scope update: %+v", result)
		}
	}
	update(1, []any{"A", "B"})
	fixture.console.mu.Lock()
	changed := fixture.console.state.Connections[connectionID]
	fixture.console.mu.Unlock()
	if changed.Revision != 2 || changed.Epoch != 8 || changed.Incarnation != record.Incarnation || !reflect.DeepEqual(changed.Scope["calendar_ids"], []string{"A", "B"}) {
		t.Fatalf("resource change did not advance source once: %+v", changed)
	}
	update(2, []any{"B", "A"})
	fixture.console.mu.Lock()
	reordered := fixture.console.state.Connections[connectionID]
	fixture.console.mu.Unlock()
	if reordered.Revision != 2 || reordered.Epoch != 8 || !reflect.DeepEqual(reordered.Scope["calendar_ids"], []string{"A", "B"}) {
		t.Fatalf("resource reorder advanced authority: %+v", reordered)
	}
	resource := "calendar.timeline:" + connectionID
	response := fixture.call(http.MethodPost, "/v1/views/calendar.timeline/source-preview", map[string]any{"connector_id": "calendar.google", "connection_id": connectionID, "resource": resource}, token)
	if response.Code != http.StatusOK {
		t.Fatalf("generic calendar preview: %d %s", response.Code, response.Body.String())
	}
	var preview map[string]any
	if err := json.Unmarshal(response.Body.Bytes(), &preview); err != nil {
		t.Fatal(err)
	}
	descriptor, err := base64.RawURLEncoding.DecodeString(preview["descriptor_b64url"].(string))
	if err != nil {
		t.Fatal(err)
	}
	signature, err := base64.RawURLEncoding.DecodeString(preview["producer_signature"].(string))
	if err != nil {
		t.Fatal(err)
	}
	publicKey, err := base64.RawURLEncoding.DecodeString(preview["public_key"].(string))
	if err != nil || !ed25519.Verify(ed25519.PublicKey(publicKey), append([]byte("floe.remote.producer.v1\x00"), descriptor...), signature) {
		t.Fatalf("invalid source signature: %v", err)
	}
	var signed map[string]any
	if err := json.Unmarshal(descriptor, &signed); err != nil || signed["resource"] != resource || signed["connection_revision"] != float64(2) || signed["epoch"] != float64(8) || !reflect.DeepEqual(signed["source_resources"], []any{"A", "B"}) {
		t.Fatalf("signed source set is not current: %s %v", descriptor, err)
	}
}
