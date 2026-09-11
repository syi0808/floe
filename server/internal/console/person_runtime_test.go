package console

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"floe/server/internal/connectors/common"
	"floe/server/internal/credentials"
)

const otherFixturePersonID = "00000000-0000-4000-8000-000000000002"

type partialGmailCleanupRuntime struct {
	credential string
	vault      Vault
	logouts    int
}

func (runtime *partialGmailCleanupRuntime) BindCredential(credential string) error {
	runtime.credential = credential
	return nil
}

func (*partialGmailCleanupRuntime) Ready() bool { return false }

func (runtime *partialGmailCleanupRuntime) Action(_ context.Context, action string) (any, error) {
	if action != "logout" {
		return nil, errors.New("unsupported")
	}
	runtime.logouts++
	if err := runtime.vault.Delete(runtime.credential); err != nil {
		return nil, err
	}
	if runtime.logouts == 1 {
		return nil, errors.New("gmail index reset failed after credential deletion")
	}
	return map[string]any{"status": "disconnected"}, nil
}

func (*partialGmailCleanupRuntime) ConnectionSnapshot() (any, error) { return nil, nil }
func (*partialGmailCleanupRuntime) ReadCommunicationView(string, int, int) (any, error) {
	return nil, nil
}
func (*partialGmailCleanupRuntime) ReadLogisticsView(context.Context) (common.LogisticsView, error) {
	return common.LogisticsView{}, nil
}

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

func TestLastClientCleanupFailurePersistsTombstoneAndRetriesBeforePairing(test *testing.T) {
	tests := []struct {
		name          string
		runtimeError  bool
		vaultFailures int
	}{
		{name: "runtime logout and Gmail index reset", runtimeError: true},
		{name: "credential deletion", vaultFailures: 3},
	}
	for _, current := range tests {
		test.Run(current.name, func(test *testing.T) {
			fixture := setup(test)
			clientID, token := fixture.pair()
			runtime := &clientOAuthRuntime{status: "disconnected", vault: fixture.vault, failures: map[string]error{}}
			fixture.console.SetMicrosoftMail(runtime, nil)
			startedResponse := fixture.call(http.MethodPost, "/v1/connectors/microsoft.mail/connect", map[string]any{"schema_version": 1, "scope": map[string]any{}}, token)
			started := createdValue(test, strings.NewReader(startedResponse.Body.String()))
			if response := fixture.call(http.MethodGet, "/v1/connectors/microsoft.mail/connection-attempts/"+started["attempt_id"].(string), nil, token); response.Code != http.StatusOK {
				test.Fatal(response.Body.String())
			}
			if current.runtimeError {
				runtime.failures["logout"] = errors.New("index reset failed")
			}
			fixture.vault.failDeletes = current.vaultFailures
			response := fixture.call(http.MethodPost, "/manage/api/client/delete", map[string]string{"id": clientID}, "")
			if response.Code != http.StatusInternalServerError {
				test.Fatalf("cleanup failure was hidden: %d %s", response.Code, response.Body.String())
			}
			fixture.console.mu.Lock()
			cleanup, pending := fixture.console.state.Cleanups[fixturePersonID]
			_, clientRetained := fixture.console.state.Clients[clientID]
			fixture.console.lastPair = time.Time{}
			fixture.console.mu.Unlock()
			if !pending || len(cleanup.Connections) != 1 || clientRetained {
				test.Fatalf("cleanup tombstone was not committed: %#v", cleanup)
			}
			persisted, _, err := readState(fixture.console.directory)
			if err != nil || len(persisted.Cleanups[fixturePersonID].Connections) != 1 {
				test.Fatalf("cleanup tombstone was not durable: %#v %v", persisted.Cleanups, err)
			}
			restarted, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil)
			if err != nil {
				test.Fatalf("server could not reload cleanup tombstone: %v", err)
			}
			runtime.credential = "wrong-credential"
			restarted.SetMicrosoftMail(runtime, nil)
			restartedFixture := *fixture
			restartedFixture.console = restarted
			restartedFixture.cookie = nil
			restartedFixture.csrf = ""

			blocked := restartedFixture.call(http.MethodPost, "/pair/start", map[string]string{"person_id": otherFixturePersonID, "device_id": "other-device"}, "")
			if blocked.Code != http.StatusServiceUnavailable {
				test.Fatalf("pairing did not fail closed: %d %s", blocked.Code, blocked.Body.String())
			}
			if runtime.credential != cleanup.Connections[0].Credential {
				test.Fatalf("restart cleanup targeted %q instead of tombstoned credential", runtime.credential)
			}
			delete(runtime.failures, "logout")
			fixture.vault.failDeletes = 0
			if response := restartedFixture.call(http.MethodPost, "/pair/start", map[string]string{"person_id": otherFixturePersonID, "device_id": "other-device"}, ""); response.Code != http.StatusOK {
				test.Fatalf("cleanup retry did not unblock pairing: %d %s", response.Code, response.Body.String())
			}
			restarted.mu.Lock()
			_, pending = restarted.state.Cleanups[fixturePersonID]
			restarted.mu.Unlock()
			if pending {
				test.Fatal("successful retry retained cleanup tombstone")
			}
		})
	}
}

