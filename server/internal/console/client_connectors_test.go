package console

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"floe/server/internal/credentials"
)

type clientOAuthRuntime struct {
	status     string
	actions    []string
	credential string
	vault      Vault
	failures   map[string]error
	bindError  error
}

type blockingLogoutRuntime struct {
	*clientOAuthRuntime
	logoutEntered chan struct{}
	releaseLogout chan struct{}
	logoutBoundTo string
}

func (runtime *blockingLogoutRuntime) Action(ctx context.Context, action string) (any, error) {
	if action == "logout" && runtime.logoutEntered != nil {
		runtime.logoutBoundTo = runtime.credential
		close(runtime.logoutEntered)
		select {
		case <-runtime.releaseLogout:
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}
	return runtime.clientOAuthRuntime.Action(ctx, action)
}

type blockingDeleteVault struct {
	mu            sync.Mutex
	values        map[string]string
	deleteEntered chan struct{}
	deleteOnce    sync.Once
	releaseDelete chan struct{}
	putCalled     chan struct{}
	failDelete    bool
}

func (vault *blockingDeleteVault) Get(key string) (string, error) {
	vault.mu.Lock()
	defer vault.mu.Unlock()
	return vault.values[key], nil
}

func (vault *blockingDeleteVault) Put(key, value string) error {
	select {
	case vault.putCalled <- struct{}{}:
	default:
	}
	vault.mu.Lock()
	defer vault.mu.Unlock()
	vault.values[key] = value
	return nil
}

func (vault *blockingDeleteVault) Delete(key string) error {
	vault.deleteOnce.Do(func() { close(vault.deleteEntered) })
	<-vault.releaseDelete
	vault.mu.Lock()
	defer vault.mu.Unlock()
	if vault.failDelete {
		vault.failDelete = false
		return errors.New("private failure detail")
	}
	delete(vault.values, key)
	return nil
}

func TestLastClientRevocationWinsFailedSecretDisconnectRollback(test *testing.T) {
	fixture := setup(test)
	clientID, token := fixture.pair()
	connected := fixture.call(http.MethodPost, "/v1/connectors/github.issues/connect", map[string]any{
		"schema_version": 1,
		"secret":         "github-secret-token",
		"scope":          map[string]any{"owner": "floe", "repository": "server"},
	}, token)
	if connected.Code != http.StatusCreated {
		test.Fatal(connected.Body.String())
	}
	precondition := connectorMutationPrecondition(test, fixture, "github.issues")
	connectionID := precondition["connection_id"].(string)
	fixture.console.mu.Lock()
	credential := fixture.console.state.Connections[connectionID].Credential
	fixture.console.mu.Unlock()
	vault := &blockingDeleteVault{
		values:        fixture.vault.values,
		deleteEntered: make(chan struct{}),
		releaseDelete: make(chan struct{}),
		putCalled:     make(chan struct{}, 1),
		failDelete:    true,
	}
	fixture.console.vault = vault
	disconnected := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		disconnected <- fixture.call(http.MethodDelete, "/v1/connectors/github.issues", precondition, token)
	}()
	<-vault.deleteEntered
	reconnectStarted := make(chan struct{})
	reconnected := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		request := httptest.NewRequest(http.MethodPost, "/v1/connectors/github.issues/connect", strings.NewReader(`{"schema_version":1,"secret":"replacement-github-token","scope":{"owner":"floe","repository":"server"}}`))
		request.Header.Set("Content-Type", "application/json")
		close(reconnectStarted)
		response := httptest.NewRecorder()
		fixture.console.serveClientConnectors(response, request, clientScope{ClientID: clientID, PersonID: fixturePersonID, DeviceID: fixtureDeviceID})
		reconnected <- response
	}()
	<-reconnectStarted

	if response := fixture.call(http.MethodPost, "/manage/api/client/delete", map[string]string{"id": clientID}, ""); response.Code != http.StatusOK {
		test.Fatalf("last client revocation waited for connector cleanup: %d %s", response.Code, response.Body.String())
	}
	if response := fixture.call(http.MethodGet, "/v1/connectors", nil, token); response.Code != http.StatusUnauthorized {
		test.Fatalf("revoked bearer remained active during connector cleanup: %d %s", response.Code, response.Body.String())
	}
	select {
	case response := <-reconnected:
		test.Fatalf("reconnect bypassed the disconnect lifecycle lock: %d %s", response.Code, response.Body.String())
	default:
	}
	close(vault.releaseDelete)
	if response := <-disconnected; response.Code != http.StatusInternalServerError {
		test.Fatalf("vault cleanup failure was hidden: %d %s", response.Code, response.Body.String())
	}
	if response := <-reconnected; response.Code != http.StatusUnauthorized {
		test.Fatalf("authenticated reconnect survived client revocation: %d %s", response.Code, response.Body.String())
	}
	select {
	case <-vault.putCalled:
		test.Fatal("revoked reconnect stored a replacement credential")
	default:
	}

	fixture.console.mu.Lock()
	_, clientRetained := fixture.console.state.Clients[clientID]
	_, connectionRetained := fixture.console.state.Connections[connectionID]
	cleanup := fixture.console.state.Cleanups[fixturePersonID]
	fixture.console.lastPair = time.Time{}
	fixture.console.mu.Unlock()
	if clientRetained || connectionRetained || len(cleanup.Connections) != 1 {
		test.Fatalf("failed disconnect resurrected revoked state: client=%v connection=%v cleanup=%#v", clientRetained, connectionRetained, cleanup)
	}
	step := cleanup.Connections[0]
	if step.ConnectionID != connectionID || step.Credential != credential || !step.RuntimeComplete || step.VaultComplete {
		test.Fatalf("cleanup tombstone lost failed disconnect progress: %#v", step)
	}
	if _, _, err := readState(fixture.console.directory); err != nil {
		test.Fatalf("cleanup invariant was not durable: %v", err)
	}
	if response := fixture.call(http.MethodGet, "/v1/connectors", nil, token); response.Code != http.StatusUnauthorized {
		test.Fatalf("failed disconnect resurrected bearer: %d %s", response.Code, response.Body.String())
	}
	if response := fixture.call(http.MethodPost, "/pair/start", map[string]string{"person_id": otherFixturePersonID, "device_id": "other-device"}, ""); response.Code != http.StatusOK {
		test.Fatalf("cleanup retry did not unblock pairing: %d %s", response.Code, response.Body.String())
	}
	fixture.console.mu.Lock()
	_, cleanupRetained := fixture.console.state.Cleanups[fixturePersonID]
	fixture.console.mu.Unlock()
	if cleanupRetained {
		test.Fatal("successful cleanup retry retained tombstone")
	}
	if value, _ := vault.Get(credential); value != "" {
		test.Fatalf("cleanup retry retained credential: %q", value)
	}
}

