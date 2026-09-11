package console

import (
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"floe/server/internal/credentials"
)

const otherFixturePersonID = "00000000-0000-4000-8000-000000000002"

func TestPairedClientConnectsDisconnectedCatalogAndReadsView(test *testing.T) {
	observedAt := time.Now().UTC()
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/api/states/sensor.temperature" || request.Header.Get("Authorization") != "Bearer person-private-token" {
			test.Fatalf("unexpected connector request: %s", request.URL.Path)
		}
		_, _ = fmt.Fprintf(writer, `{"entity_id":"sensor.temperature","state":"22","last_updated":%q,"attributes":{"friendly_name":"Temperature"}}`, observedAt.Format(time.RFC3339Nano))
	}))
	defer upstream.Close()
	fixture := setup(test)
	_, token := fixture.pair()
	catalog := fixture.value(fixture.call(http.MethodGet, "/v1/connectors", nil, token))["connectors"].([]any)
	for _, item := range catalog {
		connector := item.(map[string]any)
		if connector["id"] == "home_assistant.states" && connector["status"] != "disconnected" {
			test.Fatalf("initial catalog state: %#v", connector)
		}
	}
	connected := fixture.call(http.MethodPost, "/v1/connectors/home_assistant.states/connect", map[string]any{
		"schema_version": 1, "secret": "person-private-token",
		"scope": map[string]any{"base_url": upstream.URL, "entities": []string{"sensor.temperature"}},
	}, token)
	if connected.Code != http.StatusCreated {
		test.Fatal(connected.Body.String())
	}
	view := fixture.value(fixture.call(http.MethodPost, "/v1/views/life.logistics", map[string]any{"schema_version": 1}, token))
	if view["view"].(map[string]any)["view_id"] != "life.logistics" {
		test.Fatalf("view not connected: %#v", view)
	}
}

func TestForeignPersonCannotExecuteOwnedConnectorRuntime(test *testing.T) {
	fixture := setup(test)
	_, ownerToken := fixture.pair()
	fixture.ownConnector("github.issues")
	now := time.Now().UnixMilli()
	runtime := &fakeContextRuntime{view: map[string]any{"schema_version": 1, "view_id": "work.context", "source_handle": "work:fixture", "scope_handle": "workspace:fixture", "observed_at_unix_ms": now - 1, "expires_at_unix_ms": now + 299_999, "coverage_complete": true, "items": []any{}}}
	fixture.console.mu.Lock()
	fixture.console.work = map[string]WorkContextRuntime{"github.issues.fixture": runtime}
	fixture.console.state.Clients["foreign"] = pairedClient{TokenHash: digest("foreign-token"), PersonID: otherFixturePersonID, DeviceID: "foreign-device"}
	fixture.console.mu.Unlock()

	if response := fixture.call(http.MethodPost, "/v1/views/work.context", map[string]any{"schema_version": 1}, "foreign-token"); response.Code != http.StatusNotFound {
		test.Fatalf("foreign Person executed connector: %d %s", response.Code, response.Body.String())
	}
	if runtime.reads.Load() != 0 {
		test.Fatalf("foreign Person reached runtime %d times", runtime.reads.Load())
	}
	if response := fixture.call(http.MethodPost, "/v1/views/work.context", map[string]any{"schema_version": 1}, ownerToken); response.Code != http.StatusOK {
		test.Fatalf("owner could not execute connector: %d %s", response.Code, response.Body.String())
	}
}

func TestRevokingLastClientRemovesPersonConnectorLifecycle(test *testing.T) {
	fixture := setup(test)
	clientID, token := fixture.pair()
	response := fixture.call(http.MethodPost, "/v1/connectors/github.issues/connect", map[string]any{
		"schema_version": 1,
		"secret":         "person-private-token",
		"scope":          map[string]any{"owner": "floe", "repository": "server"},
	}, token)
	if response.Code != http.StatusCreated {
		test.Fatal(response.Body.String())
	}
	fixture.console.mu.Lock()
	record, connected := fixture.console.connectionForPerson("github.issues", fixturePersonID)
	credential := record.Credential
	fixture.console.mu.Unlock()
	if !connected || fixture.vault.values[credential] == "" {
		test.Fatal("Person connector was not established")
	}

	if response := fixture.call(http.MethodPost, "/manage/api/client/delete", map[string]string{"id": clientID}, ""); response.Code != http.StatusOK {
		test.Fatal(response.Body.String())
	}
	fixture.console.mu.Lock()
	_, connectionRetained := fixture.console.state.Connections[record.ConnectionID]
	runtimeRetained := len(fixture.console.work) != 0
	fixture.console.lastPair = time.Time{}
	fixture.console.mu.Unlock()
	if connectionRetained || runtimeRetained || fixture.vault.values[credential] != "" {
		test.Fatal("client revocation retained connector state, runtime, or credential")
	}

	started := fixture.value(fixture.call(http.MethodPost, "/pair/start", map[string]string{"person_id": otherFixturePersonID, "device_id": "other-device"}, ""))
	proof := started["proof"].(string)
	fixture.value(fixture.call(http.MethodPost, "/manage/api/pair/approve", map[string]any{"id": started["id"]}, ""))
	other := fixture.value(fixture.call(http.MethodPost, "/pair/poll", map[string]string{"proof": proof}, ""))
	otherToken := other["token"].(string)
	if response := fixture.call(http.MethodPost, "/v1/views/work.context", map[string]any{"schema_version": 1}, otherToken); response.Code != http.StatusNotFound {
		test.Fatalf("re-paired Person read revoked owner's view: %d %s", response.Code, response.Body.String())
	}
}

func TestPersistedConnectionRequiresLivePersonOwner(test *testing.T) {
	fixture := setup(test)
	fixture.console.mu.Lock()
	fixture.console.state.Connections["gmail.orphan"] = connectionRecord{ConnectionID: "gmail.orphan", ConnectorID: "gmail", PersonID: fixturePersonID}
	if err := fixture.console.save(fixture.console.state); err != nil {
		test.Fatal(err)
	}
	fixture.console.mu.Unlock()
	if _, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil); err == nil {
		test.Fatal("orphaned Person connection was accepted")
	}
}

func TestPersistedCredentialMustMatchPersonAndConnection(test *testing.T) {
	fixture := setup(test)
	fixture.pair()
	wrongCredential, err := credentials.ConnectionName(githubTokenKey, "github.issues.someone-else", fixturePersonID)
	if err != nil {
		test.Fatal(err)
	}
	fixture.console.mu.Lock()
	fixture.console.state.Connections["github.issues.fixture"] = connectionRecord{
		ConnectionID: "github.issues.fixture", ConnectorID: "github.issues", PersonID: fixturePersonID,
		Scope: map[string]any{"owner": "floe", "repository": "server"}, Credential: wrongCredential,
	}
	if err := fixture.console.save(fixture.console.state); err != nil {
		test.Fatal(err)
	}
	fixture.console.mu.Unlock()
	if _, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil); err == nil {
		test.Fatal("credential from another connection was accepted")
	}
}
