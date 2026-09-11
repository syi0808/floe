package console

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"floe/server/internal/connectors/common"
)

const fixturePersonID = "00000000-0000-4000-8000-000000000001"
const fixtureDeviceID = "fixture-device"

type memoryVault struct {
	values map[string]string
	fail   bool
}

type fakeAuthRuntime struct{ ready bool }

type fakeDriveAuth struct{ token string }

type fakeCalendarAuth struct{ token string }

func (runtime *fakeCalendarAuth) Token(context.Context) (string, error) { return runtime.token, nil }
func (*fakeCalendarAuth) Action(context.Context, string) (any, error) {
	return map[string]any{"status": "connected", "scope": "https://www.googleapis.com/auth/calendar.readonly"}, nil
}

type fakeMicrosoftCalendarAuth struct{ token string }

func (runtime *fakeMicrosoftCalendarAuth) Token(context.Context) (string, error) {
	return runtime.token, nil
}

type fakeMicrosoftTeamsAuth struct{ token string }

func (runtime *fakeMicrosoftTeamsAuth) Token(context.Context) (string, error) {
	return runtime.token, nil
}
func (*fakeMicrosoftTeamsAuth) Action(context.Context, string) (any, error) {
	return map[string]any{"status": "connected", "scope": "ChannelMessage.Read.All"}, nil
}
func (*fakeMicrosoftCalendarAuth) Action(context.Context, string) (any, error) {
	return map[string]any{"status": "connected", "scope": "Calendars.Read"}, nil
}

type fakeMicrosoftAuth struct{}

func (*fakeMicrosoftAuth) Action(context.Context, string) (any, error) {
	return map[string]any{"status": "connected", "scope": "Mail.Read"}, nil
}

type fakeCommunicationRuntime struct {
	snapshot any
	view     any
	err      error
	reads    atomic.Int32
}

type fakeCalendarRuntime struct {
	snapshot any
	view     any
	err      error
}

func (runtime *fakeCalendarRuntime) ConnectionSnapshot(context.Context) (any, error) {
	return runtime.snapshot, runtime.err
}

func (runtime *fakeCalendarRuntime) ReadCalendarView(context.Context, time.Time, time.Time, string, int) (any, error) {
	return runtime.view, runtime.err
}

func (runtime *fakeCommunicationRuntime) ConnectionSnapshot(context.Context) (any, error) {
	return runtime.snapshot, runtime.err
}

func (runtime *fakeCommunicationRuntime) ReadCommunicationView(context.Context, string, int, int) (any, error) {
	runtime.reads.Add(1)
	return runtime.view, runtime.err
}

func (runtime *fakeDriveAuth) Token(context.Context) (string, error) { return runtime.token, nil }
func (*fakeDriveAuth) Action(context.Context, string) (any, error) {
	return map[string]any{"status": "connected", "scope": "https://www.googleapis.com/auth/drive.readonly"}, nil
}

type fakeConnectorRuntime struct {
	snapshot      any
	view          any
	logisticsView any
	err           error
}

type fakeContextRuntime struct {
	snapshot any
	view     any
	err      error
}

func (*fakeConnectorRuntime) Action(context.Context, string) (any, error) {
	return map[string]any{"status": "connected"}, nil
}

func (runtime *fakeConnectorRuntime) ConnectionSnapshot() (any, error) {
	return runtime.snapshot, runtime.err
}

func (runtime *fakeConnectorRuntime) ReadCommunicationView(string, int, int) (any, error) {
	return runtime.view, runtime.err
}

func (runtime *fakeConnectorRuntime) ReadLogisticsView(context.Context) (common.LogisticsView, error) {
	if runtime.err != nil {
		return common.LogisticsView{}, runtime.err
	}
	encoded, _ := json.Marshal(runtime.logisticsView)
	var view common.LogisticsView
	_ = json.Unmarshal(encoded, &view)
	return view, nil
}

func (runtime *fakeContextRuntime) ConnectionSnapshot(context.Context) (any, error) {
	return runtime.snapshot, runtime.err
}

func (runtime *fakeContextRuntime) ReadWorkContextView(context.Context) (common.WorkContextView, error) {
	if runtime.err != nil {
		return common.WorkContextView{}, runtime.err
	}
	encoded, _ := json.Marshal(runtime.view)
	var view common.WorkContextView
	_ = json.Unmarshal(encoded, &view)
	return view, nil
}

func (runtime *fakeContextRuntime) ReadLogisticsView(context.Context) (common.LogisticsView, error) {
	if runtime.err != nil {
		return common.LogisticsView{}, runtime.err
	}
	encoded, _ := json.Marshal(runtime.view)
	var view common.LogisticsView
	_ = json.Unmarshal(encoded, &view)
	return view, nil
}

type blockingAuthRuntime struct {
	started chan struct{}
	release chan struct{}
}

func (runtime *blockingAuthRuntime) Action(context.Context, string) (any, error) {
	return nil, errors.New("unavailable")
}

func (runtime *blockingAuthRuntime) Ready() bool {
	close(runtime.started)
	<-runtime.release
	return true
}

func (*blockingAuthRuntime) ReplayIdentity() string { return "fixture-account" }

func (*blockingAuthRuntime) Generate(context.Context, string, string, string, json.RawMessage, json.RawMessage) (string, error) {
	return `{"ok":true}`, nil
}

func (runtime *fakeAuthRuntime) Action(context.Context, string) (any, error) {
	return map[string]any{"status": "connected", "inference_enabled": runtime.ready}, nil
}

func (runtime *fakeAuthRuntime) Ready() bool { return runtime.ready }

func (runtime *fakeAuthRuntime) ReplayIdentity() string { return "fixture-account" }

func (*fakeAuthRuntime) Generate(_ context.Context, _, _, _ string, _ json.RawMessage, schema json.RawMessage) (string, error) {
	if len(schema) == 0 {
		return `{"content":"OK"}`, nil
	}
	return `{"ok":true}`, nil
}

func (vault *memoryVault) Get(key string) (string, error) { return vault.values[key], nil }
func (vault *memoryVault) Put(key, value string) error {
	if vault.fail {
		return errors.New("private failure detail")
	}
	vault.values[key] = value
	return nil
}
func (vault *memoryVault) Delete(key string) error { delete(vault.values, key); return nil }

type fixture struct {
	console *Console
	vault   *memoryVault
	cookie  *http.Cookie
	csrf    string
	test    *testing.T
}