func TestConnectorLifecycleHandlersRevalidateAuthenticatedClient(test *testing.T) {
	tests := []struct {
		name   string
		method string
		path   string
		body   string
	}{
		{name: "connect", method: http.MethodPost, path: "/v1/connectors/github.issues/connect", body: `{"schema_version":1,"secret":"github-secret-token","scope":{"owner":"floe","repository":"server"}}`},
		{name: "attempt poll", method: http.MethodGet, path: "/v1/connectors/microsoft.mail/connection-attempts/attempt"},
		{name: "attempt cancel", method: http.MethodPost, path: "/v1/connectors/microsoft.mail/connection-attempts/attempt/cancel", body: `{}`},
		{name: "scope", method: http.MethodPatch, path: "/v1/connectors/github.issues/scope", body: `{"schema_version":1,"connection_id":"00000000-0000-4000-8000-000000000001","connection_revision":1,"scope":{"owner":"floe","repository":"server"}}`},
		{name: "disconnect", method: http.MethodDelete, path: "/v1/connectors/github.issues", body: `{"schema_version":1,"connection_id":"00000000-0000-4000-8000-000000000001","connection_revision":1}`},
	}
	for _, current := range tests {
		test.Run(current.name, func(test *testing.T) {
			fixture := setup(test)
			clientID, _ := fixture.pair()
			scope := clientScope{ClientID: clientID, PersonID: fixturePersonID, DeviceID: fixtureDeviceID}
			fixture.console.mu.Lock()
			delete(fixture.console.state.Clients, clientID)
			fixture.console.mu.Unlock()
			request := httptest.NewRequest(current.method, current.path, strings.NewReader(current.body))
			if current.body != "" {
				request.Header.Set("Content-Type", "application/json")
			}
			response := httptest.NewRecorder()
			fixture.console.serveClientConnectors(response, request, scope)
			if response.Code != http.StatusUnauthorized {
				test.Fatalf("revoked authenticated scope reached %s lifecycle: %d %s", current.name, response.Code, response.Body.String())
			}
			if len(fixture.vault.values) != 0 {
				test.Fatalf("revoked %s lifecycle mutated vault: %#v", current.name, fixture.vault.values)
			}
		})
	}
}

