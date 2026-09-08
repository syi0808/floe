package inference

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

const testToken = "fixture-gateway-token-at-least-32-bytes"
const candidate = `{"slot_id":"slot_1","reason":"Unoccupied","source_ids":["schedule"]}`

type fixtureCodex struct{ calls atomic.Int32 }

func (client *fixtureCodex) Ready() bool { return true }

func (client *fixtureCodex) Generate(_ context.Context, model, effort, instructions string, input, schema json.RawMessage) (string, error) {
	client.calls.Add(1)
	if model != "fixture-model" || effort != "high" || instructions == "" || !json.Valid(input) || !json.Valid(schema) {
		return "", errors.New("bad request")
	}
	return candidate, nil
}

func fixtureRequest() Request {
	return Request{SchemaVersion: 1, InferenceClass: "high_effort", Instructions: "Choose one supplied slot", Input: json.RawMessage(`{"slots":[{"id":"slot_1"}]}`), OutputSchema: json.RawMessage(`{"type":"object","properties":{"slot_id":{"type":"string"}},"required":["slot_id"]}`)}
}

func fixtureGateway(test *testing.T, providerName, endpoint string) *Gateway {
	test.Helper()
	gateway, err := New(Config{Targets: map[string]Target{"fixture": {Provider: providerName, BaseURL: endpoint, Model: "fixture-model", APIKeyEnv: "FIXTURE_KEY"}}, Routes: map[string]Route{"high_effort": {Target: "fixture", ReasoningEffort: "high"}}}, testToken, func(string) string { return "private-provider-key" })
	if err != nil {
		test.Fatal(err)
	}
	return gateway
}

func TestExtendedReasoningEffortsAreAccepted(test *testing.T) {
	for _, effort := range []string{"max", "ultra"} {
		_, err := New(Config{
			Targets: map[string]Target{"codex": {Provider: "codex_oauth", BaseURL: "https://chatgpt.com/backend-api/codex", Model: "fixture-model"}},
			Routes:  map[string]Route{"high_effort": {Target: "codex", ReasoningEffort: effort}},
		}, testToken, func(string) string { return "" }, &fixtureCodex{})
		if err != nil {
			test.Errorf("reasoning effort %q was rejected: %v", effort, err)
		}
	}
}

func invoke(gateway *Gateway, input Request) *httptest.ResponseRecorder {
	body, _ := json.Marshal(input)
	request := httptest.NewRequest(http.MethodPost, "/v1/generate", bytes.NewReader(body))
	request.Header.Set("Authorization", "Bearer "+testToken)
	writer := httptest.NewRecorder()
	gateway.ServeHTTP(writer, request)
	return writer
}

func TestOpenAIWireContract(test *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/v1/chat/completions" || request.Header.Get("Authorization") != "Bearer private-provider-key" {
			test.Error("wrong provider request")
		}
		var body map[string]any
		if json.NewDecoder(request.Body).Decode(&body) != nil {
			test.Error("bad request JSON")
		}
		if body["model"] != "fixture-model" || body["reasoning_effort"] != "high" || body["stream"] != false || body["tools"] != nil {
			test.Error("unexpected model capabilities")
		}
		format := body["response_format"].(map[string]any)
		if format["type"] != "json_schema" {
			test.Error("missing schema")
		}
		encoded, _ := json.Marshal(body)
		if strings.Contains(string(encoded), "private-provider-key") {
			test.Error("credential entered model context")
		}
		_ = json.NewEncoder(writer).Encode(map[string]any{"choices": []any{map[string]any{"finish_reason": "stop", "message": map[string]any{"content": candidate}}}})
	}))
	defer upstream.Close()
	gateway := fixtureGateway(test, "openai_compatible", upstream.URL+"/v1")
	input := fixtureRequest()
	input.AllowExternal = true
	response := invoke(gateway, input)
	if response.Code != 200 || !strings.Contains(response.Body.String(), `"inference_class":"high_effort"`) || strings.Contains(response.Body.String(), "fixture-model") {
		test.Fatal(response.Body.String())
	}
	if response.Header().Get("Cache-Control") != "no-store" {
		test.Error("response must not be cached")
	}
}