func setup(test *testing.T) *fixture {
	test.Helper()
	vault := &memoryVault{values: map[string]string{}}
	management, err := New(filepath.Join(test.TempDir(), "node"), "127.0.0.1:8431", vault, nil)
	if err != nil {
		test.Fatal(err)
	}
	fixture := &fixture{console: management, vault: vault, test: test}
	secret, _ := os.ReadFile(filepath.Join(management.directory, "admin-token"))
	response := fixture.call("POST", "/manage/api/login", map[string]string{"token": string(secret)}, "")
	if response.Code != 200 {
		test.Fatal(response.Body.String())
	}
	fixture.cookie = response.Result().Cookies()[0]
	state := fixture.value(fixture.call("GET", "/manage/api/state", nil, ""))
	fixture.csrf = state["csrf"].(string)
	return fixture
}

func TestBlockedCredentialStatusDoesNotBlockInferenceAuthentication(test *testing.T) {
	runtime := &blockingAuthRuntime{started: make(chan struct{}), release: make(chan struct{})}
	management, err := New(filepath.Join(test.TempDir(), "node"), "127.0.0.1:8431", &memoryVault{values: map[string]string{}}, runtime)
	if err != nil {
		test.Fatal(err)
	}
	management.mu.Lock()
	management.state.Providers["codex_oauth"] = providerProfile{BaseURL: codexEndpoint, Classes: map[string]classProfile{"balanced": {Model: "fixture"}}}
	management.state.Clients["fixture"] = pairedClient{TokenHash: digest("app-token"), PersonID: fixturePersonID, DeviceID: fixtureDeviceID}
	management.mu.Unlock()
	stateDone := make(chan struct{})
	go func() {
		management.writeState(httptest.NewRecorder(), session{csrf: "csrf"})
		close(stateDone)
	}()
	select {
	case <-runtime.started:
	case <-time.After(time.Second):
		test.Fatal("credential status was not checked")
	}
	request := httptest.NewRequest(http.MethodGet, "/v1/inference-purposes", nil)
	request.Host = "127.0.0.1:8431"
	request.Header.Set("Authorization", "Bearer app-token")
	response := httptest.NewRecorder()
	done := make(chan struct{})
	go func() {
		management.ServeHTTP(response, request)
		close(done)
	}()
	select {
	case <-done:
		if response.Code != http.StatusOK {
			test.Fatal(response.Body.String())
		}
	case <-time.After(time.Second):
		test.Fatal("credential status held the console mutex")
	}
	close(runtime.release)
	<-stateDone
}

func (fixture *fixture) call(method, path string, body any, token string) *httptest.ResponseRecorder {
	encoded, _ := json.Marshal(body)
	request := httptest.NewRequest(method, "http://127.0.0.1:8431"+path, bytes.NewReader(encoded))
	request.Header.Set("Content-Type", "application/json; charset=utf-8")
	if strings.HasPrefix(path, "/manage/") && method == "POST" {
		request.Header.Set("Origin", "http://127.0.0.1:8431")
	}
	if fixture.cookie != nil {
		request.AddCookie(fixture.cookie)
	}
	request.Header.Set("X-Floe-CSRF", fixture.csrf)
	if token != "" {
		request.Header.Set("Authorization", "Bearer "+token)
	}
	response := httptest.NewRecorder()
	fixture.console.ServeHTTP(response, request)
	return response
}

func (fixture *fixture) value(response *httptest.ResponseRecorder) map[string]any {
	fixture.test.Helper()
	if response.Code != 200 {
		fixture.test.Fatalf("status %d: %s", response.Code, response.Body.String())
	}
	var value map[string]any
	if json.Unmarshal(response.Body.Bytes(), &value) != nil {
		fixture.test.Fatal("invalid JSON")
	}
	return value
}

func (fixture *fixture) pair() (string, string) {
	started := fixture.value(fixture.call("POST", "/pair/start", map[string]string{"person_id": fixturePersonID, "device_id": fixtureDeviceID}, ""))
	proof := started["proof"].(string)
	pending := fixture.value(fixture.call("POST", "/pair/poll", map[string]string{"proof": proof}, ""))
	if pending["status"] != "pending" || pending["token"] != nil {
		fixture.test.Fatal("unapproved credential issued")
	}
	fixture.value(fixture.call("POST", "/manage/api/pair/approve", map[string]any{"id": started["id"]}, ""))
	approved := fixture.value(fixture.call("POST", "/pair/poll", map[string]string{"proof": proof}, ""))
	if approved["person_id"] != fixturePersonID || approved["device_id"] != fixtureDeviceID {
		fixture.test.Fatalf("pairing lost identity scope: %#v", approved)
	}
	return approved["client_id"].(string), approved["token"].(string)
}

func TestPairingRestartAndRevocation(test *testing.T) {
	fixture := setup(test)
	identifier, token := fixture.pair()
	fixture.value(fixture.call("GET", "/v1/inference-purposes", nil, token))
	disk, _ := os.ReadFile(filepath.Join(fixture.console.directory, "state.json"))
	if strings.Contains(string(disk), token) {
		test.Fatal("plaintext app token persisted")
	}
	management, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil)
	if err != nil {
		test.Fatal(err)
	}
	old := fixture.console
	fixture.console = management
	fixture.value(fixture.call("GET", "/v1/inference-purposes", nil, token))
	if fixture.call("GET", "/manage/api/state", nil, "").Code != 401 {
		test.Fatal("management session survived restart")
	}
	fixture.console = old
	fixture.value(fixture.call("POST", "/manage/api/client/delete", map[string]string{"id": identifier}, ""))
	if fixture.call("GET", "/v1/inference-purposes", nil, token).Code != 401 {
		test.Fatal("revoked token accepted")
	}
}

