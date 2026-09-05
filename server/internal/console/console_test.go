package console

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

type memoryVault struct {
	values map[string]string
	fail   bool
}

type fakeAuthRuntime struct{ ready bool }

func (runtime *fakeAuthRuntime) Action(context.Context, string) (any, error) {
	return map[string]any{"status": "connected", "inference_enabled": runtime.ready}, nil
}

func (runtime *fakeAuthRuntime) Ready() bool { return runtime.ready }

func (*fakeAuthRuntime) Generate(context.Context, string, string, string, json.RawMessage, json.RawMessage) (string, error) {
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
	started := fixture.value(fixture.call("POST", "/pair/start", map[string]string{}, ""))
	proof := started["proof"].(string)
	pending := fixture.value(fixture.call("POST", "/pair/poll", map[string]string{"proof": proof}, ""))
	if pending["status"] != "pending" || pending["token"] != nil {
		fixture.test.Fatal("unapproved credential issued")
	}
	fixture.value(fixture.call("POST", "/manage/api/pair/approve", map[string]any{"id": started["id"]}, ""))
	approved := fixture.value(fixture.call("POST", "/pair/poll", map[string]string{"proof": proof}, ""))
	return approved["client_id"].(string), approved["token"].(string)
}

func TestPairingRestartAndRevocation(test *testing.T) {
	fixture := setup(test)
	identifier, token := fixture.pair()
	fixture.value(fixture.call("GET", "/v1/inference-classes", nil, token))
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
	fixture.value(fixture.call("GET", "/v1/inference-classes", nil, token))
	if fixture.call("GET", "/manage/api/state", nil, "").Code != 401 {
		test.Fatal("management session survived restart")
	}
	fixture.console = old
	fixture.value(fixture.call("POST", "/manage/api/client/delete", map[string]string{"id": identifier}, ""))
	if fixture.call("GET", "/v1/inference-classes", nil, token).Code != 401 {
		test.Fatal("revoked token accepted")
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
		{"/v1/inference-classes", "GET", "http://127.0.0.1:8431", "127.0.0.1:8431", "", true, token},
		{"/v1/inference-classes", "GET", "", "127.0.0.1:8431", "", true, ""},
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
		if body["messages"].([]any)[1].(map[string]any)["content"] != `{"test":true}` {
			test.Error("test context was not synthetic")
		}
		_, _ = writer.Write([]byte(`{"choices":[{"finish_reason":"stop","message":{"content":"{\"ok\":true}"}}]}`))
	}))
	defer upstream.Close()
	input := map[string]string{"id": "focus", "provider": "openai_compatible", "base_url": upstream.URL, "model": "fixture", "api_key": "private-provider-key"}
	fixture.value(fixture.call("POST", "/manage/api/target", input, ""))
	fixture.value(fixture.call("POST", "/manage/api/route", map[string]string{"inference_class": "high_effort", "target": "focus", "reasoning_effort": "high"}, ""))
	if calls.Load() != 0 {
		test.Fatal("adding target transmitted a request")
	}
	state := fixture.call("GET", "/manage/api/state", nil, "")
	if !strings.Contains(state.Body.String(), `"high_effort"`) || !strings.Contains(state.Body.String(), `"reasoning_effort":"high"`) {
		test.Fatal("saved route missing from management state")
	}
	_, appToken := fixture.pair()
	classes := fixture.call("GET", "/v1/inference-classes", nil, appToken)
	if !strings.Contains(classes.Body.String(), `"high_effort"`) || strings.Contains(classes.Body.String(), "fixture") || strings.Contains(classes.Body.String(), "focus") {
		test.Fatal("app inference inventory exposed server routing")
	}
	disk, _ := os.ReadFile(filepath.Join(fixture.console.directory, "state.json"))
	if strings.Contains(state.Body.String()+string(disk), "private-provider-key") || len(fixture.vault.values) != 1 {
		test.Fatal("credential storage boundary violated")
	}
	if fixture.call("POST", "/manage/api/test", map[string]any{"id": "focus", "allow_external": false}, "").Code != 403 || calls.Load() != 0 {
		test.Fatal("consent did not gate provider call")
	}
	fixture.value(fixture.call("POST", "/manage/api/test", map[string]any{"id": "focus", "allow_external": true}, ""))
	if calls.Load() != 1 {
		test.Fatal("unexpected request count")
	}
	input["api_key"] = ""
	input["base_url"] = "https://new-provider.example/v1"
	fixture.value(fixture.call("POST", "/manage/api/target", input, ""))
	if fixture.console.state.Targets["focus"].APIKeyEnv != "" || len(fixture.vault.values) != 0 {
		test.Fatal("credential inherited by changed endpoint")
	}
}

func TestCredentialFailureIsAtomicAndRedacted(test *testing.T) {
	fixture := setup(test)
	fixture.vault.fail = true
	response := fixture.call("POST", "/manage/api/target", map[string]string{"id": "focus", "provider": "openai_compatible", "base_url": "https://api.example/v1", "model": "fixture", "api_key": "secret"}, "")
	if response.Code != 503 || strings.Contains(response.Body.String(), "private failure") || len(fixture.console.state.Targets) != 0 {
		test.Fatal("unsafe failed credential write")
	}
}

func TestExpiredRejectedAndDuplicatePairing(test *testing.T) {
	fixture := setup(test)
	started := fixture.value(fixture.call("POST", "/pair/start", map[string]string{}, ""))
	if fixture.call("POST", "/pair/start", map[string]string{}, "").Code != 429 {
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
	fixture.value(fixture.call("POST", "/manage/api/target", map[string]string{"id": "focus", "provider": "openai_compatible", "base_url": "https://api.example/v1", "model": "fixture", "api_key": "secret"}, ""))
	fixture.vault.values = map[string]string{}
	management, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil)
	if err != nil || management.gateway == nil || !management.unavailable["focus"] {
		test.Fatal("missing credential prevented management startup")
	}
}

func TestCodexTargetUsesOAuthRuntimeWithoutAPIKey(test *testing.T) {
	fixture := setup(test)
	runtime := &fakeAuthRuntime{}
	fixture.console.runtime = runtime
	fixture.value(fixture.call("POST", "/manage/api/target", map[string]string{
		"id": "codex-focus", "provider": "codex_oauth", "base_url": "https://evil.example", "model": "fixture", "api_key": "must-not-be-stored",
	}, ""))
	target := fixture.console.state.Targets["codex-focus"]
	if target.BaseURL != "https://chatgpt.com/backend-api/codex" || target.APIKeyEnv != "" || len(fixture.vault.values) != 0 {
		test.Fatal("Codex target accepted configurable endpoint or API key")
	}
	state := fixture.value(fixture.call("GET", "/manage/api/state", nil, ""))
	if state["targets"].(map[string]any)["codex-focus"].(map[string]any)["available"] != false {
		test.Fatal("disconnected OAuth target reported available")
	}
	runtime.ready = true
	state = fixture.value(fixture.call("GET", "/manage/api/state", nil, ""))
	if state["targets"].(map[string]any)["codex-focus"].(map[string]any)["available"] != true {
		test.Fatal("connected OAuth target reported unavailable")
	}
}
