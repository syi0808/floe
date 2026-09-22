package application

import (
	"strings"

	"floe/server/internal/inference"
	"floe/server/internal/operation"
	httptransport "floe/server/internal/transport/http"
)

const codexEndpoint = "https://chatgpt.com/backend-api/codex"

func profileTargetID(provider, class string) string {
	return "managed_" + provider + "_" + class
}

func supportedProvider(provider string) bool {
	return provider == "codex_oauth" || provider == "openai_compatible"
}

func (console *Console) profileTarget(provider, class string, profile providerProfile) inference.Target {
	configured := profile.Classes[class]
	return inference.Target{Provider: provider, BaseURL: profile.BaseURL, Model: configured.Model, APIKeyEnv: profile.APIKeyEnv}
}

func (console *Console) configuredTarget(identifier string) (inference.Target, bool) {
	if target, exists := console.state.Targets[identifier]; exists {
		return target, true
	}
	for provider, profile := range console.state.Providers {
		for class := range profile.Classes {
			if profileTargetID(provider, class) == identifier {
				return console.profileTarget(provider, class, profile), true
			}
		}
	}
	return inference.Target{}, false
}

func credentialReferenced(state diskState, name string) bool {
	if name == "" {
		return false
	}
	for _, target := range state.Targets {
		if target.APIKeyEnv == name {
			return true
		}
	}
	for _, profile := range state.Providers {
		if profile.APIKeyEnv == name {
			return true
		}
	}
	return false
}

func (console *Console) updateProvider(input httptransport.ProviderRequest) (outcome operation.Result) {
	console.mu.Lock()
	defer console.mu.Unlock()
	if !supportedProvider(input.Provider) || len(input.Classes) > 3 || len(input.APIKey) > 8192 || strings.ContainsAny(input.APIKey, "\r\n\x00") {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	old := console.state.Providers[input.Provider]
	if input.Provider == "codex_oauth" {
		input.BaseURL = codexEndpoint
		input.APIKey = ""
	}
	if len(input.Classes) == 0 {
		next := cloneState(console.state)
		delete(next.Providers, input.Provider)
		for class, route := range next.Routes {
			if strings.HasPrefix(route.Target, "managed_"+input.Provider+"_") {
				delete(next.Routes, class)
			}
		}
		if console.save(next) != nil {
			outcome = operation.Reject(operation.Internal, "save_failed")
			return
		}
		console.state = next
		console.rebuild()
		if old.APIKeyEnv != "" && !credentialReferenced(next, old.APIKeyEnv) {
			_ = console.vault.Delete(old.APIKeyEnv)
		}
		outcome = operation.Result{Category: operation.Ready, Value: map[string]bool{"ok": true}}
		return
	}
	profile := providerProfile{BaseURL: input.BaseURL, Classes: input.Classes}
	if input.APIKey != "" {
		profile.APIKeyEnv = "FLOE_KEY_" + strings.ToUpper(randomToken())
	} else if old.BaseURL == profile.BaseURL {
		profile.APIKeyEnv = old.APIKeyEnv
	}
	targets := map[string]inference.Target{}
	routes := map[string]inference.Route{}
	for class, configured := range profile.Classes {
		if !inference.ValidClass(class) || strings.TrimSpace(configured.Model) == "" {
			outcome = operation.Reject(operation.Invalid, "validation")
			return
		}
		identifier := profileTargetID(input.Provider, class)
		targets[identifier] = console.profileTarget(input.Provider, class, profile)
		routes[class] = inference.Route{Target: identifier, ReasoningEffort: configured.ReasoningEffort}
	}
	lookup := console.lookup
	if input.APIKey != "" {
		lookup = func(string) string { return input.APIKey }
	}
	if _, err := inference.New(inference.Config{Targets: targets, Routes: routes}, console.internalToken, lookup, console.runtime); err != nil {
		outcome = operation.Reject(operation.Invalid, "invalid_provider_configuration")
		return
	}
	next := cloneState(console.state)
	next.Providers[input.Provider] = profile
	configuredTargets := len(next.Targets)
	for _, configuredProfile := range next.Providers {
		configuredTargets += len(configuredProfile.Classes)
	}
	if configuredTargets > 32 {
		outcome = operation.Reject(operation.Invalid, "too_many_models")
		return
	}
	for class, route := range next.Routes {
		if strings.HasPrefix(route.Target, "managed_"+input.Provider+"_") {
			delete(next.Routes, class)
		}
	}
	for class, route := range routes {
		next.Routes[class] = route
	}
	if input.APIKey != "" && console.vault.Put(profile.APIKeyEnv, input.APIKey) != nil {
		outcome = operation.Reject(operation.Unavailable, "credential_store_unavailable")
		return
	}
	if console.save(next) != nil {
		if input.APIKey != "" {
			_ = console.vault.Delete(profile.APIKeyEnv)
		}
		outcome = operation.Reject(operation.Internal, "save_failed")
		return
	}
	console.state = next
	console.rebuild()
	if old.APIKeyEnv != "" && old.APIKeyEnv != profile.APIKeyEnv && !credentialReferenced(next, old.APIKeyEnv) {
		_ = console.vault.Delete(old.APIKeyEnv)
	}
	outcome = operation.Result{Category: operation.Ready, Value: map[string]bool{"ok": true}}
	return
}