func TestLegacyPairedCredentialMigratesAsUnscopedReadOnly(test *testing.T) {
	directory := filepath.Join(test.TempDir(), "node")
	if err := os.MkdirAll(directory, 0700); err != nil {
		test.Fatal(err)
	}
	token := "legacy-token"
	state := fmt.Sprintf(`{"targets":{},"routes":{},"clients":{"legacy":%q}}`, digest(token))
	if err := os.WriteFile(filepath.Join(directory, "state.json"), []byte(state), 0600); err != nil {
		test.Fatal(err)
	}
	management, err := New(directory, "127.0.0.1:8431", &memoryVault{values: map[string]string{}}, nil)
	if err != nil {
		test.Fatal(err)
	}
	request := httptest.NewRequest(http.MethodGet, "http://127.0.0.1:8431/v1/connections", nil)
	request.Host = "127.0.0.1:8431"
	request.Header.Set("Authorization", "Bearer "+token)
	response := httptest.NewRecorder()
	management.ServeHTTP(response, request)
	if response.Code != http.StatusOK || !strings.Contains(response.Body.String(), `"legacy_unscoped":true`) {
		test.Fatalf("legacy read compatibility lost: %d %s", response.Code, response.Body.String())
	}
	mutation := httptest.NewRequest(http.MethodPost, "http://127.0.0.1:8431/v1/connectors/calendar.google/connect", strings.NewReader(`{}`))
	mutation.Host = "127.0.0.1:8431"
	mutation.Header.Set("Authorization", "Bearer "+token)
	mutationResponse := httptest.NewRecorder()
	management.ServeHTTP(mutationResponse, mutation)
	if mutationResponse.Code != http.StatusForbidden || !strings.Contains(mutationResponse.Body.String(), "person_scope_required") {
		test.Fatalf("legacy mutation was not gated: %d %s", mutationResponse.Code, mutationResponse.Body.String())
	}
	stored, err := json.Marshal(management.state.Clients["legacy"])
	if err != nil || !strings.Contains(string(stored), `"legacy_unscoped":true`) || strings.Contains(string(stored), token) {
		test.Fatalf("unsafe legacy migration: %s %v", stored, err)
	}
}

func TestConnectionOwnershipAndDeviceBindingPersist(test *testing.T) {
	fixture := setup(test)
	fixture.console.mu.Lock()
	next := cloneState(fixture.console.state)
	next.Connections["calendar.apple.primary"] = connectionRecord{
		ConnectionID: "calendar.apple.primary",
		ConnectorID:  "calendar.apple",
		PersonID:     fixturePersonID,
		Device:       &deviceBinding{DeviceID: fixtureDeviceID},
	}
	err := fixture.console.save(next)
	fixture.console.mu.Unlock()
	if err != nil {
		test.Fatal(err)
	}
	state, _, err := readState(fixture.console.directory)
	if err != nil {
		test.Fatal(err)
	}
	connection := state.Connections["calendar.apple.primary"]
	if connection.PersonID != fixturePersonID || connection.Device == nil || connection.Device.DeviceID != fixtureDeviceID {
		test.Fatalf("connection ownership lost: %#v", connection)
	}

	encoded, _ := json.Marshal(state)
	var raw map[string]any
	_ = json.Unmarshal(encoded, &raw)
	if strings.Contains(string(encoded), "credential") || raw["connections"] == nil {
		test.Fatalf("invalid connection persistence boundary: %s", encoded)
	}
}

func TestManagementAndInferenceAuthAreSeparate(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	for _, sample := range []struct {
		path, method, origin, host, csrf string
		cookie                           bool
		token                            string
	}{
		{"/manage/api/state", "GET", "", "127.0.0.1:8431", "", false, token},
		{"/manage/api/target/delete", "POST", "http://127.0.0.1:8431", "127.0.0.1:8431", "wrong", true, ""},
		{"/manage/api/target/delete", "POST", "https://evil.example", "127.0.0.1:8431", fixture.csrf, true, ""},
		{"/manage/api/state", "GET", "", "evil.example:8431", "", true, ""},
		{"/v1/inference-purposes", "GET", "http://127.0.0.1:8431", "127.0.0.1:8431", "", true, token},
		{"/v1/inference-purposes", "GET", "", "127.0.0.1:8431", "", true, ""},
		{"/pair/start", "POST", "http://127.0.0.1:8431", "127.0.0.1:8431", "", true, ""},
	} {
		request := httptest.NewRequest(sample.method, "http://"+sample.host+sample.path, strings.NewReader(`{"id":"missing"}`))
		request.Header.Set("Content-Type", "application/json")
		request.Header.Set("Origin", sample.origin)
		request.Header.Set("X-Floe-CSRF", sample.csrf)
		request.Header.Set("Authorization", "Bearer "+sample.token)
		if sample.cookie {
			request.AddCookie(fixture.cookie)
		}
		response := httptest.NewRecorder()
		fixture.console.ServeHTTP(response, request)
		if response.Code != 401 && response.Code != 403 {
			test.Fatalf("unsafe access: %s %d", sample.path, response.Code)
		}
	}
	if !fixture.cookie.HttpOnly || fixture.cookie.SameSite != http.SameSiteStrictMode || fixture.cookie.Path != "/manage" {
		test.Fatal("unsafe management cookie")
	}
}

func TestGmailOAuthActionsRequireManagementSessionAndConfiguredRuntime(test *testing.T) {
	fixture := setup(test)
	if response := fixture.call("POST", "/manage/api/gmail/status", map[string]any{}, ""); response.Code != http.StatusServiceUnavailable {
		test.Fatalf("unconfigured status: %d %s", response.Code, response.Body.String())
	}
	fixture.console.SetGmailAuth(&fakeConnectorRuntime{})
	value := fixture.value(fixture.call("POST", "/manage/api/gmail/status", map[string]any{}, ""))
	if value["status"] != "connected" {
		test.Fatalf("status: %#v", value)
	}
	request := httptest.NewRequest(http.MethodPost, "http://127.0.0.1:8431/manage/api/gmail/logout", strings.NewReader(`{}`))
	request.Host = "127.0.0.1:8431"
	request.Header.Set("Origin", "http://127.0.0.1:8431")
	request.Header.Set("Content-Type", "application/json")
	response := httptest.NewRecorder()
	fixture.console.ServeHTTP(response, request)
	if response.Code != http.StatusUnauthorized {
		test.Fatalf("unauthorized action: %d", response.Code)
	}
}

func TestMicrosoftMailOAuthActionsRequireConfiguredRuntime(test *testing.T) {
	fixture := setup(test)
	if response := fixture.call("POST", "/manage/api/microsoft-mail/status", map[string]any{}, ""); response.Code != http.StatusServiceUnavailable {
		test.Fatalf("unconfigured status: %d %s", response.Code, response.Body.String())
	}
	fixture.console.SetMicrosoftMail(&fakeMicrosoftAuth{}, &fakeCommunicationRuntime{})
	value := fixture.value(fixture.call("POST", "/manage/api/microsoft-mail/status", map[string]any{}, ""))
	if value["status"] != "connected" || value["scope"] != "Mail.Read" {
		test.Fatalf("status: %#v", value)
	}
}

