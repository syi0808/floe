package httptransport

import (
	"context"
	"encoding/json"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync/atomic"
	"testing"

	"floe/server/internal/inference"
	"floe/server/internal/pairing"
	"floe/server/internal/trust"
)

func TestSchema3GatewayBudgetPreflightAndMockProviderPath(t *testing.T) {
	var providerCalls atomic.Int32
	provider := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/chat/completions" || request.Header.Get("Authorization") != "Bearer synthetic-test-key" {
			t.Errorf("mock provider received unexpected request %s authorization=%q", request.URL.Path, request.Header.Get("Authorization"))
		}
		var payload struct {
			Model               string `json:"model"`
			MaxCompletionTokens uint32 `json:"max_completion_tokens"`
			ResponseFormat      any    `json:"response_format"`
		}
		if err := json.NewDecoder(request.Body).Decode(&payload); err != nil || payload.Model != "synthetic-budget-model" || payload.MaxCompletionTokens != 96 {
			t.Errorf("mock provider received wrong selected model: %#v err=%v", payload, err)
		}
		providerCalls.Add(1)
		writer.Header().Set("Content-Type", "application/json")
		if payload.ResponseFormat != nil {
			_, _ = writer.Write([]byte(`{"choices":[{"finish_reason":"stop","message":{"content":"{\"ok\":true}"}}],"usage":{"total_tokens":17}}`))
		} else {
			_, _ = writer.Write([]byte(`{"choices":[{"finish_reason":"stop","message":{"content":"Synthetic completion"}}],"usage":{"total_tokens":17}}`))
		}
	}))
	defer provider.Close()

	fixture := newConsoleFixture(t)
	fixture.handler.Inference.ModelCatalog = fixture.catalog
	operatorCookie, csrf := fixture.login(t)
	operator, err := fixture.trust.AuthenticateOperatorSession(context.Background(), operatorCookie.Value, csrf, true)
	if err != nil {
		t.Fatalf("authenticate synthetic operator: %v", err)
	}
	configure := func(override *inference.ModelBudgetOverride) {
		t.Helper()
		result := fixture.handler.Configuration.UpdateProvider(context.Background(), operator, inference.ProviderUpdate{
			OperationID: trust.NewID(),
			Provider:    "openai_compatible",
			BaseURL:     provider.URL,
			APIKey:      "synthetic-test-key",
			Purposes: map[string]inference.PurposeModel{
				string(inference.QuickResponse): {
					Model: "synthetic-budget-model", ReasoningEffort: "", Capabilities: []string{inference.ChatCapability, inference.StructuredOutputCapability},
					BudgetOverride: override,
				},
			},
		})
		if result.Code != "" {
			t.Fatalf("configure mock provider: %s", result.Code)
		}
	}
	configure(&inference.ModelBudgetOverride{
		SchemaVersion:     inference.ModelBudgetOverrideVersion,
		MaxInputJSONBytes: budgetUint32(512), SelectedOutputReservationTokens: budgetUint32(96),
	})

	bearer := completeSyntheticPairing(t, fixture, operatorCookie, csrf)
	get := func() InventoryResponseDTO {
		t.Helper()
		response := fixture.inferenceRequest(http.MethodGet, "/v1/inference-purposes", nil, bearer)
		if response.Code != http.StatusOK {
			t.Fatalf("schema-3 inventory returned %d: %s", response.Code, response.Body.String())
		}
		var inventory InventoryResponseDTO
		if err := json.Unmarshal(response.Body.Bytes(), &inventory); err != nil {
			t.Fatalf("decode schema-3 inventory: %v", err)
		}
		if inventory.SchemaVersion != 3 || inventory.Purposes.QuickResponse.BudgetProfile == nil {
			t.Fatalf("inventory omitted schema-3 profile: %#v", inventory)
		}
		profile := inventory.Purposes.QuickResponse.BudgetProfile
		if profile.Framing.MaxInputJSONBytes != 512 || profile.ContextWindow.Status != inference.LimitUnknown ||
			profile.Sources.Catalog.Status != inference.CatalogModelNotListed {
			t.Fatalf("inventory changed selected limits or invented catalog capacity: %#v", profile)
		}
		return inventory
	}
	inventory := get()
	capability := inventory.Purposes.QuickResponse.CapabilityRevision
	if !inference.ValidHex(capability, 32) {
		t.Fatalf("invalid capability revision %q", capability)
	}

	invoke := func(content string, revision string) *httptest.ResponseRecorder {
		t.Helper()
		frame := `{"run_instructions":{"output_format":{"kind":"text"}},"test_context":"` + content + `"}`
		input, err := json.Marshal(inference.AgentInput{
			Messages: []inference.Message{{Role: "user", Content: stringPtr(frame)}},
			Tools:    []inference.Tool{},
		})
		if err != nil {
			t.Fatal(err)
		}
		body, err := json.Marshal(AgentRequestDTO{
			SchemaVersion: 3, Purpose: inference.QuickResponse, CapabilityRevision: revision,
			AttemptID: trust.NewID(), DataClasses: []string{"synthetic"}, Instructions: "Answer briefly.",
			Input: input, OutputFormat: json.RawMessage(`{"kind":"text"}`), MaxOutputBytes: 4096,
		})
		if err != nil {
			t.Fatal(err)
		}
		return fixture.inferenceRequest(http.MethodPost, "/v1/agent", body, bearer)
	}

	completed := invoke("", capability)
	if completed.Code != http.StatusOK {
		t.Fatalf("schema-3 Agent request returned %d: %s", completed.Code, completed.Body.String())
	}
	var result AgentResponseDTO
	if err := json.Unmarshal(completed.Body.Bytes(), &result); err != nil || result.SchemaVersion != 3 || len(result.Output) != 1 || result.Output[0].Kind != "answer" || result.Output[0].Text != "Synthetic completion" {
		t.Fatalf("mock-provider result did not cross the full Go handler: %#v err=%v", result, err)
	}
	if result.Usage.Tokens == nil || *result.Usage.Tokens != 17 || providerCalls.Load() != 1 {
		t.Fatalf("provider usage/call count = %#v/%d, want 17/1", result.Usage, providerCalls.Load())
	}
	structured, structuredErr := fixture.handler.Inference.Service.InvokeStructured(context.Background(), operator, inference.StructuredInvocation{
		Purpose: inference.QuickResponse, CapabilityRevision: capability, AttemptID: trust.NewID(),
		DataClasses: []string{"synthetic"}, Instructions: "Return a small JSON object.",
		Input:          json.RawMessage(`{"question":"synthetic"}`),
		OutputSchema:   json.RawMessage(`{"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false}`),
		MaxOutputBytes: 1024,
	})
	if structuredErr != nil || string(structured.Output) != `{"ok":true}` || structured.Usage.Tokens == nil || *structured.Usage.Tokens != 17 || providerCalls.Load() != 2 {
		t.Fatalf("structured mock-provider path returned %#v err=%v providerCalls=%d", structured, structuredErr, providerCalls.Load())
	}

	oversize := invoke(strings.Repeat("한", 200), capability)
	if oversize.Code != http.StatusRequestEntityTooLarge || responseCode(t, oversize) != string(inference.BodyTooLarge) || providerCalls.Load() != 2 {
		t.Fatalf("oversize preflight returned %d code=%s providerCalls=%d", oversize.Code, responseCode(t, oversize), providerCalls.Load())
	}

	configure(&inference.ModelBudgetOverride{SchemaVersion: inference.ModelBudgetOverrideVersion, MaxInputJSONBytes: budgetUint32(256)})
	drift := invoke("", capability)
	if drift.Code != http.StatusConflict || responseCode(t, drift) != string(inference.CapabilityChanged) || providerCalls.Load() != 2 {
		t.Fatalf("stale profile dispatch returned %d code=%s providerCalls=%d", drift.Code, responseCode(t, drift), providerCalls.Load())
	}
	configure(&inference.ModelBudgetOverride{
		SchemaVersion:     inference.ModelBudgetOverrideVersion,
		MaxInputJSONBytes: budgetUint32(8192), SelectedOutputReservationTokens: budgetUint32(96),
	})
	if os.Getenv("FLOE_R3A_RUN_RUST_E2E") != "1" {
		return
	}

	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen for Rust Gateway test: %v", err)
	}
	address := listener.Addr().String()
	fixture.address = address
	fixture.handler.Address = address
	fixture.handler.Inference.Address = address
	gateway := httptest.NewUnstartedServer(fixture.handler)
	gateway.Listener = listener
	gateway.Start()
	defer gateway.Close()
	runRustGatewayToGoMockProvider(t, gateway.URL, bearer)
	if providerCalls.Load() != 3 {
		t.Fatalf("Rust gateway path dispatched to mock provider %d times, want exactly one new request", providerCalls.Load())
	}
}

