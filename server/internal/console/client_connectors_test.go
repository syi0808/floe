package console

import (
	"context"
	"encoding/json"
	"net/http"
	"strings"
	"testing"

	"floe/server/internal/credentials"
)

type clientOAuthRuntime struct {
	status     string
	actions    []string
	credential string
}

func (runtime *clientOAuthRuntime) BindCredential(name string) error {
	runtime.credential = name
	return nil
}

func createdValue(test *testing.T, responseBody *strings.Reader) map[string]any {
	test.Helper()
	var value map[string]any
	if json.NewDecoder(responseBody).Decode(&value) != nil {
		test.Fatal("invalid JSON")
	}
	return value
}

func (runtime *clientOAuthRuntime) Action(_ context.Context, action string) (any, error) {
	runtime.actions = append(runtime.actions, action)
	switch action {
	case "login":
		runtime.status = "pending"
	case "cancel", "logout":
		runtime.status = "disconnected"
	case "status":
		if runtime.status == "pending" {
			runtime.status = "connected"
		}
	}
	value := map[string]any{"status": runtime.status, "scope": "Mail.Read"}
	if runtime.status == "pending" {
		value["auth_url"] = "https://login.example.test/authorize?code_challenge=public-challenge"
	}
	return value, nil
}

func TestPairedConnectorCatalogIncludesDisconnectedAndUnavailableProviders(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	value := fixture.value(fixture.call(http.MethodGet, "/v1/connectors", nil, token))
	connectors := value["connectors"].([]any)
	if len(connectors) != len(clientConnectorDefinitions) {
		test.Fatalf("catalog length: %d", len(connectors))
	}
	byID := map[string]map[string]any{}
	for _, item := range connectors {
		connector := item.(map[string]any)
		byID[connector["id"].(string)] = connector
	}
	if byID["github.issues"]["status"] != "disconnected" || byID["github.issues"]["available"] != true {
		test.Fatalf("PAT connector missing from catalog: %#v", byID["github.issues"])
	}
	if byID["gmail"]["status"] != "unavailable" || byID["gmail"]["available"] != false {
		test.Fatalf("unconfigured OAuth connector not explicit: %#v", byID["gmail"])
	}
}

func TestPairedOAuthConnectionReturnsOnlyAuthorizationURLAndServerAttempt(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	runtime := &clientOAuthRuntime{status: "disconnected"}
	fixture.console.mu.Lock()
	fixture.console.microsoftAuth = runtime
	fixture.console.mu.Unlock()

	response := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	if response.Code != http.StatusCreated {
		test.Fatal(response.Body.String())
	}
	started := createdValue(test, strings.NewReader(response.Body.String()))
	if started["status"] != "pending" || started["authorization_url"] == "" || started["attempt_id"] == "" || started["person_id"] != fixturePersonID || started["device_id"] != fixtureDeviceID {
		test.Fatalf("invalid OAuth start: %#v", started)
	}
	encoded, _ := json.Marshal(started)
	if strings.Contains(string(encoded), "verifier") || strings.Contains(string(encoded), "refresh_token") || strings.Contains(string(encoded), "access_token") {
		test.Fatalf("server-owned OAuth secret escaped: %s", encoded)
	}
	attemptID := started["attempt_id"].(string)
	status := fixture.value(fixture.call(http.MethodGet, "/v1/connectors/microsoft.mail/connection-attempts/"+attemptID, nil, token))
	if status["status"] != "connected" || status["authorization_url"] != nil || status["person_id"] != fixturePersonID || status["device_id"] != fixtureDeviceID {
		test.Fatalf("OAuth status did not settle: %#v", status)
	}
	if strings.Join(runtime.actions, ",") != "login,status" {
		test.Fatalf("unexpected OAuth lifecycle: %#v", runtime.actions)
	}
	if !strings.HasPrefix(runtime.credential, "FLOE_MICROSOFT_MAIL_OAUTH:") || len(runtime.credential) != len("FLOE_MICROSOFT_MAIL_OAUTH:")+64 {
		test.Fatalf("OAuth credential was not Person scoped: %q", runtime.credential)
	}
}

func TestPairedSecretConnectionUsesScopedVaultAndNeverEchoesSecret(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	secret := "private-github-pat"
	response := fixture.call(http.MethodPost, "/v1/connectors/github.issues/connect", map[string]any{
		"schema_version": 1,
		"secret":         secret,
		"scope":          map[string]any{"owner": "floe", "repository": "product"},
	}, token)
	if response.Code != http.StatusCreated {
		test.Fatal(response.Body.String())
	}
	started := createdValue(test, strings.NewReader(response.Body.String()))
	encoded, _ := json.Marshal(started)
	if started["status"] != "connected" || strings.Contains(string(encoded), secret) {
		test.Fatalf("secret connection response: %s", encoded)
	}
	fixture.console.mu.Lock()
	connection, exists := fixture.console.connectionForPerson("github.issues", fixturePersonID)
	fixture.console.mu.Unlock()
	if !exists || connection.Credential == "" || connection.Credential == githubTokenKey || fixture.vault.values[connection.Credential] != secret {
		test.Fatalf("credential was not connection scoped: %#v %#v", connection, fixture.vault.values)
	}
	state, _ := json.Marshal(fixture.console.state)
	if strings.Contains(string(state), secret) || !strings.Contains(connection.Credential, "FLOE_CONNECTOR_GITHUB_TOKEN:") {
		test.Fatalf("plaintext credential persisted: %s", state)
	}

	updated := fixture.value(fixture.call(http.MethodPatch, "/v1/connectors/github.issues/scope", map[string]any{"schema_version": 1, "scope": map[string]any{"owner": "floe", "repository": "server"}}, token))
	if updated["connection_id"] != connection.ConnectionID || updated["person_id"] != fixturePersonID || updated["device_id"] != fixtureDeviceID || updated["scope"].(map[string]any)["repository"] != "server" || fixture.vault.values[connection.Credential] != secret {
		test.Fatalf("scope update changed ownership or secret: %#v", updated)
	}
	disconnected := fixture.value(fixture.call(http.MethodDelete, "/v1/connectors/github.issues", nil, token))
	if disconnected["connection_id"] != connection.ConnectionID || disconnected["person_id"] != fixturePersonID || disconnected["device_id"] != fixtureDeviceID {
		test.Fatalf("disconnect ownership missing: %#v", disconnected)
	}
	if _, exists := fixture.vault.values[connection.Credential]; exists {
		test.Fatal("disconnect retained scoped credential")
	}
}

