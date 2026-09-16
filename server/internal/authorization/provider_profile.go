package authorization

import (
	"net/http"
	"strings"

	"floe/server/internal/inference"
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