func TestPairedClientReadsConnectorSnapshots(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	snapshot := map[string]any{
		"descriptor": map[string]any{"id": "gmail.fixture", "provider": "gmail"},
		"connection": map[string]any{"connector_id": "gmail.fixture", "state": "ready"},
		"views":      []any{},
	}
	fixture.console.SetGmailAuth(&fakeConnectorRuntime{snapshot: snapshot})

	value := fixture.value(fixture.call(http.MethodGet, "/v1/connections", nil, token))
	if value["schema_version"] != float64(1) {
		test.Fatalf("schema: %#v", value)
	}
	connections := value["connections"].([]any)
	if len(connections) != 1 || connections[0].(map[string]any)["descriptor"].(map[string]any)["provider"] != "gmail" {
		test.Fatalf("connections: %#v", connections)
	}
	connection := connections[0].(map[string]any)["connection"].(map[string]any)
	if value["person_id"] != fixturePersonID || value["device_id"] != fixtureDeviceID || connection["person_id"] != fixturePersonID || !strings.HasPrefix(connection["connection_id"].(string), "gmail.fixture.") {
		test.Fatalf("unbound connection: %#v", value)
	}
	if response := fixture.call(http.MethodPost, "/v1/connections", map[string]any{}, token); response.Code != http.StatusNotFound {
		test.Fatalf("write endpoint accepted: %d", response.Code)
	}
	if response := fixture.call(http.MethodGet, "/v1/connections", nil, ""); response.Code != http.StatusUnauthorized {
		test.Fatalf("unpaired read accepted: %d", response.Code)
	}
}

func TestPairingRejectsMissingAndDifferentPersonIdentity(test *testing.T) {
	fixture := setup(test)
	if response := fixture.call("POST", "/pair/start", map[string]string{}, ""); response.Code != http.StatusBadRequest {
		test.Fatalf("missing identity accepted: %d", response.Code)
	}
	fixture.pair()
	restarted, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil)
	if err != nil {
		test.Fatal(err)
	}
	fixture.console = restarted
	response := fixture.call("POST", "/pair/start", map[string]string{
		"person_id": "00000000-0000-4000-8000-000000000002",
		"device_id": "other-device",
	}, "")
	if response.Code != http.StatusConflict || !strings.Contains(response.Body.String(), "person_mismatch") {
		test.Fatalf("different person accepted: %d %s", response.Code, response.Body.String())
	}
}

func TestConnectorSnapshotFailureIsRedacted(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	fixture.console.SetGmailAuth(&fakeConnectorRuntime{err: errors.New("private connector failure")})
	response := fixture.call(http.MethodGet, "/v1/connections", nil, token)
	if response.Code != http.StatusServiceUnavailable || strings.Contains(response.Body.String(), "private connector failure") {
		test.Fatalf("unsafe failure: %d %s", response.Code, response.Body.String())
	}
}

func TestPairedClientReadsBoundedCommunicationView(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	view := map[string]any{
		"schema_version": 1,
		"view_id":        "mail.communication",
		"source_handle":  "mail:fixture",
		"items":          []any{},
	}
	fixture.console.SetGmailAuth(&fakeConnectorRuntime{view: view})

	value := fixture.value(fixture.call(http.MethodPost, "/v1/views/mail.communication", map[string]any{
		"schema_version": 1,
		"query":          "follow up",
		"cursor":         0,
		"limit":          25,
	}, token))
	if value["view"].(map[string]any)["view_id"] != "mail.communication" {
		test.Fatalf("view: %#v", value)
	}
	for _, body := range []map[string]any{
		{"schema_version": 2, "query": "", "cursor": 0, "limit": 25},
		{"schema_version": 1, "query": "", "cursor": -1, "limit": 25},
		{"schema_version": 1, "query": "", "cursor": 0, "limit": 101},
		{"schema_version": 1, "query": "", "cursor": 0, "limit": 25, "authority": "send"},
	} {
		if response := fixture.call(http.MethodPost, "/v1/views/mail.communication", body, token); response.Code != http.StatusBadRequest {
			test.Fatalf("invalid view request accepted: %#v", body)
		}
	}
	if response := fixture.call(http.MethodPost, "/v1/views/mail.communication", map[string]any{"schema_version": 1, "query": "", "cursor": 0, "limit": 25}, ""); response.Code != http.StatusUnauthorized {
		test.Fatalf("unpaired view read accepted: %d", response.Code)
	}
}

func TestPairedClientReadsBoundedCalendarView(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	fixture.console.calendars = []CalendarRuntime{&fakeCalendarRuntime{snapshot: map[string]any{"descriptor": map[string]any{"provider": "google_calendar"}}, view: map[string]any{"schema_version": 1, "view_id": "calendar.timeline", "source_handle": "calendar.timeline:fixture", "items": []any{}}}}
	start := time.Date(2026, 9, 11, 0, 0, 0, 0, time.UTC).UnixMilli()
	value := fixture.value(fixture.call(http.MethodPost, "/v1/views/calendar.timeline", map[string]any{"schema_version": 1, "range_start_unix_ms": start, "range_end_unix_ms": start + int64(24*time.Hour/time.Millisecond), "cursor": "", "limit": 25}, token))
	if value["view"].(map[string]any)["view_id"] != "calendar.timeline" {
		test.Fatalf("view: %#v", value)
	}
	if response := fixture.call(http.MethodPost, "/v1/views/calendar.timeline", map[string]any{"schema_version": 1, "range_start_unix_ms": start, "range_end_unix_ms": start, "cursor": "", "limit": 25}, token); response.Code != http.StatusBadRequest {
		test.Fatalf("invalid range accepted: %d", response.Code)
	}
}

func TestCalendarRouteFallsBackToMicrosoftProvider(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	fixture.console.calendars = []CalendarRuntime{
		&fakeCalendarRuntime{err: errors.New("google unavailable")},
		&fakeCalendarRuntime{view: map[string]any{"schema_version": 1, "view_id": "calendar.timeline", "source_handle": "calendar.timeline:microsoft", "items": []any{}}},
	}
	start := time.Date(2026, 9, 11, 0, 0, 0, 0, time.UTC).UnixMilli()
	value := fixture.value(fixture.call(http.MethodPost, "/v1/views/calendar.timeline", map[string]any{"schema_version": 1, "range_start_unix_ms": start, "range_end_unix_ms": start + int64(24*time.Hour/time.Millisecond), "cursor": "", "limit": 25}, token))
	if value["view"].(map[string]any)["source_handle"] != "calendar.timeline:microsoft" {
		test.Fatalf("view: %#v", value)
	}
}