func TestExternalConsentPrecedesAnyProviderCall(test *testing.T) {
	var calls atomic.Int32
	upstream := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) { calls.Add(1) }))
	defer upstream.Close()
	response := invoke(fixtureGateway(test, "openai_compatible", upstream.URL), fixtureRequest())
	if response.Code != 403 || !strings.Contains(response.Body.String(), "external_transfer_denied") || calls.Load() != 0 {
		test.Fatal("external transfer was not blocked")
	}
}

func TestCodexOAuthUsesServerRuntimeBehindConsent(test *testing.T) {
	client := &fixtureCodex{}
	gateway, err := New(Config{Targets: map[string]Target{"codex": {
		Provider: "codex_oauth", BaseURL: "https://chatgpt.com/backend-api/codex", Model: "fixture-model",
	}}, Routes: map[string]Route{"high_effort": {Target: "codex", ReasoningEffort: "high"}}}, testToken, func(string) string { return "" }, client)
	if err != nil {
		test.Fatal(err)
	}
	input := fixtureRequest()
	if invoke(gateway, input).Code != http.StatusForbidden || client.calls.Load() != 0 {
		test.Fatal("Codex request bypassed consent")
	}
	input.AllowExternal = true
	if invoke(gateway, input).Code != http.StatusOK || client.calls.Load() != 1 {
		test.Fatal("Codex OAuth runtime was not used")
	}
	if _, err = New(Config{Targets: map[string]Target{"codex": {
		Provider: "codex_oauth", BaseURL: "https://untrusted.example", Model: "fixture-model",
	}}}, testToken, func(string) string { return "" }, client); err == nil {
		test.Fatal("custom Codex endpoint accepted")
	}
}

func TestOllamaPreflightAndStructuredRequest(test *testing.T) {
	var calls atomic.Int32
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		calls.Add(1)
		if request.URL.Path == "/api/show" {
			_, _ = writer.Write([]byte(`{}`))
			return
		}
		if request.URL.Path != "/api/chat" {
			test.Error("wrong path")
		}
		var body map[string]any
		_ = json.NewDecoder(request.Body).Decode(&body)
		if body["format"] == nil || body["stream"] != false || body["tools"] != nil {
			test.Error("wrong Ollama contract")
		}
		_ = json.NewEncoder(writer).Encode(map[string]any{"done": true, "message": map[string]any{"content": candidate}})
	}))
	defer upstream.Close()
	response := invoke(fixtureGateway(test, "ollama", upstream.URL), fixtureRequest())
	if response.Code != 200 || calls.Load() != 2 {
		test.Fatal(response.Body.String())
	}
}

func TestOllamaCloudAliasNeverReceivesContext(test *testing.T) {
	var calls atomic.Int32
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		calls.Add(1)
		if request.URL.Path != "/api/show" {
			test.Error("context sent to cloud alias")
		}
		_, _ = writer.Write([]byte(`{"remote_model":"remote","remote_host":"https://example.com"}`))
	}))
	defer upstream.Close()
	if invoke(fixtureGateway(test, "ollama", upstream.URL), fixtureRequest()).Code != 502 || calls.Load() != 1 {
		test.Fatal("cloud alias accepted")
	}
}

func TestAuthenticationOriginAndRequestBounds(test *testing.T) {
	gateway := fixtureGateway(test, "ollama", "http://127.0.0.1:1")
	for _, auth := range []string{"", "Bearer wrong", testToken} {
		request := httptest.NewRequest(http.MethodPost, "/v1/generate", strings.NewReader(`{}`))
		request.Header.Set("Authorization", auth)
		writer := httptest.NewRecorder()
		gateway.ServeHTTP(writer, request)
		if writer.Code != 401 {
			test.Error("unauthenticated request accepted")
		}
	}
	request := httptest.NewRequest(http.MethodGet, "/v1/inference-classes", nil)
	request.Header.Set("Authorization", "Bearer "+testToken)
	request.Header.Set("Origin", "https://untrusted.example")
	writer := httptest.NewRecorder()
	gateway.ServeHTTP(writer, request)
	if writer.Code != 403 {
		test.Error("browser origin accepted")
	}
	for _, body := range []string{`{} {}`, `{"credential":"secret"}`, strings.Repeat("x", 98305)} {
		request = httptest.NewRequest(http.MethodPost, "/v1/generate", strings.NewReader(body))
		request.Header.Set("Authorization", "Bearer "+testToken)
		writer = httptest.NewRecorder()
		gateway.ServeHTTP(writer, request)
		if writer.Code != 400 {
			test.Error("invalid request accepted")
		}
	}
	input := fixtureRequest()
	input.SchemaVersion = 2
	if invoke(gateway, input).Code != 400 {
		test.Error("unsupported version accepted")
	}
	input = fixtureRequest()
	input.InferenceClass = "unknown"
	if invoke(gateway, input).Code != 400 {
		test.Error("unknown target accepted")
	}
}