func TestPersonDataAccessFailsClosedWhileCleanupIsPending(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	fixture.console.mu.Lock()
	fixture.console.state.Cleanups[fixturePersonID] = personCleanup{
		PersonID: fixturePersonID,
		Connections: []connectionCleanupStep{{
			ConnectionID: "00000000-0000-4000-8000-000000000010", ConnectorID: "microsoft.mail", Credential: "pending", VaultComplete: true,
		}},
	}
	fixture.console.mu.Unlock()
	response := fixture.call(http.MethodGet, "/v1/connectors", nil, token)
	if response.Code != http.StatusServiceUnavailable || !strings.Contains(response.Body.String(), "person_cleanup_pending") {
		test.Fatalf("data access did not fail closed: %d %s", response.Code, response.Body.String())
	}
}

func TestGmailIndexResetFailureRetriesAfterCredentialWasDeleted(test *testing.T) {
	fixture := setup(test)
	connectionID := "00000000-0000-4000-8000-000000000011"
	credential, err := credentials.ConnectionName("FLOE_GMAIL_OAUTH", connectionID, fixturePersonID)
	if err != nil {
		test.Fatal(err)
	}
	fixture.vault.values[credential] = `{"refresh_token":"private"}`
	runtime := &partialGmailCleanupRuntime{vault: fixture.vault}
	fixture.console.mu.Lock()
	fixture.console.gmail = runtime
	fixture.console.state.Cleanups[fixturePersonID] = personCleanup{
		PersonID: fixturePersonID,
		Connections: []connectionCleanupStep{{
			ConnectionID: connectionID, ConnectorID: "gmail", Credential: credential,
		}},
	}
	if err := fixture.console.save(fixture.console.state); err != nil {
		fixture.console.mu.Unlock()
		test.Fatal(err)
	}
	firstErr := fixture.console.retryPersonCleanupLocked(fixturePersonID)
	cleanup := fixture.console.state.Cleanups[fixturePersonID]
	fixture.console.mu.Unlock()
	if firstErr == nil || runtime.logouts != 1 || len(cleanup.Connections) != 1 || cleanup.Connections[0].RuntimeComplete || !cleanup.Connections[0].VaultComplete {
		test.Fatalf("partial Gmail cleanup was not retryable: %#v %v", cleanup, firstErr)
	}
	if _, retained := fixture.vault.values[credential]; retained {
		test.Fatal("partial Gmail logout retained credential")
	}

	fixture.console.mu.Lock()
	secondErr := fixture.console.retryPersonCleanupLocked(fixturePersonID)
	_, pending := fixture.console.state.Cleanups[fixturePersonID]
	fixture.console.mu.Unlock()
	if secondErr != nil || pending || runtime.logouts != 2 || runtime.credential != credential {
		test.Fatalf("Gmail cleanup retry failed: pending=%v logouts=%d credential=%q err=%v", pending, runtime.logouts, runtime.credential, secondErr)
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

func TestPersistedCleanupCannotOverlapLivePersonData(test *testing.T) {
	fixture := setup(test)
	fixture.pair()
	connectionID := "00000000-0000-4000-8000-000000000012"
	credential, err := credentials.ConnectionName("FLOE_MICROSOFT_MAIL_OAUTH", connectionID, fixturePersonID)
	if err != nil {
		test.Fatal(err)
	}
	fixture.console.mu.Lock()
	fixture.console.state.Cleanups[fixturePersonID] = personCleanup{
		PersonID: fixturePersonID,
		Connections: []connectionCleanupStep{{
			ConnectionID: connectionID, ConnectorID: "microsoft.mail", Credential: credential,
		}},
	}
	if err := fixture.console.save(fixture.console.state); err != nil {
		fixture.console.mu.Unlock()
		test.Fatal(err)
	}
	fixture.console.mu.Unlock()
	if _, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil); err == nil {
		test.Fatal("persisted cleanup overlapped live Person state")
	}
}

func TestPersistedCleanupCredentialMustMatchPersonAndConnection(test *testing.T) {
	fixture := setup(test)
	connectionID := "00000000-0000-4000-8000-000000000013"
	wrongCredential, err := credentials.ConnectionName("FLOE_MICROSOFT_MAIL_OAUTH", connectionID, otherFixturePersonID)
	if err != nil {
		test.Fatal(err)
	}
	fixture.console.mu.Lock()
	fixture.console.state.Cleanups[fixturePersonID] = personCleanup{
		PersonID: fixturePersonID,
		Connections: []connectionCleanupStep{{
			ConnectionID: connectionID, ConnectorID: "microsoft.mail", Credential: wrongCredential,
		}},
	}
	if err := fixture.console.save(fixture.console.state); err != nil {
		fixture.console.mu.Unlock()
		test.Fatal(err)
	}
	fixture.console.mu.Unlock()
	if _, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil); err == nil {
		test.Fatal("persisted cleanup accepted another Person credential")
	}
}

func TestPersistedCredentialMustMatchPersonAndConnection(test *testing.T) {
	fixture := setup(test)
	fixture.pair()
	connectionID := "00000000-0000-4000-8000-000000000030"
	wrongCredential, err := credentials.ConnectionName(githubTokenKey, "00000000-0000-4000-8000-000000000031", fixturePersonID)
	if err != nil {
		test.Fatal(err)
	}
	fixture.console.mu.Lock()
	fixture.console.state.Connections[connectionID] = connectionRecord{
		ConnectionID: connectionID, ConnectorID: "github.issues", PersonID: fixturePersonID,
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
