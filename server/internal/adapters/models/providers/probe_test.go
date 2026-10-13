package providers

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"reflect"
	"sync/atomic"
	"testing"

	"floe/server/internal/inference"
	"floe/server/internal/trust"
)

type probeTrust struct{ denied bool }

func (p probeTrust) WithCurrentPrincipal(_ trust.Principal, f func(trust.PrincipalSnapshot) error) error {
	return f(trust.PrincipalSnapshot{})
}
func (p probeTrust) WithCurrentOperator(_ trust.OperatorPrincipal, f func() error) error {
	if p.denied {
		return errors.New("synthetic operator denied")
	}
	return f()
}
func (p probeTrust) ActiveIssuer(trust.Principal) (trust.IssuerSnapshot, error) {
	return trust.IssuerSnapshot{}, nil
}

type unknownProbeMetadata struct{}

func (unknownProbeMetadata) Snapshot(context.Context) (inference.CapabilityEvidenceSnapshot, error) {
	return inference.CapabilityEvidenceSnapshot{ContractVersion: inference.CapabilityEvidenceContractVersion, Entries: []inference.CapabilityEvidenceEntry{}}, nil
}

type recordingProbeExecutor struct {
	inference.ModelExecutor
	target  inference.ResolvedModelTarget
	request inference.AgentInvocation
}

func (r *recordingProbeExecutor) InvokeAgent(ctx context.Context, target inference.ResolvedModelTarget, in inference.AgentInvocation) (inference.AgentResult, error) {
	r.target, r.request = target, in
	return r.ModelExecutor.InvokeAgent(ctx, target, in)
}

func TestOperatorProbeUsesRealProviderWithoutPromotingUnknownCapabilities(t *testing.T) {
	var calls atomic.Int32
	provider := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		if r.URL.Path != "/chat/completions" {
			t.Error("unexpected provider path")
		}
		var body struct {
			Tools    []any                            `json:"tools"`
			Messages []struct{ Role, Content string } `json:"messages"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Error(err)
		}
		if len(body.Tools) != 0 || len(body.Messages) != 2 || body.Messages[1].Content != "Confirm that this synthetic Agent connectivity check succeeded." {
			t.Error("probe carried non-synthetic input")
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Synthetic check succeeded."}}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}`))
	}))
	defer provider.Close()
	factory := NewFactory(func(context.Context, string) (string, error) { return "", nil }, nil)
	accounts, executor, err := factory.Open(context.Background(), map[string]inference.ProviderTarget{"synthetic": {Provider: "openai_compatible", BaseURL: provider.URL, Model: "synthetic-model"}})
	if err != nil {
		t.Fatal(err)
	}
	service, err := inference.NewService(probeTrust{}, unknownProbeMetadata{})
	if err != nil {
		t.Fatal(err)
	}
	config := inference.InferenceConfig{Routes: map[inference.Purpose]inference.PurposeRoute{inference.QuickResponse: {TargetID: "synthetic", Enabled: true}}}
	recorder := &recordingProbeExecutor{ModelExecutor: executor}
	if err := service.Configure(config, accounts, recorder); err != nil {
		t.Fatal(err)
	}
	before, err := service.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	result, err := service.ProbeTarget(context.Background(), trust.OperatorPrincipal{}, "synthetic")
	if err != nil {
		t.Fatalf("real provider rejected synthetic operator probe: %v", err)
	}
	if result.TraceID == "" || calls.Load() != 1 {
		t.Fatal("probe did not reach synthetic provider exactly once")
	}
	after, err := service.Snapshot(context.Background())
	if err != nil || !reflect.DeepEqual(before, after) {
		t.Fatal("probe promoted or changed product capabilities")
	}
	for _, state := range recorder.target.CapabilityStates() {
		if state.Status == inference.CapabilitySupported {
			t.Fatal("probe fabricated capability evidence")
		}
	}
	mutations := map[string]func(*inference.AgentInvocation){
		"input":        func(in *inference.AgentInvocation) { text := "different input"; in.Input.Messages[0].Content = &text },
		"instructions": func(in *inference.AgentInvocation) { in.Instructions = "different instructions" },
		"tools": func(in *inference.AgentInvocation) {
			in.Input.Tools = []inference.Tool{{Type: "function", Function: inference.ToolFunction{Name: "synthetic-tool"}}}
		},
		"format": func(in *inference.AgentInvocation) {
			in.OutputFormat = inference.OutputFormat{Kind: "json", Schema: json.RawMessage(`{"type":"object"}`)}
		},
		"attempt":  func(in *inference.AgentInvocation) { in.AttemptID = trust.NewID() },
		"classes":  func(in *inference.AgentInvocation) { in.DataClasses = []string{"personal"} },
		"limit":    func(in *inference.AgentInvocation) { in.MaxOutputBytes++ },
		"revision": func(in *inference.AgentInvocation) { in.CapabilityRevision = "different" },
	}
	encoded, err := json.Marshal(recorder.request)
	if err != nil {
		t.Fatal(err)
	}
	for name, mutate := range mutations {
		t.Run(name, func(t *testing.T) {
			var changed inference.AgentInvocation
			if err := json.Unmarshal(encoded, &changed); err != nil {
				t.Fatal(err)
			}
			mutate(&changed)
			if _, err := executor.InvokeAgent(context.Background(), recorder.target, changed); err == nil {
				t.Fatal("probe authority accepted changed invocation")
			}
			if calls.Load() != 1 {
				t.Fatal("changed probe reached provider")
			}
		})
	}
	// No purpose route is required for an explicitly admitted operator probe.
	if err := service.Configure(inference.InferenceConfig{Routes: map[inference.Purpose]inference.PurposeRoute{}}, accounts, recorder); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ProbeTarget(context.Background(), trust.OperatorPrincipal{}, "synthetic"); err != nil {
		t.Fatal(err)
	}
	if calls.Load() != 2 {
		t.Fatal("unrouted registered target was not probed")
	}
	denied, err := inference.NewService(probeTrust{denied: true}, unknownProbeMetadata{})
	if err != nil {
		t.Fatal(err)
	}
	if err := denied.Configure(config, accounts, executor); err != nil {
		t.Fatal(err)
	}
	if _, err := denied.ProbeTarget(context.Background(), trust.OperatorPrincipal{}, "synthetic"); err == nil {
		t.Fatal("operator authorization bypassed")
	}
	if calls.Load() != 2 {
		t.Fatal("denied operator reached provider")
	}
}