func TestClassInventoryDoesNotExposeModelRoutingOrSecrets(test *testing.T) {
	gateway := fixtureGateway(test, "openai_compatible", "https://private.example/v1")
	request := httptest.NewRequest(http.MethodGet, "/v1/inference-classes", nil)
	request.Header.Set("Authorization", "Bearer "+testToken)
	writer := httptest.NewRecorder()
	gateway.ServeHTTP(writer, request)
	if writer.Code != 200 {
		test.Fatal(writer.Body.String())
	}
	for _, secret := range []string{"private-provider-key", "FIXTURE_KEY", "private.example", "fixture-model", "fixture", testToken} {
		if strings.Contains(writer.Body.String(), secret) {
			test.Error("inventory exposed secret or configuration")
		}
	}
}

func TestPurposeRoutingAndContentFreeTrace(test *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		_ = json.NewEncoder(writer).Encode(map[string]any{"choices": []any{map[string]any{"finish_reason": "stop", "message": map[string]any{"content": candidate}}}})
	}))
	defer upstream.Close()
	gateway := fixtureGateway(test, "openai_compatible", upstream.URL)

	inventory := httptest.NewRequest(http.MethodGet, "/v2/inference-purposes", nil)
	inventory.Header.Set("Authorization", "Bearer "+testToken)
	inventoryWriter := httptest.NewRecorder()
	gateway.ServeHTTP(inventoryWriter, inventory)
	if inventoryWriter.Code != http.StatusOK || !strings.Contains(inventoryWriter.Body.String(), `"deep_work":{"available":true,"placement":"external","recipient":"127.0.0.1","requires_external_consent":true}`) || strings.Contains(inventoryWriter.Body.String(), "fixture-model") {
		test.Fatal(inventoryWriter.Body.String())
	}

	input := fixtureRequest()
	input.SchemaVersion = 2
	input.InferenceClass = ""
	input.Purpose = "deep_work"
	input.DataClasses = []string{"personal"}
	input.AllowExternal = true
	body, _ := json.Marshal(input)
	request := httptest.NewRequest(http.MethodPost, "/v2/generate", bytes.NewReader(body))
	request.Header.Set("Authorization", "Bearer "+testToken)
	writer := httptest.NewRecorder()
	gateway.ServeHTTP(writer, request)
	if writer.Code != http.StatusOK || strings.Contains(writer.Body.String(), "high_effort") || !strings.Contains(writer.Body.String(), `"purpose":"deep_work"`) {
		test.Fatal(writer.Body.String())
	}
	var response struct {
		TraceID string `json:"trace_id"`
	}
	if json.Unmarshal(writer.Body.Bytes(), &response) != nil || response.TraceID == "" {
		test.Fatal("missing trace ID")
	}
	traceRequest := httptest.NewRequest(http.MethodGet, "/v2/traces/"+response.TraceID, nil)
	traceRequest.Header.Set("Authorization", "Bearer "+testToken)
	traceWriter := httptest.NewRecorder()
	gateway.ServeHTTP(traceWriter, traceRequest)
	trace := traceWriter.Body.String()
	if traceWriter.Code != http.StatusOK || !strings.Contains(trace, `"placement":"remote"`) || strings.Contains(trace, "Choose one supplied slot") || strings.Contains(trace, candidate) || strings.Contains(trace, "fixture-model") {
		test.Fatal(trace)
	}
	listRequest := httptest.NewRequest(http.MethodGet, "/v2/traces", nil)
	listRequest.Header.Set("Authorization", "Bearer "+testToken)
	listWriter := httptest.NewRecorder()
	gateway.ServeHTTP(listWriter, listRequest)
	if listWriter.Code != http.StatusOK || !strings.Contains(listWriter.Body.String(), response.TraceID) || strings.Contains(listWriter.Body.String(), candidate) {
		test.Fatal(listWriter.Body.String())
	}
}