func TestLastClientRevocationSerializesWithOAuthLogout(test *testing.T) {
	fixture := setup(test)
	clientID, token := fixture.pair()
	runtime := &blockingLogoutRuntime{clientOAuthRuntime: &clientOAuthRuntime{status: "disconnected", vault: fixture.vault}}
	fixture.console.SetMicrosoftMail(runtime, nil)
	startedResponse := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	started := createdValue(test, strings.NewReader(startedResponse.Body.String()))
	if response := fixture.call(http.MethodGet, "/v1/connectors/microsoft.mail/connection-attempts/"+started["attempt_id"].(string), nil, token); response.Code != http.StatusOK {
		test.Fatal(response.Body.String())
	}
	precondition := connectorMutationPrecondition(test, fixture, "microsoft.mail")
	runtime.logoutEntered = make(chan struct{})
	runtime.releaseLogout = make(chan struct{})
	disconnected := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		disconnected <- fixture.call(http.MethodDelete, "/v1/connectors/microsoft.mail", precondition, token)
	}()
	<-runtime.logoutEntered

	if response := fixture.call(http.MethodPost, "/manage/api/client/delete", map[string]string{"id": clientID}, ""); response.Code != http.StatusOK {
		test.Fatalf("last client revocation waited for OAuth logout: %d %s", response.Code, response.Body.String())
	}
	if response := fixture.call(http.MethodGet, "/v1/connectors", nil, token); response.Code != http.StatusUnauthorized {
		test.Fatalf("revoked bearer remained active during OAuth logout: %d %s", response.Code, response.Body.String())
	}
	close(runtime.releaseLogout)
	if response := <-disconnected; response.Code != http.StatusOK {
		test.Fatalf("OAuth disconnect failed: %d %s", response.Code, response.Body.String())
	}
	fixture.console.mu.Lock()
	_, cleanupRetained := fixture.console.state.Cleanups[fixturePersonID]
	_, connectionRetained := fixture.console.connectionForPerson("microsoft.mail", fixturePersonID)
	fixture.console.mu.Unlock()
	if cleanupRetained || connectionRetained {
		test.Fatalf("completed OAuth logout retained lifecycle state: cleanup=%v connection=%v", cleanupRetained, connectionRetained)
	}
	if response := fixture.call(http.MethodGet, "/v1/connectors", nil, token); response.Code != http.StatusUnauthorized {
		test.Fatalf("OAuth disconnect resurrected bearer: %d %s", response.Code, response.Body.String())
	}
}

func (runtime *clientOAuthRuntime) BindCredential(name string) error {
	if runtime.bindError != nil {
		return runtime.bindError
	}
	runtime.credential = name
	return nil
}