func TestCommunicationRouteFallsBackToMicrosoftMail(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	view := map[string]any{"schema_version": 1, "view_id": "mail.communication", "source_handle": "mail:microsoft", "items": []any{}}
	microsoft := &fakeCommunicationRuntime{snapshot: map[string]any{"descriptor": map[string]any{"provider": "microsoft"}}, view: view}
	fixture.console.SetGmailAuth(&fakeConnectorRuntime{err: errors.New("gmail unavailable")})
	fixture.console.SetMicrosoftMail(&fakeMicrosoftAuth{}, microsoft)

	value := fixture.value(fixture.call(http.MethodPost, "/v1/views/mail.communication", map[string]any{"schema_version": 1, "query": "follow up", "cursor": 0, "limit": 25}, token))
	if value["view"].(map[string]any)["source_handle"] != "mail:microsoft" || microsoft.reads.Load() != 1 {
		test.Fatalf("view: %#v reads=%d", value, microsoft.reads.Load())
	}
	fixture.console.SetGmailAuth(nil)
	connections := fixture.value(fixture.call(http.MethodGet, "/v1/connections", nil, token))["connections"].([]any)
	if len(connections) != 1 || connections[0].(map[string]any)["descriptor"].(map[string]any)["provider"] != "microsoft" {
		test.Fatalf("connections: %#v", connections)
	}
}

func TestPairedClientReadsConfiguredWorkAndLogisticsViews(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	now := time.Now().UnixMilli()
	fixture.console.SetWorkContext(&fakeContextRuntime{snapshot: map[string]any{"descriptor": map[string]any{"provider": "github"}}, view: map[string]any{"schema_version": 1, "view_id": "work.context", "source_handle": "work:fixture", "scope_handle": "workspace:fixture", "observed_at_unix_ms": now - 1, "expires_at_unix_ms": now + 299_999, "coverage_complete": true, "items": []any{}}})
	fixture.console.SetLogistics(&fakeContextRuntime{snapshot: map[string]any{"descriptor": map[string]any{"provider": "home_assistant"}}, view: map[string]any{"schema_version": 1, "view_id": "life.logistics", "source_handle": "home:fixture", "observed_at_unix_ms": now - 1, "expires_at_unix_ms": now + 299_999, "coverage_complete": true, "items": []any{}}})

	for path, viewID := range map[string]string{
		"/v1/views/work.context":   "work.context",
		"/v1/views/life.logistics": "life.logistics",
	} {
		value := fixture.value(fixture.call(http.MethodPost, path, map[string]any{"schema_version": 1}, token))
		if value["view"].(map[string]any)["view_id"] != viewID {
			test.Fatalf("view: %#v", value)
		}
		if response := fixture.call(http.MethodPost, path, map[string]any{"schema_version": 1, "write": true}, token); response.Code != http.StatusBadRequest {
			test.Fatalf("authority field accepted: %s", path)
		}
	}
	connections := fixture.value(fixture.call(http.MethodGet, "/v1/connections", nil, token))["connections"].([]any)
	if len(connections) != 2 {
		test.Fatalf("connections: %#v", connections)
	}
}

func TestWorkContextRouteMergesHealthyProvidersAndToleratesOneFailure(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	now := time.Now().UnixMilli()
	view := func(source, scope, evidence string) map[string]any {
		return map[string]any{"schema_version": 1, "view_id": "work.context", "source_handle": source, "scope_handle": scope, "observed_at_unix_ms": now - 1, "expires_at_unix_ms": now + 299_999, "coverage_complete": true, "items": []any{map[string]any{"evidence_handle": evidence, "kind": "communication", "title": "Selected work", "observed_at_unix_ms": now - 2}}}
	}
	fixture.console.work = []WorkContextRuntime{
		&fakeContextRuntime{view: view("github:a", "workspace:a", "github:item")},
		&fakeContextRuntime{view: view("slack:b", "channel:b", "slack:item")},
	}
	response := fixture.value(fixture.call(http.MethodPost, "/v1/views/work.context", map[string]any{"schema_version": 1}, token))
	merged := response["view"].(map[string]any)
	if len(merged["items"].([]any)) != 2 || !strings.HasPrefix(merged["source_handle"].(string), "work:") {
		test.Fatalf("merged view: %#v", merged)
	}
	fixture.console.work[0] = &fakeContextRuntime{err: errors.New("private provider failure")}
	response = fixture.value(fixture.call(http.MethodPost, "/v1/views/work.context", map[string]any{"schema_version": 1}, token))
	if len(response["view"].(map[string]any)["items"].([]any)) != 1 || strings.Contains(fmt.Sprint(response), "private provider failure") {
		test.Fatalf("partial view: %#v", response)
	}
}

func TestLogisticsRouteMergesMailAndHomeEvidence(test *testing.T) {
	fixture := setup(test)
	_, token := fixture.pair()
	now := time.Now().UnixMilli()
	view := func(source, evidence, kind string) map[string]any {
		return map[string]any{"schema_version": 1, "view_id": "life.logistics", "source_handle": source, "observed_at_unix_ms": now - 1, "expires_at_unix_ms": now + 299_999, "coverage_complete": true, "items": []any{map[string]any{"evidence_handle": evidence, "kind": kind, "summary": "Selected evidence", "status": "observed", "needs_attention": false}}}
	}
	fixture.console.SetGmailAuth(&fakeConnectorRuntime{logisticsView: view("mail:a", "mail:item", "delivery")})
	fixture.console.SetLogistics(&fakeContextRuntime{view: view("home:b", "home:item", "home_state")})
	response := fixture.value(fixture.call(http.MethodPost, "/v1/views/life.logistics", map[string]any{"schema_version": 1}, token))
	merged := response["view"].(map[string]any)
	if len(merged["items"].([]any)) != 2 || !strings.HasPrefix(merged["source_handle"].(string), "logistics:") {
		test.Fatalf("merged logistics: %#v", merged)
	}
}

