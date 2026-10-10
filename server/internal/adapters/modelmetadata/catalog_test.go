package modelmetadata

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"floe/server/internal/inference"
	"floe/server/internal/modelcatalog"
	"floe/server/internal/trust"
)

// This fixture exercises the user data path from a local catalog file through
// Node's adapter seam into Inference's resolved operator view. It uses only a
// non-routable synthetic identity and never constructs a provider client.
func TestLocalCatalogEvidenceConfiguresInferenceWithoutCodeChanges(t *testing.T) {
	data, err := os.ReadFile(filepath.Join("..", "..", "modelcatalog", "example.json"))
	if err != nil {
		t.Fatalf("read synthetic local data example: %v", err)
	}
	path := filepath.Join(t.TempDir(), modelcatalog.FileName)
	if err := modelcatalog.Install(path, data); err != nil {
		t.Fatalf("install synthetic capability evidence: %v", err)
	}
	store, err := modelcatalog.Open(path)
	if err != nil {
		t.Fatalf("open local model catalog: %v", err)
	}

	service, err := inference.NewService(syntheticTrust{}, NewCatalogSource(store))
	if err != nil {
		t.Fatalf("create inference service: %v", err)
	}
	account := syntheticAccount{}
	if err := service.Configure(inference.InferenceConfig{
		Routes: map[inference.Purpose]inference.PurposeRoute{
			inference.QuickResponse: {TargetID: "fixture", Enabled: true},
		},
	}, map[string]inference.ModelAccount{"fixture": account}, syntheticExecutor{}); err != nil {
		t.Fatalf("configure synthetic model account: %v", err)
	}
	inventory, err := service.Snapshot(context.Background())
	if err != nil {
		t.Fatalf("resolve locally configured capabilities: %v", err)
	}
	quick := inventory.Get(inference.QuickResponse)
	if quick.Status != inference.Available || !reflect.DeepEqual(quick.Capabilities, []string{
		inference.ChatCapability, inference.StructuredOutputCapability, inference.ToolProposalsCapability,
	}) {
		t.Fatalf("local evidence did not configure all three synthetic facts: %#v", quick)
	}
	for _, name := range []string{inference.ChatCapability, inference.StructuredOutputCapability, inference.ToolProposalsCapability} {
		state := quick.CapabilityStates[name]
		if state.Status != inference.CapabilitySupported || state.Provenance == nil || state.Provenance.Source != "synthetic local fixture" {
			t.Fatalf("local data did not retain evidence provenance for %q: %#v", name, state)
		}
	}
}

type syntheticTrust struct{}

func (syntheticTrust) WithCurrentPrincipal(_ trust.Principal, use func(trust.PrincipalSnapshot) error) error {
	return use(trust.PrincipalSnapshot{})
}

func (syntheticTrust) WithCurrentOperator(_ trust.OperatorPrincipal, use func() error) error {
	return use()
}

func (syntheticTrust) ActiveIssuer(trust.Principal) (trust.IssuerSnapshot, error) {
	return trust.IssuerSnapshot{}, nil
}

type syntheticAccount struct{}

func (syntheticAccount) Ready(context.Context) error { return nil }
func (syntheticAccount) ReplayIdentity() string      { return "synthetic-fixture" }
func (syntheticAccount) ProtocolCapabilities() []string {
	return []string{inference.ChatCapability, inference.StructuredOutputCapability, inference.ToolProposalsCapability}
}
func (syntheticAccount) ModelIdentity() inference.ModelIdentity {
	return inference.ModelIdentity{
		ProviderID: "openai_compatible", ModelID: "floe-fixture-chat-tools-json", Endpoint: "https://fixture.invalid/v1",
	}
}
func (syntheticAccount) BudgetOverride() *inference.ModelBudgetOverride { return nil }

type syntheticExecutor struct{}

func (syntheticExecutor) InvokeAgent(context.Context, inference.ResolvedModelTarget, inference.AgentInvocation) (inference.AgentResult, error) {
	return inference.AgentResult{}, nil
}

func (syntheticExecutor) InvokeStructured(context.Context, inference.ResolvedModelTarget, inference.StructuredInvocation) (inference.StructuredResult, error) {
	return inference.StructuredResult{}, nil
}