func TestPurposeAPIRejectsClientSelectedClass(test *testing.T) {
	input := fixtureRequest()
	input.SchemaVersion = 2
	input.Purpose = "deep_work"
	input.AllowExternal = true
	body, _ := json.Marshal(input)
	request := httptest.NewRequest(http.MethodPost, "/v2/generate", bytes.NewReader(body))
	request.Header.Set("Authorization", "Bearer "+testToken)
	writer := httptest.NewRecorder()
	fixtureGateway(test, "ollama", "http://127.0.0.1:1").ServeHTTP(writer, request)
	if writer.Code != http.StatusBadRequest {
		test.Fatal("v2 accepted a client-selected inference class")
	}
}

func TestReplayRequiresTheExactContentAndRecordsParentTrace(test *testing.T) {
	var calls atomic.Int32
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		calls.Add(1)
		_ = json.NewEncoder(writer).Encode(map[string]any{"choices": []any{map[string]any{"finish_reason": "stop", "message": map[string]any{"content": candidate}}}})
	}))
	defer upstream.Close()
	gateway := fixtureGateway(test, "openai_compatible", upstream.URL)
	input := fixtureRequest()
	input.SchemaVersion = 2
	input.InferenceClass = ""
	input.Purpose = "deep_work"
	input.DataClasses = []string{"personal"}
	input.AllowExternal = true
	first := invokePath(gateway, "/v2/generate", input)
	var generated struct {
		TraceID string `json:"trace_id"`
	}
	_ = json.Unmarshal(first.Body.Bytes(), &generated)
	input.ReplayOf = generated.TraceID
	replayed := invokePath(gateway, "/v2/generate", input)
	if replayed.Code != http.StatusOK || calls.Load() != 2 {
		test.Fatal(replayed.Body.String())
	}
	input.Instructions = "Changed content"
	refused := invokePath(gateway, "/v2/generate", input)
	if refused.Code != http.StatusConflict || calls.Load() != 2 {
		test.Fatal("mismatched replay reached provider")
	}
}

func invokePath(gateway *Gateway, path string, input Request) *httptest.ResponseRecorder {
	body, _ := json.Marshal(input)
	request := httptest.NewRequest(http.MethodPost, path, bytes.NewReader(body))
	request.Header.Set("Authorization", "Bearer "+testToken)
	writer := httptest.NewRecorder()
	gateway.ServeHTTP(writer, request)
	return writer
}

func TestOnlySupportedInferenceClassesAndValidRoutesAreAccepted(test *testing.T) {
	target := Target{Provider: "ollama", BaseURL: "http://127.0.0.1:11434", Model: "fixture"}
	for class, route := range map[string]Route{
		"domain_task": {Target: "fixture"},
		"high-effort": {Target: "fixture"},
		"high_effort": {Target: "missing"},
	} {
		if _, err := New(Config{Targets: map[string]Target{"fixture": target}, Routes: map[string]Route{class: route}}, testToken, func(string) string { return "" }); err == nil {
			test.Errorf("accepted invalid route %q", class)
		}
	}
}

func TestMalformedRefusedToolAndOversizedOutput(test *testing.T) {
	for _, payload := range []string{
		`not JSON`, `{"choices":[]}`, `{"choices":[{"finish_reason":"length","message":{"content":"{}"}}]}`,
		`{"choices":[{"finish_reason":"stop","message":{"content":"{}","tool_calls":[{}]}}]}`,
		`{"choices":[{"finish_reason":"stop","message":{"content":"{}","refusal":"no"}}]}`,
		`{"choices":[{"finish_reason":"stop","message":{"content":"not JSON"}}]}`,
		strings.Repeat("x", 1048577),
	} {
		upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) { _, _ = writer.Write([]byte(payload)) }))
		input := fixtureRequest()
		input.AllowExternal = true
		response := invoke(fixtureGateway(test, "openai_compatible", upstream.URL), input)
		upstream.Close()
		if response.Code != 502 || !strings.Contains(response.Body.String(), "invalid_proposal") {
			test.Fatal(response.Body.String())
		}
	}
}