func (runtime *clientOAuthRuntime) Ready() bool {
	if runtime.vault == nil || runtime.credential == "" {
		return false
	}
	value, err := runtime.vault.Get(runtime.credential)
	return err == nil && value != "" && !strings.Contains(value, `"stale"`)
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

func TestOAuthConnectRequiresCredentialBinding(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	runtime := &clientOAuthRuntime{status: "disconnected", vault: fixture.vault, bindError: errors.New("binding unavailable")}
	fixture.console.SetMicrosoftMail(runtime, nil)

	response := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	if response.Code != http.StatusServiceUnavailable || !strings.Contains(response.Body.String(), "credential_scope_unavailable") {
		test.Fatalf("credential binding failure was hidden: %d %s", response.Code, response.Body.String())
	}
	if len(runtime.actions) != 0 {
		test.Fatalf("OAuth started without credential binding: %#v", runtime.actions)
	}
	fixture.console.mu.Lock()
	attempts := len(fixture.console.connectorAttempts)
	fixture.console.mu.Unlock()
	if attempts != 0 {
		test.Fatal("credential binding failure retained an attempt")
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
	connectionID := started["connection_id"].(string)
	if len(connectionID) != 36 || connectionID[8] != '-' || connectionID[13] != '-' || connectionID[18] != '-' || connectionID[23] != '-' {
		test.Fatalf("connection identity is not a UUID: %q", connectionID)
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

	updated := fixture.value(fixture.call(http.MethodPatch, "/v1/connectors/github.issues/scope", map[string]any{"schema_version": 1, "connection_id": connection.ConnectionID, "connection_revision": connection.Revision, "scope": map[string]any{"owner": "floe", "repository": "server"}}, token))
	if updated["connection_id"] != connection.ConnectionID || updated["connection_revision"] != float64(2) || updated["person_id"] != fixturePersonID || updated["device_id"] != fixtureDeviceID || updated["scope"].(map[string]any)["repository"] != "server" || fixture.vault.values[connection.Credential] != secret {
		test.Fatalf("scope update changed ownership or secret: %#v", updated)
	}
	catalog := fixture.value(fixture.call(http.MethodGet, "/v1/connectors", nil, token))
	item := connectorCatalogItem(catalog, "github.issues")
	if item["connection_id"] != connection.ConnectionID || item["connection_revision"] != float64(2) {
		test.Fatalf("catalog lost authoritative connection identity: %#v", item)
	}
	disconnected := fixture.value(fixture.call(http.MethodDelete, "/v1/connectors/github.issues", map[string]any{"schema_version": 1, "connection_id": connection.ConnectionID, "connection_revision": 2}, token))
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
	fixture.console.state.Connections[connectionID] = connectionRecord{ConnectionID: connectionID, Revision: 1, ConnectorID: "github.issues", PersonID: fixturePersonID}
	fixture.console.mu.Unlock()

	foreign := fixture.call(http.MethodDelete, "/v1/connectors/github.issues", map[string]any{"schema_version": 1, "connection_id": connectionID, "connection_revision": 1}, otherToken)
	if foreign.Code != http.StatusConflict || !strings.Contains(foreign.Body.String(), "connection_changed") {
		test.Fatalf("cross-person mutation not blocked: %d %s", foreign.Code, foreign.Body.String())
	}
}

func TestConnectorMutationsRequireExactCurrentConnection(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	response := fixture.call(http.MethodPost, "/v1/connectors/github.issues/connect", map[string]any{
		"schema_version": 1,
		"secret":         "private-github-pat",
		"scope":          map[string]any{"owner": "floe", "repository": "product"},
	}, token)
	started := createdValue(test, strings.NewReader(response.Body.String()))
	connectionID := started["connection_id"].(string)

	for _, body := range []map[string]any{
		{"schema_version": 1},
		{"schema_version": 1, "connection_id": connectionID, "connection_revision": 2},
		{"schema_version": 1, "connection_id": "00000000-0000-4000-8000-000000000099", "connection_revision": 1},
	} {
		body["scope"] = map[string]any{"owner": "floe", "repository": "server"}
		response := fixture.call(http.MethodPatch, "/v1/connectors/github.issues/scope", body, token)
		if response.Code != http.StatusConflict || !strings.Contains(response.Body.String(), "connection_changed") {
			test.Fatalf("stale PATCH accepted: %#v: %d %s", body, response.Code, response.Body.String())
		}
	}
	response = fixture.call(http.MethodDelete, "/v1/connectors/github.issues", map[string]any{
		"schema_version": 1, "connection_id": connectionID, "connection_revision": 2,
	}, token)
	if response.Code != http.StatusConflict || !strings.Contains(response.Body.String(), "connection_changed") {
		test.Fatalf("stale DELETE accepted: %d %s", response.Code, response.Body.String())
	}
	fixture.console.mu.Lock()
	_, retained := fixture.console.connectionForPerson("github.issues", fixturePersonID)
	fixture.console.mu.Unlock()
	if !retained {
		test.Fatal("stale mutation removed current connection")
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
	connectionID := "00000000-0000-4000-8000-000000000010"
	credential, err := credentials.ConnectionName("FLOE_MICROSOFT_MAIL_OAUTH", connectionID, fixturePersonID)
	if err != nil {
		test.Fatal(err)
	}
	fixture.console.mu.Lock()
	fixture.console.state.Connections[connectionID] = connectionRecord{ConnectionID: connectionID, Revision: 1, ConnectorID: "microsoft.mail", PersonID: fixturePersonID, Scope: map[string]any{}, Credential: credential}
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
	connectionID := "00000000-0000-4000-8000-000000000041"
	credential, _ := credentials.ConnectionName("FLOE_MICROSOFT_MAIL_OAUTH", connectionID, fixturePersonID)
	fixture.vault.values[credential] = `{"stale":"credential"}`
	fixture.console.mu.Lock()
	fixture.console.state.Connections[connectionID] = connectionRecord{ConnectionID: connectionID, Revision: 1, ConnectorID: "microsoft.mail", PersonID: fixturePersonID, Scope: map[string]any{}, Credential: credential}
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
	if _, exists := fixture.vault.values[credential]; exists {
		test.Fatal("stale OAuth credential survived reconnect")
	}
	if runtime.credential == credential {
		test.Fatal("reconnect reused stale credential scope")
	}
}

func TestOAuthReconnectRetriesStaleCredentialCleanup(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	runtime := &clientOAuthRuntime{status: "disconnected", vault: fixture.vault}
	fixture.console.SetMicrosoftMail(runtime, nil)
	connectionID := "00000000-0000-4000-8000-000000000042"
	credential, _ := credentials.ConnectionName("FLOE_MICROSOFT_MAIL_OAUTH", connectionID, fixturePersonID)
	fixture.vault.values[credential] = `{"stale":"credential"}`
	fixture.console.mu.Lock()
	fixture.console.state.Connections[connectionID] = connectionRecord{ConnectionID: connectionID, Revision: 1, ConnectorID: "microsoft.mail", PersonID: fixturePersonID, Scope: map[string]any{}, Credential: credential}
	fixture.console.mu.Unlock()
	fixture.vault.failDeletes = 1

	first := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	if first.Code != http.StatusServiceUnavailable || !strings.Contains(first.Body.String(), "credential_cleanup_failed") {
		test.Fatalf("cleanup failure was hidden: %d %s", first.Code, first.Body.String())
	}
	fixture.console.mu.Lock()
	retained, exists := fixture.console.connectionForPerson("microsoft.mail", fixturePersonID)
	fixture.console.mu.Unlock()
	if !exists || retained.ConnectionID != connectionID || fixture.vault.values[credential] == "" {
		test.Fatal("cleanup failure did not preserve retry state")
	}

	second := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	if second.Code != http.StatusCreated {
		test.Fatalf("cleanup retry blocked reconnect: %d %s", second.Code, second.Body.String())
	}
	if _, exists := fixture.vault.values[credential]; exists {
		test.Fatal("cleanup retry retained stale credential")
	}
}

func TestOAuthDisconnectSaveFailureDoesNotDestroyCredential(test *testing.T) {
	fixture, token, runtime, credential := connectedOAuthFixture(test)
	precondition := connectorMutationPrecondition(test, fixture, "microsoft.mail")
	statePath := filepath.Join(fixture.console.directory, "state.json")
	backupPath := filepath.Join(fixture.console.directory, "state.backup")
	if err := os.Rename(statePath, backupPath); err != nil {
		test.Fatal(err)
	}
	if err := os.Mkdir(statePath, 0700); err != nil {
		test.Fatal(err)
	}
	response := fixture.call(http.MethodDelete, "/v1/connectors/microsoft.mail", precondition, token)
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
	response := fixture.call(http.MethodDelete, "/v1/connectors/microsoft.mail", connectorMutationPrecondition(test, fixture, "microsoft.mail"), token)
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
	response := fixture.call(http.MethodDelete, "/v1/connectors/microsoft.mail", connectorMutationPrecondition(test, fixture, "microsoft.mail"), token)
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

func TestOAuthReconnectWaitsForPriorLogoutLifecycle(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	runtime := &blockingLogoutRuntime{clientOAuthRuntime: &clientOAuthRuntime{status: "disconnected", vault: fixture.vault}}
	fixture.console.SetMicrosoftMail(runtime, nil)
	startedResponse := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	started := createdValue(test, strings.NewReader(startedResponse.Body.String()))
	status := fixture.call(http.MethodGet, "/v1/connectors/microsoft.mail/connection-attempts/"+started["attempt_id"].(string), nil, token)
	if status.Code != http.StatusOK {
		test.Fatal(status.Body.String())
	}
	precondition := connectorMutationPrecondition(test, fixture, "microsoft.mail")
	oldConnectionID := precondition["connection_id"].(string)
	oldCredential := runtime.credential
	runtime.logoutEntered = make(chan struct{})
	runtime.releaseLogout = make(chan struct{})
	disconnected := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		disconnected <- fixture.call(http.MethodDelete, "/v1/connectors/microsoft.mail", precondition, token)
	}()
	<-runtime.logoutEntered

	reconnected := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		reconnected <- fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
	}()
	select {
	case response := <-reconnected:
		test.Fatalf("reconnect completed during prior logout: %d %s", response.Code, response.Body.String())
	case <-time.After(50 * time.Millisecond):
	}
	if runtime.credential != oldCredential || runtime.logoutBoundTo != oldCredential {
		test.Fatalf("singleton runtime rebound during old logout: bound=%q logout=%q old=%q", runtime.credential, runtime.logoutBoundTo, oldCredential)
	}

	close(runtime.releaseLogout)
	if response := <-disconnected; response.Code != http.StatusOK {
		test.Fatalf("disconnect failed: %d %s", response.Code, response.Body.String())
	}
	response := <-reconnected
	if response.Code != http.StatusCreated {
		test.Fatalf("reconnect failed after logout: %d %s", response.Code, response.Body.String())
	}
	created := createdValue(test, strings.NewReader(response.Body.String()))
	if created["connection_id"] == oldConnectionID || runtime.credential == oldCredential {
		test.Fatalf("reconnect reused disconnected identity: %#v credential=%q", created, runtime.credential)
	}
}

func TestSecretReconnectWaitsForFailedVaultCleanupRollback(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	connected := fixture.call(http.MethodPost, "/v1/connectors/github.issues/connect", map[string]any{
		"schema_version": 1,
		"secret":         "github-secret-token",
		"scope":          map[string]any{"owner": "floe", "repository": "server"},
	}, token)
	if connected.Code != http.StatusCreated {
		test.Fatal(connected.Body.String())
	}
	precondition := connectorMutationPrecondition(test, fixture, "github.issues")
	oldConnectionID := precondition["connection_id"].(string)
	fixture.console.mu.Lock()
	oldRecord := fixture.console.state.Connections[oldConnectionID]
	fixture.console.mu.Unlock()
	vault := &blockingDeleteVault{
		values:        fixture.vault.values,
		deleteEntered: make(chan struct{}),
		releaseDelete: make(chan struct{}),
		putCalled:     make(chan struct{}, 1),
		failDelete:    true,
	}
	fixture.console.vault = vault
	disconnected := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		disconnected <- fixture.call(http.MethodDelete, "/v1/connectors/github.issues", precondition, token)
	}()
	<-vault.deleteEntered

	reconnected := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		reconnected <- fixture.call(http.MethodPost, "/v1/connectors/github.issues/connect", map[string]any{
			"schema_version": 1,
			"secret":         "replacement-github-token",
			"scope":          map[string]any{"owner": "floe", "repository": "server"},
		}, token)
	}()
	select {
	case <-vault.putCalled:
		test.Fatal("reconnect stored a new secret during old cleanup")
	case response := <-reconnected:
		test.Fatalf("reconnect completed during old cleanup: %d %s", response.Code, response.Body.String())
	case <-time.After(50 * time.Millisecond):
	}

	close(vault.releaseDelete)
	if response := <-disconnected; response.Code != http.StatusInternalServerError {
		test.Fatalf("vault cleanup failure was hidden: %d %s", response.Code, response.Body.String())
	}
	response := <-reconnected
	if response.Code != http.StatusConflict || !strings.Contains(response.Body.String(), "already_connected") {
		test.Fatalf("rollback did not remain authoritative: %d %s", response.Code, response.Body.String())
	}
	fixture.console.mu.Lock()
	retained, exists := fixture.console.state.Connections[oldConnectionID]
	fixture.console.mu.Unlock()
	if !exists || retained.ConnectionID != oldRecord.ConnectionID || retained.Revision != oldRecord.Revision || retained.Credential != oldRecord.Credential {
		test.Fatalf("failed cleanup rollback replaced optimistic identity: %#v", retained)
	}
	if value, _ := vault.Get(oldRecord.Credential); value != "github-secret-token" {
		test.Fatalf("failed cleanup replaced old secret: %q", value)
	}
}

func connectorMutationPrecondition(test *testing.T, fixture *fixture, connectorID string) map[string]any {
	test.Helper()
	fixture.console.mu.Lock()
	record, exists := fixture.console.connectionForPerson(connectorID, fixturePersonID)
	fixture.console.mu.Unlock()
	if !exists {
		test.Fatalf("missing %s connection", connectorID)
	}
	return map[string]any{"schema_version": 1, "connection_id": record.ConnectionID, "connection_revision": record.Revision}
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