func TestConnectorConfigurationKeepsTokensInVaultAndRestoresRuntime(test *testing.T) {
	now := time.Now().UTC()
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/api/states/sensor.temperature" || request.Header.Get("Authorization") != "Bearer private-home-token" {
			test.Fatalf("unsafe connector request: %s", request.URL.Path)
		}
		_, _ = writer.Write([]byte(fmt.Sprintf(`{"entity_id":"sensor.temperature","state":"22","last_updated":%q,"attributes":{"friendly_name":"Temperature"}}`, now.Format(time.RFC3339Nano))))
	}))
	defer upstream.Close()
	fixture := setup(test)

	fixture.value(fixture.call(http.MethodPost, "/manage/api/connector/home-assistant", map[string]any{
		"enabled": true, "base_url": upstream.URL, "entities": []string{"sensor.temperature"}, "token": "private-home-token",
	}, ""))
	if fixture.vault.values[homeTokenKey] != "private-home-token" || fixture.console.logistics == nil {
		test.Fatal("connector credential or runtime missing")
	}
	state, _ := os.ReadFile(filepath.Join(fixture.console.directory, "state.json"))
	if strings.Contains(string(state), "private-home-token") || !strings.Contains(string(state), "sensor.temperature") {
		test.Fatal("connector state crossed credential boundary")
	}
	_, token := fixture.pair()
	view := fixture.value(fixture.call(http.MethodPost, "/v1/views/life.logistics", map[string]any{"schema_version": 1}, token))
	if view["view"].(map[string]any)["view_id"] != "life.logistics" {
		test.Fatalf("view: %#v", view)
	}

	restarted, err := New(fixture.console.directory, "127.0.0.1:8431", fixture.vault, nil)
	if err != nil || restarted.logistics == nil {
		test.Fatalf("restart: %v", err)
	}
	fixture.value(fixture.call(http.MethodPost, "/manage/api/connector/home-assistant", map[string]any{
		"enabled": false, "base_url": "", "entities": []string{}, "token": "",
	}, ""))
	if _, exists := fixture.vault.values[homeTokenKey]; exists || fixture.console.logistics != nil {
		test.Fatal("connector credential or runtime survived disconnect")
	}
}

func TestGitHubConnectorConfigurationIsSelectedAndValidated(test *testing.T) {
	fixture := setup(test)
	fixture.value(fixture.call(http.MethodPost, "/manage/api/connector/github", map[string]any{
		"enabled": true, "owner": "acme", "repository": "floe", "token": "private-github-token",
	}, ""))
	if fixture.console.work == nil || fixture.vault.values[githubTokenKey] != "private-github-token" {
		test.Fatal("GitHub connector was not installed")
	}
	response := fixture.call(http.MethodPost, "/manage/api/connector/github", map[string]any{
		"enabled": true, "owner": "../all", "repository": "floe", "token": "replacement-token",
	}, "")
	if response.Code != http.StatusBadRequest || fixture.vault.values[githubTokenKey] != "private-github-token" {
		test.Fatal("invalid scope changed credential")
	}
}

func TestSlackConnectorConfigurationIsSelectedAndValidated(test *testing.T) {
	fixture := setup(test)
	fixture.value(fixture.call(http.MethodPost, "/manage/api/connector/slack", map[string]any{
		"enabled": true, "channel": "C12345678", "thread": "1789127940.123456", "token": "private-slack-token",
	}, ""))
	if len(fixture.console.work) != 1 || fixture.vault.values[slackTokenKey] != "private-slack-token" {
		test.Fatal("Slack connector was not installed")
	}
	state, _ := os.ReadFile(filepath.Join(fixture.console.directory, "state.json"))
	if strings.Contains(string(state), "private-slack-token") {
		test.Fatal("Slack token entered server state")
	}
	response := fixture.call(http.MethodPost, "/manage/api/connector/slack", map[string]any{
		"enabled": true, "channel": "*", "thread": "", "token": "replacement-token",
	}, "")
	if response.Code != http.StatusBadRequest || fixture.vault.values[slackTokenKey] != "private-slack-token" {
		test.Fatal("invalid Slack scope changed credential")
	}
}

func TestGoogleDriveSelectionRequiresDedicatedAuthRuntime(test *testing.T) {
	fixture := setup(test)
	input := map[string]any{"enabled": true, "folder_id": "folder12345"}
	if response := fixture.call(http.MethodPost, "/manage/api/connector/google-drive", input, ""); response.Code != http.StatusServiceUnavailable {
		test.Fatalf("missing Drive auth accepted: %d", response.Code)
	}
	if err := fixture.console.SetDriveAuth(&fakeDriveAuth{token: "private-drive-token"}); err != nil {
		test.Fatal(err)
	}
	fixture.value(fixture.call(http.MethodPost, "/manage/api/connector/google-drive", input, ""))
	if len(fixture.console.work) != 1 || fixture.console.state.Connectors.GoogleDrive.FolderID != "folder12345" {
		test.Fatal("Drive selection was not installed")
	}
	state, _ := os.ReadFile(filepath.Join(fixture.console.directory, "state.json"))
	if strings.Contains(string(state), "private-drive-token") {
		test.Fatal("Drive credential entered server state")
	}
	status := fixture.value(fixture.call(http.MethodPost, "/manage/api/drive/status", map[string]any{}, ""))
	if status["scope"] != "https://www.googleapis.com/auth/drive.readonly" {
		test.Fatalf("status: %#v", status)
	}
}

func TestGoogleCalendarSelectionRequiresDedicatedAuthRuntime(test *testing.T) {
	fixture := setup(test)
	input := map[string]any{"enabled": true, "calendar_id": "team/selected"}
	if response := fixture.call(http.MethodPost, "/manage/api/connector/google-calendar", input, ""); response.Code != http.StatusServiceUnavailable {
		test.Fatalf("missing Calendar auth accepted: %d", response.Code)
	}
	if err := fixture.console.SetCalendarAuth(&fakeCalendarAuth{token: "private-calendar-token"}); err != nil {
		test.Fatal(err)
	}
	fixture.value(fixture.call(http.MethodPost, "/manage/api/connector/google-calendar", input, ""))
	if len(fixture.console.calendars) != 1 || fixture.console.state.Connectors.GoogleCalendar.CalendarID != "team/selected" {
		test.Fatal("Calendar selection was not installed")
	}
	state, _ := os.ReadFile(filepath.Join(fixture.console.directory, "state.json"))
	if strings.Contains(string(state), "private-calendar-token") {
		test.Fatal("Calendar credential entered server state")
	}
	status := fixture.value(fixture.call(http.MethodPost, "/manage/api/calendar/status", map[string]any{}, ""))
	if status["scope"] != "https://www.googleapis.com/auth/calendar.readonly" {
		test.Fatalf("status: %#v", status)
	}
}

func TestMicrosoftCalendarSelectionRequiresDedicatedAuthRuntime(test *testing.T) {
	fixture := setup(test)
	input := map[string]any{"enabled": true, "calendar_id": "team/selected"}
	if response := fixture.call(http.MethodPost, "/manage/api/connector/microsoft-calendar", input, ""); response.Code != http.StatusServiceUnavailable {
		test.Fatalf("missing Microsoft Calendar auth accepted: %d", response.Code)
	}
	if err := fixture.console.SetMicrosoftCalendarAuth(&fakeMicrosoftCalendarAuth{token: "private-calendar-token"}); err != nil {
		test.Fatal(err)
	}
	fixture.value(fixture.call(http.MethodPost, "/manage/api/connector/microsoft-calendar", input, ""))
	if len(fixture.console.calendars) != 1 || fixture.console.state.Connectors.MicrosoftCalendar.CalendarID != "team/selected" {
		test.Fatal("Microsoft Calendar selection was not installed")
	}
	state, _ := os.ReadFile(filepath.Join(fixture.console.directory, "state.json"))
	if strings.Contains(string(state), "private-calendar-token") {
		test.Fatal("Microsoft Calendar credential entered server state")
	}
	status := fixture.value(fixture.call(http.MethodPost, "/manage/api/microsoft-calendar/status", map[string]any{}, ""))
	if status["scope"] != "Calendars.Read" {
		test.Fatalf("status: %#v", status)
	}
}