func TestProviderErrorsAreRedactedWithoutRetriesOrRedirects(test *testing.T) {
	for _, status := range []int{302, 401, 429, 500} {
		var calls atomic.Int32
		upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
			calls.Add(1)
			writer.Header().Set("Location", "/redirected")
			writer.WriteHeader(status)
			_, _ = writer.Write([]byte("PRIVATE UPSTREAM ERROR"))
		}))
		input := fixtureRequest()
		input.AllowExternal = true
		response := invoke(fixtureGateway(test, "openai_compatible", upstream.URL), input)
		upstream.Close()
		if response.Code != 502 || calls.Load() != 1 || strings.Contains(response.Body.String(), "PRIVATE") {
			test.Fatal(response.Body.String())
		}
	}
}

func TestDeadlineAndClientCancellationReachUpstream(test *testing.T) {
	for _, cancelClient := range []bool{false, true} {
		started := make(chan struct{})
		cancelled := make(chan struct{})
		upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
			_, _ = io.Copy(io.Discard, request.Body)
			close(started)
			<-request.Context().Done()
			close(cancelled)
		}))
		gateway := fixtureGateway(test, "openai_compatible", upstream.URL)
		gateway.timeout = 100 * time.Millisecond
		input := fixtureRequest()
		input.AllowExternal = true
		body, _ := json.Marshal(input)
		ctx, cancel := context.WithCancel(context.Background())
		request := httptest.NewRequest(http.MethodPost, "/v1/generate", bytes.NewReader(body)).WithContext(ctx)
		request.Header.Set("Authorization", "Bearer "+testToken)
		writer := httptest.NewRecorder()
		done := make(chan struct{})
		go func() { gateway.ServeHTTP(writer, request); close(done) }()
		select {
		case <-started:
		case <-time.After(3 * time.Second):
			test.Fatal("upstream not reached")
		}
		if cancelClient {
			cancel()
		}
		select {
		case <-cancelled:
		case <-time.After(3 * time.Second):
			test.Fatal("cancellation not propagated")
		}
		<-done
		cancel()
		upstream.Close()
		if !cancelClient && !strings.Contains(writer.Body.String(), "model_timeout") {
			test.Fatal(writer.Body.String())
		}
	}
}

func TestConcurrencyIsBounded(test *testing.T) {
	gateway := fixtureGateway(test, "ollama", "http://127.0.0.1:1")
	for index := 0; index < cap(gateway.active); index++ {
		gateway.active <- struct{}{}
	}
	if invoke(gateway, fixtureRequest()).Code != 429 {
		test.Fatal("concurrency limit ignored")
	}
}

func TestConfigurationRejectsUnsafeDestinationsAndMissingKeys(test *testing.T) {
	for _, target := range []Target{
		{Provider: "openai_compatible", BaseURL: "http://example.com/v1", Model: "model"},
		{Provider: "openai_compatible", BaseURL: "https://user:secret@example.com/v1", Model: "model"},
		{Provider: "openai_compatible", BaseURL: "https://example.com/v1?key=secret", Model: "model"},
		{Provider: "ollama", BaseURL: "https://example.com", Model: "model"},
		{Provider: "ollama", BaseURL: "http://127.0.0.1:11434", Model: "model:cloud"},
		{Provider: "openai_compatible", BaseURL: "https://example.com", Model: "model", APIKeyEnv: "MISSING_KEY"},
		{Provider: "unknown", BaseURL: "https://example.com", Model: "model"},
	} {
		_, err := New(Config{Targets: map[string]Target{"fixture": target}}, testToken, func(string) string { return "" })
		if err == nil {
			test.Fatal("unsafe configuration accepted")
		}
	}
	if _, err := New(Config{Targets: map[string]Target{}}, "short", func(string) string { return "" }); err == nil {
		test.Fatal("weak gateway authentication")
	}
}
