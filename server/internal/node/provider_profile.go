package node

import (
	"floe/server/internal/inference"
	"floe/server/internal/inference/providers"
	"floe/server/internal/operation"
	httptransport "floe/server/internal/transport/http"
	"strings"
)

const codexEndpoint = "https://chatgpt.com/backend-api/codex"

func profileTargetID(provider, purpose string) string { return "managed_" + provider + "_" + purpose }
func (c *Console) profileTarget(provider, purpose string, p providerProfile) providers.ProviderTarget {
	configured := p.Purposes[purpose]
	return providers.ProviderTarget{Provider: provider, BaseURL: p.BaseURL, Model: configured.Model, APIKeyEnv: p.APIKeyEnv}
}
func (c *Console) configuredTarget(id string) (providers.ProviderTarget, bool) {
	if t, ok := c.state.Targets[id]; ok {
		return t, true
	}
	for provider, p := range c.state.Providers {
		for purpose := range p.Purposes {
			if profileTargetID(provider, purpose) == id {
				return c.profileTarget(provider, purpose, p), true
			}
		}
	}
	return providers.ProviderTarget{}, false
}
func (c *Console) updateProvider(in httptransport.ProviderRequest) operation.Result {
	if in.Purposes == nil || in.Provider != "codex_oauth" && in.Provider != "openai_compatible" || len(in.Purposes) > 3 || len(in.APIKey) > 8192 || strings.ContainsAny(in.APIKey, "\r\n\x00") {
		return operation.Reject(operation.Invalid, "validation")
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	next := cloneState(c.state)
	for p, r := range next.Routes {
		if strings.HasPrefix(r.TargetID, "managed_"+in.Provider+"_") {
			delete(next.Routes, p)
		}
	}
	if len(in.Purposes) == 0 {
		delete(next.Providers, in.Provider)
		return c.commitConfiguration(next)
	}
	if in.Provider == "codex_oauth" {
		in.BaseURL = codexEndpoint
		in.APIKey = ""
	}
	p := providerProfile{BaseURL: in.BaseURL, Purposes: in.Purposes}
	old := next.Providers[in.Provider]
	if in.APIKey != "" {
		p.APIKeyEnv = "FLOE_KEY_" + strings.ToUpper(randomToken())
	} else if old.BaseURL == p.BaseURL {
		p.APIKeyEnv = old.APIKeyEnv
	}
	for purpose, profile := range p.Purposes {
		if !inference.ValidPurpose(purpose) || !inference.ValidEffort(profile.ReasoningEffort) || providers.ValidateTarget(c.profileTarget(in.Provider, purpose, p)) != nil {
			return operation.Reject(operation.Invalid, "invalid_provider_configuration")
		}
		next.Routes[inference.Purpose(purpose)] = inference.PurposeRoute{TargetID: profileTargetID(in.Provider, purpose), ReasoningEffort: profile.ReasoningEffort, Enabled: true}
	}
	next.Providers[in.Provider] = p
	if in.APIKey != "" && c.vault.Put(p.APIKeyEnv, in.APIKey) != nil {
		return operation.Reject(operation.Unavailable, "credential_store_unavailable")
	}
	return c.commitConfiguration(next)
}