func TestMicrosoftTeamsSelectionRequiresDedicatedAuthRuntime(test *testing.T) {
	fixture := setup(test)
	input := map[string]any{"enabled": true, "team_id": "2f5d86d0-2527-4c94-8f03-33423b9db904", "channel_id": "19:launch@thread.tacv2"}
	if response := fixture.call(http.MethodPost, "/manage/api/connector/microsoft-teams", input, ""); response.Code != http.StatusServiceUnavailable {
		test.Fatalf("missing Microsoft Teams auth accepted: %d", response.Code)
	}
	if err := fixture.console.SetMicrosoftTeamsAuth(&fakeMicrosoftTeamsAuth{token: "private-teams-token"}); err != nil {
		test.Fatal(err)
	}
	fixture.value(fixture.call(http.MethodPost, "/manage/api/connector/microsoft-teams", input, ""))
	if len(fixture.console.work) != 1 || fixture.console.state.Connectors.MicrosoftTeams.TeamID != input["team_id"] || fixture.console.state.Connectors.MicrosoftTeams.ChannelID != input["channel_id"] {
		test.Fatal("Microsoft Teams selection was not installed")
	}
	state, _ := os.ReadFile(filepath.Join(fixture.console.directory, "state.json"))
	if strings.Contains(string(state), "private-teams-token") {
		test.Fatal("Microsoft Teams credential entered server state")
	}
	status := fixture.value(fixture.call(http.MethodPost, "/manage/api/microsoft-teams/status", map[string]any{}, ""))
	if status["scope"] != "ChannelMessage.Read.All" {
		test.Fatalf("status: %#v", status)
	}
	response := fixture.call(http.MethodPost, "/manage/api/connector/microsoft-teams", map[string]any{"enabled": true, "team_id": "../all", "channel_id": "*"}, "")
	if response.Code != http.StatusBadRequest || fixture.console.state.Connectors.MicrosoftTeams.TeamID != input["team_id"] {
		test.Fatal("invalid Microsoft Teams scope changed selection")
	}
}

func TestTargetCredentialsConsentAndSyntheticTest(test *testing.T) {
	fixture := setup(test)
	var calls atomic.Int32
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		calls.Add(1)
		if request.Header.Get("Authorization") != "Bearer private-provider-key" {
			test.Error("missing provider credential")
		}
		var body map[string]any
		_ = json.NewDecoder(request.Body).Decode(&body)
		if body["messages"].([]any)[1].(map[string]any)["content"] != "Confirm the Agent route." || len(body["tools"].([]any)) != 0 {
			test.Error("test context was not synthetic")
		}
		_, _ = writer.Write([]byte(`{"choices":[{"finish_reason":"stop","message":{"content":"OK"}}]}`))
	}))
	defer upstream.Close()
	input := map[string]string{"id": "fixture", "provider": "openai_compatible", "base_url": upstream.URL, "model": "fixture", "api_key": "private-provider-key"}
	fixture.value(fixture.call("POST", "/manage/api/target", input, ""))
	fixture.value(fixture.call("POST", "/manage/api/route", map[string]string{"inference_class": "high_effort", "target": "fixture", "reasoning_effort": "high"}, ""))
	if calls.Load() != 0 {
		test.Fatal("adding target transmitted a request")
	}
	state := fixture.call("GET", "/manage/api/state", nil, "")
	managementState := fixture.value(state)
	if managementState["targets"] != nil || managementState["routes"] != nil {
		test.Fatal("internal target or route inventory leaked through management state")
	}
	_, appToken := fixture.pair()
	classes := fixture.call("GET", "/v1/inference-purposes", nil, appToken)
	if !strings.Contains(classes.Body.String(), `"deep_work"`) || strings.Contains(classes.Body.String(), "fixture") {
		test.Fatal("app inference inventory exposed server routing")
	}
	purposes := fixture.call("GET", "/v1/inference-purposes", nil, appToken)
	if purposes.Code != 200 || !strings.Contains(purposes.Body.String(), `"everyday_assistance"`) || strings.Contains(purposes.Body.String(), "fixture") {
		test.Fatal("app purpose inventory was not routed or exposed server configuration")
	}
	disk, _ := os.ReadFile(filepath.Join(fixture.console.directory, "state.json"))
	if strings.Contains(state.Body.String()+string(disk), "private-provider-key") || len(fixture.vault.values) != 1 {
		test.Fatal("credential storage boundary violated")
	}
	if fixture.call("POST", "/manage/api/test", map[string]any{"id": "fixture", "allow_external": false}, "").Code != 403 || calls.Load() != 0 {
		test.Fatal("consent did not gate provider call")
	}
	fixture.value(fixture.call("POST", "/manage/api/test", map[string]any{"id": "fixture", "allow_external": true}, ""))
	if calls.Load() != 1 {
		test.Fatal("unexpected request count")
	}
	input["api_key"] = ""
	input["base_url"] = "https://new-provider.example/v1"
	fixture.value(fixture.call("POST", "/manage/api/target", input, ""))
	if fixture.console.state.Targets["fixture"].APIKeyEnv != "" || len(fixture.vault.values) != 0 {
		test.Fatal("credential inherited by changed endpoint")
	}
}

