package providers

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"

	"floe/server/internal/inference"
	"floe/server/internal/trust"
)

func TestOllamaEnforcesSelectedOutputReservationForAgentAndStructuredCalls(t *testing.T) {
	var calls atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.Header().Set("Content-Type", "application/json")
		switch request.URL.Path {
		case "/api/show":
			_, _ = writer.Write([]byte(`{}`))
		case "/api/chat":
			var payload struct {
				Options struct {
					NumPredict uint32 `json:"num_predict"`
				} `json:"options"`
				Format any `json:"format"`
			}
			if err := json.NewDecoder(request.Body).Decode(&payload); err != nil || payload.Options.NumPredict != 96 {
				t.Errorf("Ollama request omitted configured output cap: %#v err=%v", payload, err)
			}
			calls.Add(1)
			if payload.Format != nil {
				_, _ = writer.Write([]byte(`{"done":true,"message":{"content":"{\"ok\":true}"},"prompt_eval_count":10,"eval_count":7}`))
			} else {
				_, _ = writer.Write([]byte(`{"done":true,"message":{"content":"Synthetic completion"},"prompt_eval_count":10,"eval_count":7}`))
			}
		default:
			t.Errorf("unexpected Ollama path %q", request.URL.Path)
			http.NotFound(writer, request)
		}
	}))
	defer server.Close()

	p, err := newProvider(context.Background(), inference.ProviderTarget{
		Provider: "ollama", BaseURL: server.URL, Model: "local-model",
		Capabilities: []string{inference.ChatCapability, inference.StructuredOutputCapability},
	}, func(context.Context, string) (string, error) { return "", nil }, nil)
	if err != nil {
		t.Fatalf("create loopback Ollama adapter: %v", err)
	}
	limit := uint32(96)
	content := "synthetic"
	agent, err := p.agent(context.Background(), inference.AgentInvocation{
		Purpose: inference.QuickResponse, CapabilityRevision: "revision", AttemptID: trust.NewID(),
		DataClasses: []string{"synthetic"}, Instructions: "Answer briefly.",
		Input:        inference.AgentInput{Messages: []inference.Message{{Role: "user", Content: &content}}, Tools: []inference.Tool{}},
		OutputFormat: inference.OutputFormat{Kind: "text"}, MaxOutputBytes: 1024,
	}, "", &limit)
	if err != nil || len(agent.Output) != 1 || agent.Output[0].Text != "Synthetic completion" {
		t.Fatalf("Ollama agent request result %#v err=%v", agent, err)
	}
	structured, err := p.structured(context.Background(), inference.StructuredInvocation{
		Purpose: inference.QuickResponse, CapabilityRevision: "revision", AttemptID: trust.NewID(),
		DataClasses: []string{"synthetic"}, Instructions: "Return a JSON object.",
		Input:          json.RawMessage(`{"question":"synthetic"}`),
		OutputSchema:   json.RawMessage(`{"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false}`),
		MaxOutputBytes: 1024,
	}, "", &limit)
	if err != nil || string(structured.Output) != `{"ok":true}` || calls.Load() != 2 {
		t.Fatalf("Ollama structured result %#v err=%v chatCalls=%d", structured, err, calls.Load())
	}
}

func TestCodexRejectsConfiguredOutputTokenReservation(t *testing.T) {
	limit := uint32(96)
	factory := NewFactory(func(context.Context, string) (string, error) { return "", nil }, nil)
	err := factory.ValidateTarget(inference.ProviderTarget{
		Provider: "codex_oauth", BaseURL: "https://chatgpt.com/backend-api/codex", Model: "model-a",
		Capabilities: []string{inference.ChatCapability},
		BudgetOverride: &inference.ModelBudgetOverride{
			SchemaVersion:                   inference.ModelBudgetOverrideVersion,
			SelectedOutputReservationTokens: &limit,
		},
	})
	if err == nil {
		t.Fatal("Codex accepted an output reservation it cannot enforce")
	}
}
