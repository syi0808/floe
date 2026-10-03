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

type ProviderTarget struct {
	Provider  string `json:"provider"`
	BaseURL   string `json:"base_url"`
	Model     string `json:"model"`
	APIKeyEnv string `json:"api_key_env,omitempty"`
}
type CodexClient interface {
	Ready() bool
	ReplayIdentity() string
	Generate(context.Context, string, string, string, json.RawMessage, json.RawMessage) (string, error)
}
type Registry struct{ targets map[string]*provider }

func NewRegistry(targets map[string]ProviderTarget, lookup func(string) (string, error), codex CodexClient) (*Registry, error) {
	if len(targets) > 32 || lookup == nil {
		return nil, errors.New("invalid provider config")
	}
	r := &Registry{targets: map[string]*provider{}}
	for id, target := range targets {
		if !inference.ValidAlias(id) {
			return nil, errors.New("invalid target")
		}
		adapter, err := newProvider(target, lookup, codex)
		if err != nil {
			return nil, err
		}
		r.targets[id] = adapter
	}
	return r, nil
}
func (r *Registry) Accounts() map[string]inference.ModelAccount {
	out := map[string]inference.ModelAccount{}
	for k, v := range r.targets {
		out[k] = v
	}
	return out
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
		Target               ProviderTarget
		Credential, Identity string
	}{p.target, p.credential, identity})
	h := sha256.Sum256(raw)
	return hex.EncodeToString(h[:])
}
func (r *Registry) InvokeAgent(ctx context.Context, target inference.ResolvedModelTarget, in inference.AgentInvocation) (inference.AgentResult, error) {
	p := r.targets[target.TargetID()]
	if p == nil || p.ReplayIdentity() != target.AccountIdentity() {
		return inference.AgentResult{}, inference.Failure{Code: inference.CapabilityChanged}
	}
	return p.agent(ctx, in, target.ReasoningEffort())
}
func (r *Registry) InvokeStructured(ctx context.Context, target inference.ResolvedModelTarget, in inference.StructuredInvocation) (inference.StructuredResult, error) {
	p := r.targets[target.TargetID()]
	if p == nil || p.ReplayIdentity() != target.AccountIdentity() {
		return inference.StructuredResult{}, inference.Failure{Code: inference.CapabilityChanged}
	}
	return p.structured(ctx, in, target.ReasoningEffort())
}
func ValidateTarget(target ProviderTarget) error {
	if strings.TrimSpace(target.Model) == "" || len(target.Model) > 128 {
		return errors.New("invalid model")
	}
	_, err := newProvider(target, func(string) (string, error) { return "validation-placeholder", nil }, nil)
	if target.Provider == "codex_oauth" && target.BaseURL == "https://chatgpt.com/backend-api/codex" && target.APIKeyEnv == "" {
		return nil
	}
	return err
}