func TestProviderProfilesOwnClassModelsAndReplaceActiveRoutes(test *testing.T) {
	fixture := setup(test)
	apiProfile := map[string]any{
		"provider": "openai_compatible", "base_url": "https://api.example/v1", "api_key": "private-provider-key",
		"classes": map[string]any{
			"fast":        map[string]string{"model": "fast-model", "reasoning_effort": "low"},
			"high_effort": map[string]string{"model": "strong-model", "reasoning_effort": "high"},
		},
	}
	fixture.value(fixture.call("POST", "/manage/api/provider", apiProfile, ""))
	if len(fixture.vault.values) != 1 || len(fixture.console.state.Providers["openai_compatible"].Classes) != 2 {
		test.Fatal("provider credential or class profiles were not stored together")
	}
	state := fixture.call("GET", "/manage/api/state", nil, "")
	if strings.Contains(state.Body.String(), "private-provider-key") || !strings.Contains(state.Body.String(), `"strong-model"`) {
		test.Fatal("management provider view exposed a secret or omitted its model")
	}
	_, appToken := fixture.pair()
	classes := fixture.call("GET", "/v1/inference-purposes", nil, appToken)
	if strings.Contains(classes.Body.String(), "strong-model") || strings.Contains(classes.Body.String(), "openai_compatible") {
		test.Fatal("app class inventory exposed provider configuration")
	}

	fixture.console.runtime = &fakeAuthRuntime{ready: true}
	fixture.value(fixture.call("POST", "/manage/api/provider", map[string]any{
		"provider": "codex_oauth", "base_url": "https://ignored.example", "api_key": "ignored",
		"classes": map[string]any{"high_effort": map[string]string{"model": "codex-model", "reasoning_effort": "xhigh"}},
	}, ""))
	if fixture.console.state.Routes["high_effort"].Target != profileTargetID("codex_oauth", "high_effort") || fixture.console.state.Providers["codex_oauth"].BaseURL != codexEndpoint {
		test.Fatal("saving a provider did not replace the active class route")
	}
	if fixture.call("POST", "/manage/api/provider", map[string]any{"provider": "claude_oauth", "base_url": "", "api_key": "", "classes": map[string]any{}}, "").Code != 400 {
		test.Fatal("unimplemented Claude provider was accepted")
	}
	fixture.value(fixture.call("POST", "/manage/api/provider", map[string]any{"provider": "codex_oauth", "base_url": "", "api_key": "", "classes": map[string]any{}}, ""))
	if _, exists := fixture.console.state.Routes["high_effort"]; exists {
		test.Fatal("removing provider retained its active route")
	}
}

func TestDashboardUsesProviderHierarchyWithoutTargetControls(test *testing.T) {
	fixture := setup(test)
	response := fixture.call("GET", "/manage/", nil, "")
	if response.Code != 200 || !strings.Contains(response.Body.String(), "Codex OAuth") || !strings.Contains(response.Body.String(), "Claude OAuth") || !strings.Contains(response.Body.String(), "OpenAI-compatible API") {
		test.Fatal("provider hierarchy is missing")
	}
	for _, removed := range []string{"Target ID", "Add or update a target", "Model target"} {
		if strings.Contains(response.Body.String(), removed) {
			test.Fatalf("dashboard still exposes %q", removed)
		}
	}
	for _, model := range []string{"gpt-6-astra", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5", "gpt-5.2", "gpt-5.3-codex", "gpt-5.3-codex-spark"} {
		if !strings.Contains(response.Body.String(), model) {
			test.Fatalf("dashboard model suggestions omitted %q", model)
		}
	}
}

func TestCredentialFailureIsAtomicAndRedacted(test *testing.T) {
	fixture := setup(test)
	fixture.vault.fail = true
	response := fixture.call("POST", "/manage/api/target", map[string]string{"id": "fixture", "provider": "openai_compatible", "base_url": "https://api.example/v1", "model": "fixture", "api_key": "secret"}, "")
	if response.Code != 503 || strings.Contains(response.Body.String(), "private failure") || len(fixture.console.state.Targets) != 0 {
		test.Fatal("unsafe failed credential write")
	}
}

func TestExpiredRejectedAndDuplicatePairing(test *testing.T) {
	fixture := setup(test)
	started := fixture.value(fixture.call("POST", "/pair/start", map[string]string{"person_id": fixturePersonID, "device_id": fixtureDeviceID}, ""))
	if fixture.call("POST", "/pair/start", map[string]string{"person_id": fixturePersonID, "device_id": fixtureDeviceID}, "").Code != 429 {
		test.Fatal("pending pairing overwritten")
	}
	if fixture.call("POST", "/pair/poll", map[string]string{"proof": "wrong"}, "").Code != 401 {
		test.Fatal("bad proof accepted")
	}
	state := fixture.call("GET", "/manage/api/state", nil, "")
	if strings.Contains(state.Body.String(), started["proof"].(string)) {
		test.Fatal("poll proof leaked to management response")
	}
	fixture.console.pair.Expires = time.Now().Add(-time.Second)
	if fixture.call("POST", "/manage/api/pair/approve", map[string]any{"id": started["id"]}, "").Code != 409 {
		test.Fatal("expired request approved")
	}
	if fixture.call("POST", "/pair/poll", map[string]any{"proof": started["proof"]}, "").Code != 401 {
		test.Fatal("expired proof accepted")
	}
}

func TestUnavailableCredentialsDoNotDisableDashboard(test *testing.T) {
	fixture := setup(test)
	fixture.value(fixture.call("POST", "/manage/api/target", map[string]string{"id": "fixture", "provider": "openai_compatible", "base_url": "https://api.example/v1", "model": "fixture", "api_key": "secret"}, ""))
	fixture.vault.values = map[string]string{}
	management, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil)
	if err != nil || management.gateway == nil || !management.unavailable["fixture"] {
		test.Fatal("missing credential prevented management startup")
	}
}

func TestCodexProfileUsesOAuthRuntimeWithoutAPIKey(test *testing.T) {
	fixture := setup(test)
	runtime := &fakeAuthRuntime{}
	fixture.console.runtime = runtime
	fixture.value(fixture.call("POST", "/manage/api/provider", map[string]any{
		"provider": "codex_oauth", "base_url": "https://evil.example", "api_key": "must-not-be-stored",
		"classes": map[string]any{"fast": map[string]string{"model": "fixture", "reasoning_effort": "low"}},
	}, ""))
	profile := fixture.console.state.Providers["codex_oauth"]
	if profile.BaseURL != codexEndpoint || profile.APIKeyEnv != "" || len(fixture.vault.values) != 0 {
		test.Fatal("Codex profile accepted configurable endpoint or API key")
	}
	state := fixture.value(fixture.call("GET", "/manage/api/state", nil, ""))
	configured := state["providers"].(map[string]any)["codex_oauth"].(map[string]any)["classes"].(map[string]any)["fast"].(map[string]any)
	if configured["available"] != false {
		test.Fatal("disconnected OAuth profile reported available")
	}
	runtime.ready = true
	state = fixture.value(fixture.call("GET", "/manage/api/state", nil, ""))
	configured = state["providers"].(map[string]any)["codex_oauth"].(map[string]any)["classes"].(map[string]any)["fast"].(map[string]any)
	if configured["available"] != true {
		test.Fatal("connected OAuth profile reported unavailable")
	}
}
