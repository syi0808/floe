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
	Ready() bool
	ReplayIdentity() string
	Generate(context.Context, string, string, string, json.RawMessage, json.RawMessage) (string, error)
}

type Factory struct {
	lookup func(string) (string, error)
	codex  CodexClient
}

func NewFactory(lookup func(string) (string, error), codex CodexClient) *Factory {
	return &Factory{lookup: lookup, codex: codex}
}

type registry struct{ targets map[string]*provider }

func (f *Factory) ValidateTarget(target inference.ProviderTarget) error {
	if strings.TrimSpace(target.Model) == "" || len(target.Model) > 128 {
		return errors.New("invalid model")
	}
	_, err := newProvider(target, func(string) (string, error) { return "validation-placeholder", nil }, nil)
	if target.Provider == "codex_oauth" && target.BaseURL == "https://chatgpt.com/backend-api/codex" && target.APIKeyEnv == "" {
		return nil
	}
	return err
}

func (f *Factory) Open(targets map[string]inference.ProviderTarget) (map[string]inference.ModelAccount, inference.ModelExecutor, error) {
	if f == nil || f.lookup == nil || len(targets) > 32 {
		return nil, nil, errors.New("invalid provider config")
	}
	r := &registry{targets: make(map[string]*provider, len(targets))}
	accounts := make(map[string]inference.ModelAccount, len(targets))
	for id, target := range targets {
		if !inference.ValidAlias(id) {
			return nil, nil, errors.New("invalid target")
		}
		adapter, err := newProvider(target, f.lookup, f.codex)
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
		current, err := p.lookup(p.target.APIKeyEnv)
		if err != nil || current != p.credential {
			return inference.Failure{Code: inference.ProviderCredentialsUnavailable}
		}
	}
	if p.target.Provider == "codex_oauth" && (p.codex == nil || !p.codex.Ready() || p.codex.ReplayIdentity() == "") {
		return inference.Failure{Code: inference.ProviderCredentialsUnavailable}
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

func (r *registry) InvokeAgent(ctx context.Context, target inference.ResolvedModelTarget, in inference.AgentInvocation) (inference.AgentResult, error) {
	p := r.targets[target.TargetID()]
	if p == nil || p.ReplayIdentity() != target.AccountIdentity() {
		return inference.AgentResult{}, inference.Failure{Code: inference.CapabilityChanged}
	}
	return p.agent(ctx, in, target.ReasoningEffort())
}

func (r *registry) InvokeStructured(ctx context.Context, target inference.ResolvedModelTarget, in inference.StructuredInvocation) (inference.StructuredResult, error) {
	p := r.targets[target.TargetID()]
	if p == nil || p.ReplayIdentity() != target.AccountIdentity() {
		return inference.StructuredResult{}, inference.Failure{Code: inference.CapabilityChanged}
	}
	return p.structured(ctx, in, target.ReasoningEffort())
}
