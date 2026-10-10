package inference

import (
	"context"
	"reflect"
	"testing"

	"floe/server/internal/trust"
)

type selectionTestTrust struct{}

func (selectionTestTrust) WithCurrentPrincipal(_ trust.Principal, consume func(trust.PrincipalSnapshot) error) error {
	return consume(trust.PrincipalSnapshot{})
}

func (selectionTestTrust) WithCurrentOperator(_ trust.OperatorPrincipal, consume func() error) error {
	return consume()
}

func (selectionTestTrust) ActiveIssuer(trust.Principal) (trust.IssuerSnapshot, error) {
	return trust.IssuerSnapshot{}, nil
}

type selectionTestAccount struct {
	model    string
	identity string
	override *ModelBudgetOverride
}

func (account selectionTestAccount) Ready(context.Context) error { return nil }
func (account selectionTestAccount) ReplayIdentity() string      { return account.identity }
func (selectionTestAccount) ProtocolCapabilities() []string {
	return []string{ChatCapability, StructuredOutputCapability, ToolProposalsCapability}
}
func (account selectionTestAccount) ModelIdentity() ModelIdentity {
	return ModelIdentity{ProviderID: "openai_compatible", ModelID: account.model, Endpoint: "https://example.invalid"}
}
func (account selectionTestAccount) BudgetOverride() *ModelBudgetOverride {
	return CloneModelBudgetOverride(account.override)
}

type selectionTestExecutor struct{}

func (selectionTestExecutor) InvokeAgent(context.Context, ResolvedModelTarget, AgentInvocation) (AgentResult, error) {
	return AgentResult{}, nil
}

func (selectionTestExecutor) InvokeStructured(context.Context, ResolvedModelTarget, StructuredInvocation) (StructuredResult, error) {
	return StructuredResult{}, nil
}

func TestCapabilityRevisionChangesWithModelIdentityAndSameBudget(t *testing.T) {
	u32 := func(value uint32) *uint32 { return &value }
	override := &ModelBudgetOverride{
		SchemaVersion:                   ModelBudgetOverrideVersion,
		ContextWindowTokens:             u32(8192),
		MaxOutputTokens:                 u32(2048),
		SelectedOutputReservationTokens: u32(1024),
		ProviderOverheadTokens:          u32(128),
		SafetyMarginTokens:              u32(256),
		MaxInputJSONBytes:               u32(16_384),
	}
	config := InferenceConfig{Routes: map[Purpose]PurposeRoute{
		QuickResponse: {TargetID: "selected", ReasoningEffort: "medium", Enabled: true},
	}}
	service, err := NewService(selectionTestTrust{}, emptyCapabilityMetadata())
	if err != nil {
		t.Fatal(err)
	}
	accountA := selectionTestAccount{model: "model-a", identity: "account-a", override: override}
	if err = service.Configure(config, map[string]ModelAccount{"selected": accountA}, selectionTestExecutor{}); err != nil {
		t.Fatal(err)
	}
	targetA, revisionA, _, err := service.current(context.Background(), QuickResponse)
	if err != nil {
		t.Fatal(err)
	}

	accountB := selectionTestAccount{model: "model-b", identity: "account-b", override: override}
	if err = service.Configure(config, map[string]ModelAccount{"selected": accountB}, selectionTestExecutor{}); err != nil {
		t.Fatal(err)
	}
	targetB, revisionB, _, err := service.current(context.Background(), QuickResponse)
	if err != nil {
		t.Fatal(err)
	}
	if targetA.modelIdentity == targetB.modelIdentity {
		t.Fatalf("selected model identity did not change: %#v", targetA.modelIdentity)
	}
	if !reflect.DeepEqual(targetA.budgetProfile, targetB.budgetProfile) {
		t.Fatal("test setup changed numeric model budget along with model identity")
	}
	if revisionA == revisionB {
		t.Fatal("model selection change retained the old capability revision")
	}
}