func runRustGatewayToGoMockProvider(t *testing.T, endpoint, bearer string) {
	t.Helper()
	_, source, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("resolve E2E test source path")
	}
	root := filepath.Clean(filepath.Join(filepath.Dir(source), "../../../.."))
	command := exec.Command("cargo", "test", "-p", "floe-provider-adapters", "--lib", "gateway::inference::tests::rust_gateway_to_go_mock_provider_e2e", "--", "--exact", "--nocapture")
	command.Dir = root
	command.Env = []string{
		"PATH=" + os.Getenv("PATH"),
		"CARGO_HOME=" + os.Getenv("CARGO_HOME"),
		"RUSTUP_HOME=" + os.Getenv("RUSTUP_HOME"),
		"TMPDIR=" + os.TempDir(),
		"FLOE_R3A_GATEWAY_ENDPOINT=" + endpoint,
		"FLOE_R3A_GATEWAY_BEARER=" + bearer,
		"FLOE_R3A_E2E_REQUIRED=1",
	}
	output, err := command.CombinedOutput()
	if err != nil {
		t.Fatalf("Rust Gateway to Go mock-provider test failed: %v\n%s", err, output)
	}
}

func TestProviderTargetChangeClearsPersistedBudgetOverride(t *testing.T) {
	first := httptest.NewServer(http.NotFoundHandler())
	defer first.Close()
	second := httptest.NewServer(http.NotFoundHandler())
	defer second.Close()
	fixture := newConsoleFixture(t)
	cookie, csrf := fixture.login(t)
	operator, err := fixture.trust.AuthenticateOperatorSession(context.Background(), cookie.Value, csrf, true)
	if err != nil {
		t.Fatalf("authenticate synthetic operator: %v", err)
	}
	update := func(endpoint, model, key string, override *inference.ModelBudgetOverride) {
		t.Helper()
		result := fixture.handler.Configuration.UpdateProvider(context.Background(), operator, inference.ProviderUpdate{
			OperationID: trust.NewID(),
			Provider:    "openai_compatible", BaseURL: endpoint, APIKey: key,
			Purposes: map[string]inference.PurposeModel{
				string(inference.QuickResponse): {
					Model: model, Capabilities: []string{inference.ChatCapability}, BudgetOverride: override,
				},
			},
		})
		if result.Code != "" {
			t.Fatalf("update provider target: %s", result.Code)
		}
	}
	update(first.URL, "model-a", "synthetic-key-a", &inference.ModelBudgetOverride{
		SchemaVersion:       inference.ModelBudgetOverrideVersion,
		ContextWindowTokens: budgetUint32(8192), MaxInputJSONBytes: budgetUint32(4096),
	})
	update(second.URL, "model-b", "synthetic-key-b", nil)

	snapshot, err := fixture.handler.Configuration.Snapshot(context.Background(), operator)
	if err != nil {
		t.Fatalf("read synthetic provider configuration: %v", err)
	}
	configured := snapshot.Profiles["openai_compatible"].Purposes[string(inference.QuickResponse)]
	if configured.Model != "model-b" || configured.BudgetOverride != nil {
		t.Fatalf("changed target retained stale measured limits: %#v", configured)
	}
	profile := snapshot.Inventory.QuickResponse.BudgetProfile
	if profile.Sources.OperatorConfiguration != inference.BudgetOverrideUnconfigured || profile.Framing.MaxInputJSONBytes != inference.LegacyAgentInputBytes {
		t.Fatalf("changed target did not resolve conservative default profile: %#v", profile)
	}
}

