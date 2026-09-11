package console

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"floe/server/internal/credentials"
)

type clientOAuthRuntime struct {
	status     string
	actions    []string
	credential string
	vault      Vault
	failures   map[string]error
}

func (runtime *clientOAuthRuntime) BindCredential(name string) error {
	runtime.credential = name
	return nil
}

func (runtime *clientOAuthRuntime) Ready() bool {
	if runtime.vault == nil || runtime.credential == "" {
		return false
	}
	value, err := runtime.vault.Get(runtime.credential)
	return err == nil && value != ""
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
	if err := runtime.failures[action]; err != nil {
		return nil, err
	}
	switch action {
	case "login":
		runtime.status = "pending"
	case "cancel":
		runtime.status = "disconnected"
	case "logout":
		if runtime.vault != nil {
			if err := runtime.vault.Delete(runtime.credential); err != nil {
				return nil, err
			}
		}
		runtime.status = "disconnected"
	case "status":
		if runtime.status == "pending" {
			runtime.status = "connected"
			if runtime.vault != nil {
				_ = runtime.vault.Put(runtime.credential, `{"access_token":"fixture"}`)
			}
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
	runtime := &clientOAuthRuntime{status: "disconnected", vault: fixture.vault}
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
	fixture.vault.values[credential] = `{"access_token":"persisted"}`
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

func TestPendingOAuthAttemptIsNotPersistedAndRestartCanRetry(test *testing.T) {
	managementFixture := setup(test)
	_, token := managementFixture.pair()
	runtime := &clientOAuthRuntime{status: "disconnected", vault: managementFixture.vault}
	managementFixture.console.SetMicrosoftMail(runtime, nil)
	startedResponse := managementFixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	started := createdValue(test, strings.NewReader(startedResponse.Body.String()))
	if started["status"] != "pending" {
		test.Fatalf("OAuth did not remain pending: %#v", started)
	}
	managementFixture.console.mu.Lock()
	_, stored := managementFixture.console.connectionForPerson("microsoft.mail", fixturePersonID)
	managementFixture.console.mu.Unlock()
	if stored {
		test.Fatal("pending OAuth was persisted as a connection")
	}
	reopened, err := New(managementFixture.console.directory, managementFixture.console.address, managementFixture.vault, nil)
	if err != nil {
		test.Fatal(err)
	}
	restartedRuntime := &clientOAuthRuntime{status: "disconnected", vault: managementFixture.vault}
	reopened.SetMicrosoftMail(restartedRuntime, nil)
	restartedFixture := &fixture{console: reopened, vault: managementFixture.vault, test: test}
	catalog := restartedFixture.value(restartedFixture.call(http.MethodGet, "/v1/connectors", nil, token))
	if connectorCatalogItem(catalog, "microsoft.mail")["status"] != "disconnected" {
		test.Fatalf("restart exposed pending attempt as connected: %#v", catalog)
	}
	response := restartedFixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	if response.Code != http.StatusCreated {
		test.Fatalf("restart blocked OAuth retry: %d %s", response.Code, response.Body.String())
	}
}

func TestFailedOAuthAttemptCleansCredentialAndAllowsRetry(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	runtime := &clientOAuthRuntime{status: "disconnected", vault: fixture.vault}
	fixture.console.SetMicrosoftMail(runtime, nil)
	startedResponse := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	started := createdValue(test, strings.NewReader(startedResponse.Body.String()))
	credential := runtime.credential
	fixture.vault.values[credential] = `{"partial":"credential"}`
	runtime.status = "disconnected"
	status := fixture.value(fixture.call(http.MethodGet, "/v1/connectors/microsoft.mail/connection-attempts/"+started["attempt_id"].(string), nil, token))
	if status["status"] != "failed" || status["error"].(map[string]any)["code"] != "authorization_interrupted" {
		test.Fatalf("failure not recorded: %#v", status)
	}
	if _, exists := fixture.vault.values[credential]; exists {
		test.Fatal("failed OAuth retained partial credential")
	}
	fixture.console.mu.Lock()
	_, stored := fixture.console.connectionForPerson("microsoft.mail", fixturePersonID)
	fixture.console.mu.Unlock()
	if stored {
		test.Fatal("failed OAuth retained connection record")
	}
	if response := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token); response.Code != http.StatusCreated {
		test.Fatalf("failed OAuth blocked retry: %d %s", response.Code, response.Body.String())
	}
}

func TestOAuthStatusTransportFailureRemainsRetryable(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	runtime := &clientOAuthRuntime{status: "disconnected", vault: fixture.vault}
	fixture.console.SetMicrosoftMail(runtime, nil)
	startedResponse := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	started := createdValue(test, strings.NewReader(startedResponse.Body.String()))
	runtime.failures = map[string]error{"status": errors.New("runtime busy")}
	path := "/v1/connectors/microsoft.mail/connection-attempts/" + started["attempt_id"].(string)
	transient := fixture.value(fixture.call(http.MethodGet, path, nil, token))
	if transient["status"] != "pending" || transient["error"].(map[string]any)["code"] != "connector_authorization_unavailable" {
		test.Fatalf("transient status failure terminated OAuth: %#v", transient)
	}
	delete(runtime.failures, "status")
	settled := fixture.value(fixture.call(http.MethodGet, path, nil, token))
	if settled["status"] != "connected" {
		test.Fatalf("OAuth status did not recover: %#v", settled)
	}
}

func TestOAuthCatalogRequiresCredentialReadinessAndReconnectRepairsStaleRecord(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	runtime := &clientOAuthRuntime{status: "disconnected", vault: fixture.vault}
	fixture.console.SetMicrosoftMail(runtime, nil)
	connectionID := "microsoft.mail.stale"
	credential, _ := credentials.ConnectionName("FLOE_MICROSOFT_MAIL_OAUTH", connectionID, fixturePersonID)
	fixture.console.mu.Lock()
	fixture.console.state.Connections[connectionID] = connectionRecord{ConnectionID: connectionID, ConnectorID: "microsoft.mail", PersonID: fixturePersonID, Scope: map[string]any{}, Credential: credential}
	if err := fixture.console.save(fixture.console.state); err != nil {
		test.Fatal(err)
	}
	fixture.console.mu.Unlock()
	catalog := fixture.value(fixture.call(http.MethodGet, "/v1/connectors", nil, token))
	if connectorCatalogItem(catalog, "microsoft.mail")["status"] != "error" {
		test.Fatalf("missing OAuth credential reported connected: %#v", catalog)
	}
	response := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	if response.Code != http.StatusCreated {
		test.Fatalf("stale record blocked reconnect: %d %s", response.Code, response.Body.String())
	}
}

func TestOAuthDisconnectSaveFailureDoesNotDestroyCredential(test *testing.T) {
	fixture, token, runtime, credential := connectedOAuthFixture(test)
	statePath := filepath.Join(fixture.console.directory, "state.json")
	backupPath := filepath.Join(fixture.console.directory, "state.backup")
	if err := os.Rename(statePath, backupPath); err != nil {
		test.Fatal(err)
	}
	if err := os.Mkdir(statePath, 0700); err != nil {
		test.Fatal(err)
	}
	response := fixture.call(http.MethodDelete, "/v1/connectors/microsoft.mail", nil, token)
	if err := os.Remove(statePath); err != nil {
		test.Fatal(err)
	}
	if err := os.Rename(backupPath, statePath); err != nil {
		test.Fatal(err)
	}
	if response.Code != http.StatusInternalServerError || runtime.actions[len(runtime.actions)-1] == "logout" {
		test.Fatalf("save failure ran destructive cleanup: %d %#v", response.Code, runtime.actions)
	}
	if fixture.vault.values[credential] == "" {
		test.Fatal("save failure destroyed credential")
	}
}

func TestOAuthDisconnectDeleteFailureRollsBackConnection(test *testing.T) {
	fixture, token, _, credential := connectedOAuthFixture(test)
	fixture.vault.failDeletes = 2
	response := fixture.call(http.MethodDelete, "/v1/connectors/microsoft.mail", nil, token)
	if response.Code != http.StatusInternalServerError {
		test.Fatalf("credential cleanup failure was hidden: %d %s", response.Code, response.Body.String())
	}
	fixture.console.mu.Lock()
	_, retained := fixture.console.connectionForPerson("microsoft.mail", fixturePersonID)
	fixture.console.mu.Unlock()
	if !retained || fixture.vault.values[credential] == "" {
		test.Fatal("failed cleanup left connection and credential inconsistent")
	}
}

func TestOAuthCancelCleanupCanBeRetried(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	runtime := &clientOAuthRuntime{status: "disconnected", vault: fixture.vault}
	fixture.console.SetMicrosoftMail(runtime, nil)
	startedResponse := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	started := createdValue(test, strings.NewReader(startedResponse.Body.String()))
	fixture.vault.values[runtime.credential] = `{"partial":"credential"}`
	fixture.vault.failDeletes = 1
	path := "/v1/connectors/microsoft.mail/connection-attempts/" + started["attempt_id"].(string) + "/cancel"
	if response := fixture.call(http.MethodPost, path, map[string]any{}, token); response.Code != http.StatusInternalServerError {
		test.Fatalf("cleanup failure was hidden: %d %s", response.Code, response.Body.String())
	}
	cancelled := fixture.value(fixture.call(http.MethodPost, path, map[string]any{}, token))
	if cancelled["status"] != "cancelled" {
		test.Fatalf("cleanup retry did not cancel: %#v", cancelled)
	}
	if _, exists := fixture.vault.values[runtime.credential]; exists {
		test.Fatal("cancel retry retained credential")
	}
}

func TestOAuthCancelRuntimeFailureLeavesRetryableAttempt(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	runtime := &clientOAuthRuntime{status: "disconnected", vault: fixture.vault, failures: map[string]error{"cancel": errors.New("cancel failed")}}
	fixture.console.SetMicrosoftMail(runtime, nil)
	startedResponse := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	started := createdValue(test, strings.NewReader(startedResponse.Body.String()))
	fixture.vault.values[runtime.credential] = `{"partial":"credential"}`
	path := "/v1/connectors/microsoft.mail/connection-attempts/" + started["attempt_id"].(string) + "/cancel"
	if response := fixture.call(http.MethodPost, path, map[string]any{}, token); response.Code != http.StatusBadGateway {
		test.Fatalf("runtime cancellation failure was hidden: %d %s", response.Code, response.Body.String())
	}
	if fixture.vault.values[runtime.credential] == "" {
		test.Fatal("runtime cancellation failure destroyed credential before retry")
	}
	delete(runtime.failures, "cancel")
	cancelled := fixture.value(fixture.call(http.MethodPost, path, map[string]any{}, token))
	if cancelled["status"] != "cancelled" {
		test.Fatalf("runtime cancellation retry failed: %#v", cancelled)
	}
}

func TestOAuthDisconnectCompensatesLogoutFailureWithVaultDelete(test *testing.T) {
	fixture, token, runtime, credential := connectedOAuthFixture(test)
	runtime.failures = map[string]error{"logout": errors.New("logout failed")}
	response := fixture.call(http.MethodDelete, "/v1/connectors/microsoft.mail", nil, token)
	if response.Code != http.StatusOK {
		test.Fatalf("local disconnect did not compensate logout failure: %d %s", response.Code, response.Body.String())
	}
	if _, exists := fixture.vault.values[credential]; exists {
		test.Fatal("logout failure retained local credential")
	}
	fixture.console.mu.Lock()
	_, retained := fixture.console.connectionForPerson("microsoft.mail", fixturePersonID)
	fixture.console.mu.Unlock()
	if retained {
		test.Fatal("logout failure retained disconnected state record")
	}
}

func connectorCatalogItem(catalog map[string]any, connectorID string) map[string]any {
	for _, value := range catalog["connectors"].([]any) {
		item := value.(map[string]any)
		if item["id"] == connectorID {
			return item
		}
	}
	return nil
}

func connectedOAuthFixture(test *testing.T) (*fixture, string, *clientOAuthRuntime, string) {
	test.Helper()
	fixture := setup(test)
	_, token := fixture.pair()
	runtime := &clientOAuthRuntime{status: "disconnected", vault: fixture.vault}
	fixture.console.SetMicrosoftMail(runtime, nil)
	startedResponse := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	started := createdValue(test, strings.NewReader(startedResponse.Body.String()))
	status := fixture.call(http.MethodGet, "/v1/connectors/microsoft.mail/connection-attempts/"+started["attempt_id"].(string), nil, token)
	if status.Code != http.StatusOK {
		test.Fatal(status.Body.String())
	}
	return fixture, token, runtime, runtime.credential
}
