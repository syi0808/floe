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
