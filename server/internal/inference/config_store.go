package inference

import "errors"

// ProviderProfile is the Inference-owned persisted provider configuration.
type ProviderProfile struct {
	BaseURL   string                  `json:"base_url"`
	APIKeyEnv string                  `json:"api_key_env,omitempty"`
	Purposes  map[string]PurposeModel `json:"purposes"`
}

// ConfigState is the complete Inference persistence unit. A repository commit
// replaces it atomically; Inference validates and adopts it only after commit.
type ConfigState struct {
	SchemaVersion int                        `json:"schema_version"`
	Targets       map[string]ProviderTarget  `json:"targets"`
	Routes        map[Purpose]PurposeRoute   `json:"routes"`
	Providers     map[string]ProviderProfile `json:"providers"`
}

func emptyConfigurationState() ConfigState {
	return ConfigState{
		SchemaVersion: 1,
		Targets:       map[string]ProviderTarget{},
		Routes:        map[Purpose]PurposeRoute{},
		Providers:     map[string]ProviderProfile{},
	}
}

func validateConfigurationState(state ConfigState, factory ProviderFactory) error {
	if factory == nil || state.SchemaVersion != 1 || state.Targets == nil || state.Routes == nil || state.Providers == nil ||
		len(state.Targets) > 32 || len(state.Providers) > 3 || ValidateConfig(InferenceConfig{Routes: state.Routes}) != nil {
		return errors.New("inference configuration unavailable")
	}
	for _, target := range state.Targets {
		if factory.ValidateTarget(target) != nil {
			return errors.New("invalid inference target")
		}
	}
	for _, profile := range state.Providers {
		if profile.Purposes == nil || len(profile.Purposes) > 3 {
			return errors.New("invalid provider configuration")
		}
		for purpose := range profile.Purposes {
			if !ValidPurpose(purpose) {
				return errors.New("invalid purpose")
			}
		}
	}
	return nil
}

func cloneConfigurationState(state ConfigState) ConfigState {
	out := ConfigState{
		SchemaVersion: state.SchemaVersion,
		Targets:       make(map[string]ProviderTarget, len(state.Targets)),
		Routes:        make(map[Purpose]PurposeRoute, len(state.Routes)),
		Providers:     make(map[string]ProviderProfile, len(state.Providers)),
	}
	for id, target := range state.Targets {
		target.Capabilities = append([]string(nil), target.Capabilities...)
		target.BudgetOverride = cloneModelBudgetOverride(target.BudgetOverride)
		out.Targets[id] = target
	}
	for purpose, route := range state.Routes {
		out.Routes[purpose] = route
	}
	for name, profile := range state.Providers {
		copy := profile
		copy.Purposes = make(map[string]PurposeModel, len(profile.Purposes))
		for purpose, configured := range profile.Purposes {
			configured.Capabilities = append([]string(nil), configured.Capabilities...)
			configured.BudgetOverride = cloneModelBudgetOverride(configured.BudgetOverride)
			copy.Purposes[purpose] = configured
		}
		out.Providers[name] = copy
	}
	return out
}
