package providers

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"floe/server/internal/inference"
	"strings"
)

type CodexClient interface {
	Ready(context.Context) bool
	ReplayIdentity() string
	Generate(context.Context, string, string, string, json.RawMessage, json.RawMessage) (string, error)
	GenerateAgent(context.Context, string, string, string, json.RawMessage, json.RawMessage) (string, inference.UsageObservation, error)
}

type Factory struct {
	lookup func(context.Context, string) (string, error)
	codex  CodexClient
}

func NewFactory(lookup func(context.Context, string) (string, error), codex CodexClient) *Factory {
	return &Factory{lookup: lookup, codex: codex}
}

type registry struct{ targets map[string]*provider }

func (f *Factory) ValidateTarget(target inference.ProviderTarget) error {
	if strings.TrimSpace(target.Model) == "" || len(target.Model) > 128 || target.BudgetOverride != nil && target.BudgetOverride.Validate() != nil {
		return errors.New("invalid model")
	}
	if target.Provider == "codex_oauth" && target.BudgetOverride != nil && target.BudgetOverride.SelectedOutputReservationTokens != nil {
		return errors.New("Codex provider cannot enforce the selected output token reservation")
	}
	_, err := newProvider(context.Background(), target, func(context.Context, string) (string, error) { return "validation-placeholder", nil }, nil)
	if target.Provider == "codex_oauth" && target.BaseURL == "https://chatgpt.com/backend-api/codex" && target.APIKeyEnv == "" {
		return nil
	}
	return err
}

func (f *Factory) Open(ctx context.Context, targets map[string]inference.ProviderTarget) (map[string]inference.ModelAccount, inference.ModelExecutor, error) {
	if f == nil || f.lookup == nil || len(targets) > 32 {
		return nil, nil, errors.New("invalid provider config")
	}
	r := &registry{targets: make(map[string]*provider, len(targets))}
	accounts := make(map[string]inference.ModelAccount, len(targets))
	for id, target := range targets {
		if err := ctx.Err(); err != nil {
			return nil, nil, err
		}
		if !inference.ValidAlias(id) {
			return nil, nil, errors.New("invalid target")
		}
		adapter, err := newProvider(ctx, target, f.lookup, f.codex)
		if err != nil {
			return nil, nil, err
		}
		r.targets[id] = adapter
		accounts[id] = adapter
	}
	return accounts, r, nil
}

func (p *provider) Ready(ctx context.Context) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if p.credentialError != nil {
		return inference.Failure{Code: inference.ProviderCredentialsUnavailable}
	}
	if p.target.APIKeyEnv != "" {
		current, err := p.lookup(ctx, p.target.APIKeyEnv)
		if errors.Is(err, context.DeadlineExceeded) || errors.Is(err, context.Canceled) {
			return err
		}
		if err != nil || current != p.credential {
			return inference.Failure{Code: inference.ProviderCredentialsUnavailable}
		}
	}
	if p.target.Provider == "codex_oauth" {
		if p.codex == nil {
			return inference.Failure{Code: inference.ProviderCredentialsUnavailable}
		}
		ready := p.codex.Ready(ctx)
		if err := ctx.Err(); err != nil {
			return err
		}
		if !ready || p.codex.ReplayIdentity() == "" {
			return inference.Failure{Code: inference.ProviderCredentialsUnavailable}
		}
	}
	return nil
}

func (p *provider) ReplayIdentity() string {
	identity := ""
	if p.codex != nil {
		identity = p.codex.ReplayIdentity()
		if identity == "" {
			return ""
		}
	}
	raw, _ := json.Marshal(struct {
		Target               inference.ProviderTarget
		Credential, Identity string
	}{p.target, p.credential, identity})
	h := sha256.Sum256(raw)
	return hex.EncodeToString(h[:])
}

func (p *provider) ProtocolCapabilities() []string {
	// These are adapter implementation facts only. Model support for chat, JSON,
	// and tool proposals still requires exact metadata evidence in Inference.
	return []string{inference.ChatCapability, inference.StructuredOutputCapability, inference.ToolProposalsCapability}
}

func (p *provider) ModelIdentity() inference.ModelIdentity {
	endpoint, ok := inference.CanonicalModelEndpoint(p.target.BaseURL)
	if !ok {
		return inference.ModelIdentity{}
	}
	return inference.ModelIdentity{ProviderID: p.target.Provider, ModelID: p.target.Model, Endpoint: endpoint}
}

func (p *provider) BudgetOverride() *inference.ModelBudgetOverride {
	return inference.CloneModelBudgetOverride(p.target.BudgetOverride)
}

func (r *registry) InvokeAgent(ctx context.Context, target inference.ResolvedModelTarget, in inference.AgentInvocation) (inference.AgentResult, error) {
	p := r.targets[target.TargetID()]
	if p == nil || p.ReplayIdentity() != target.AccountIdentity() {
		return inference.AgentResult{}, inference.Failure{Code: inference.CapabilityChanged}
	}
	var outputTokenLimit *uint32
	if tokens, ok := target.SelectedOutputReservationTokens(); ok {
		outputTokenLimit = &tokens
	}
	return p.agent(ctx, target.CapabilityStates(), in, target.ReasoningEffort(), outputTokenLimit)
}

func (r *registry) InvokeStructured(ctx context.Context, target inference.ResolvedModelTarget, in inference.StructuredInvocation) (inference.StructuredResult, error) {
	p := r.targets[target.TargetID()]
	if p == nil || p.ReplayIdentity() != target.AccountIdentity() {
		return inference.StructuredResult{}, inference.Failure{Code: inference.CapabilityChanged}
	}
	var outputTokenLimit *uint32
	if tokens, ok := target.SelectedOutputReservationTokens(); ok {
		outputTokenLimit = &tokens
	}
	return p.structured(ctx, target.CapabilityStates(), in, target.ReasoningEffort(), outputTokenLimit)
}