func TestConnectorMutationsRejectCrossPersonCredentials(test *testing.T) {
	fixture := setup(test)
	otherToken := "other-device-token"
	otherPersonID := "00000000-0000-4000-8000-000000000002"
	fixture.console.mu.Lock()
	fixture.console.state.Clients["other"] = pairedClient{TokenHash: digest(otherToken), PersonID: otherPersonID, DeviceID: "other-device"}
	connectionID := "github.issues.foreign"
	fixture.console.state.Connections[connectionID] = connectionRecord{ConnectionID: connectionID, ConnectorID: "github.issues", PersonID: fixturePersonID}
	fixture.console.mu.Unlock()

	foreign := fixture.call(http.MethodDelete, "/v1/connectors/github.issues", nil, otherToken)
	if foreign.Code != http.StatusForbidden || !strings.Contains(foreign.Body.String(), "connection_owned_by_another_person") {
		test.Fatalf("cross-person mutation not blocked: %d %s", foreign.Code, foreign.Body.String())
	}
}

func TestConnectorAttemptIsBoundToPairedDevice(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	runtime := &clientOAuthRuntime{status: "disconnected"}
	fixture.console.mu.Lock()
	fixture.console.microsoftAuth = runtime
	fixture.console.state.Clients["second-device"] = pairedClient{TokenHash: digest("second-device-token"), PersonID: fixturePersonID, DeviceID: "second-device"}
	fixture.console.mu.Unlock()
	startedResponse := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	if startedResponse.Code != http.StatusCreated {
		test.Fatal(startedResponse.Body.String())
	}
	started := createdValue(test, strings.NewReader(startedResponse.Body.String()))
	response := fixture.call(http.MethodGet, "/v1/connectors/microsoft.mail/connection-attempts/"+started["attempt_id"].(string), nil, "second-device-token")
	if response.Code != http.StatusNotFound || !strings.Contains(response.Body.String(), "attempt_not_found") {
		test.Fatalf("cross-device attempt disclosed: %d %s", response.Code, response.Body.String())
	}
}

func TestCancellingOAuthAttemptRemovesPendingConnection(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	runtime := &clientOAuthRuntime{status: "disconnected"}
	fixture.console.mu.Lock()
	fixture.console.microsoftAuth = runtime
	fixture.console.mu.Unlock()
	startedResponse := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	started := createdValue(test, strings.NewReader(startedResponse.Body.String()))
	attemptID := started["attempt_id"].(string)
	cancelled := fixture.value(fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connection-attempts/"+attemptID+"/cancel", map[string]any{}, token))
	if cancelled["status"] != "cancelled" {
		test.Fatalf("attempt was not cancelled: %#v", cancelled)
	}
	fixture.console.mu.Lock()
	_, exists := fixture.console.connectionForPerson("microsoft.mail", fixturePersonID)
	fixture.console.mu.Unlock()
	if exists || strings.Join(runtime.actions, ",") != "login,cancel" {
		test.Fatalf("cancel retained connection: %#v", runtime.actions)
	}
}

func TestConnectorScopeCapabilityIsExplicit(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	response := fixture.call(http.MethodPatch, "/v1/connectors/gmail/scope", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	if response.Code != http.StatusConflict || !strings.Contains(response.Body.String(), "capability_not_supported") {
		test.Fatalf("unsupported scope mutation was not explicit: %d %s", response.Code, response.Body.String())
	}
}

func TestOAuthRuntimeRebindsPersistedConnectionOnServerRestart(test *testing.T) {
	fixture := setup(test)
	fixture.pair()
	connectionID := "microsoft.mail.persisted"
	credential, err := credentials.ConnectionName("FLOE_MICROSOFT_MAIL_OAUTH", connectionID, fixturePersonID)
	if err != nil {
		test.Fatal(err)
	}
	fixture.console.mu.Lock()
	fixture.console.state.Connections[connectionID] = connectionRecord{ConnectionID: connectionID, ConnectorID: "microsoft.mail", PersonID: fixturePersonID, Scope: map[string]any{}, Credential: credential}
	if err := fixture.console.save(fixture.console.state); err != nil {
		test.Fatal(err)
	}
	fixture.console.mu.Unlock()
	reopened, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil)
	if err != nil {
		test.Fatal(err)
	}
	runtime := &clientOAuthRuntime{status: "disconnected"}
	reopened.SetMicrosoftMail(runtime, nil)
	if !strings.HasPrefix(runtime.credential, "FLOE_MICROSOFT_MAIL_OAUTH:") {
		test.Fatalf("persisted OAuth owner was not rebound: %q", runtime.credential)
	}
}
