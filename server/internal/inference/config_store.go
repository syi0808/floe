package inference

import (
	"encoding/json"
	"errors"
	"os"

	"floe/server/internal/storage"
	"floe/server/internal/trust"
)

const configurationFileName = "inference.json"

type providerProfile struct {
	BaseURL   string                  `json:"base_url"`
	APIKeyEnv string                  `json:"api_key_env,omitempty"`
	Purposes  map[string]PurposeModel `json:"purposes"`
}

type configurationState struct {
	SchemaVersion int                        `json:"schema_version"`
	Targets       map[string]ProviderTarget  `json:"targets"`
	Routes        map[Purpose]PurposeRoute   `json:"routes"`
	Providers     map[string]providerProfile `json:"providers"`
}

func emptyConfigurationState() configurationState {
	return configurationState{
		SchemaVersion: 1,
		Targets:       map[string]ProviderTarget{},
		Routes:        map[Purpose]PurposeRoute{},
		Providers:     map[string]providerProfile{},
	}
}

func readConfigurationState(files *storage.Files, factory ProviderFactory) (configurationState, error) {
	state := emptyConfigurationState()
	data, err := files.Read(configurationFileName, 65536)
	if os.IsNotExist(err) {
		return state, nil
	}
	if err != nil || trust.DecodeStrict(data, &state, 65536, 32) != nil ||
		state.SchemaVersion != 1 || state.Targets == nil || state.Routes == nil || state.Providers == nil ||
		len(state.Targets) > 32 || len(state.Providers) > 3 || ValidateConfig(InferenceConfig{Routes: state.Routes}) != nil {
		return state, errors.New("inference configuration unavailable")
	}
	for _, target := range state.Targets {
		if factory.ValidateTarget(target) != nil {
			return state, errors.New("invalid inference target")
		}
	}
	for _, profile := range state.Providers {
		if profile.Purposes == nil || len(profile.Purposes) > 3 {
			return state, errors.New("invalid provider configuration")
		}
		for purpose := range profile.Purposes {
			if !ValidPurpose(purpose) {
				return state, errors.New("invalid purpose")
			}
		}
	}
	return state, nil
}

func cloneConfigurationState(state configurationState) configurationState {
	out := configurationState{
		SchemaVersion: state.SchemaVersion,
		Targets:       make(map[string]ProviderTarget, len(state.Targets)),
		Routes:        make(map[Purpose]PurposeRoute, len(state.Routes)),
		Providers:     make(map[string]providerProfile, len(state.Providers)),
	}
	for id, target := range state.Targets {
		target.Capabilities = append([]string(nil), target.Capabilities...)
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
			copy.Purposes[purpose] = configured
		}
		out.Providers[name] = copy
	}
	return out
}

func writeConfigurationState(files *storage.Files, state configurationState) error {
	data, err := json.Marshal(state)
	if err != nil {
		return err
	}
	return files.Write(configurationFileName, data)
}