func (fixture *consoleFixture) inferenceRequest(method, path string, body []byte, bearer string) *httptest.ResponseRecorder {
	request := httptest.NewRequest(method, path, strings.NewReader(string(body)))
	request.Host = fixture.address
	request.Header.Set("Authorization", "Bearer "+bearer)
	if body != nil {
		request.Header.Set("Content-Type", "application/json")
	}
	response := httptest.NewRecorder()
	fixture.handler.ServeHTTP(response, request)
	return response
}

func completeSyntheticPairing(t *testing.T, fixture *consoleFixture, cookie *http.Cookie, csrf string) string {
	t.Helper()
	enrollment := fixture.startPairing(t)
	approval := fixture.request(http.MethodPost, "/manage/api/pair/approve", pairingApproval(enrollment), cookie, "http://"+fixture.address, csrf, "")
	if approval.Code != http.StatusOK {
		t.Fatalf("approve synthetic client: %d %s", approval.Code, approval.Body.String())
	}
	poll := fixture.request(http.MethodPost, "/pair/poll", pairingPoll(enrollment), nil, "", "", "")
	if poll.Code != http.StatusOK {
		t.Fatalf("poll synthetic client: %d %s", poll.Code, poll.Body.String())
	}
	var delivery struct {
		Token string `json:"token"`
	}
	if err := json.Unmarshal(poll.Body.Bytes(), &delivery); err != nil || delivery.Token == "" {
		t.Fatalf("synthetic client credential missing: %#v err=%v", delivery, err)
	}
	return delivery.Token
}

func pairingApproval(enrollment testEnrollment) pairing.ApprovalRequest {
	return pairing.ApprovalRequest{SchemaVersion: 1, PairingID: enrollment.start.PairingID, Fingerprint: enrollment.start.Issuer.Fingerprint}
}

func pairingPoll(enrollment testEnrollment) pairing.Request {
	return pairing.Request{SchemaVersion: 1, PairingID: enrollment.start.PairingID, Proof: enrollment.start.Proof}
}

func budgetUint32(value uint32) *uint32 { return &value }

func stringPtr(value string) *string { return &value }
