package inference

import (
	"context"
	"errors"
	"sync"

	"floe/server/internal/credentials"
	"floe/server/internal/storage"
)

type ProviderTarget struct {
	Provider  string `json:"provider"`
	BaseURL   string `json:"base_url"`
	Model     string `json:"model"`
	APIKeyEnv string `json:"api_key_env,omitempty"`
}

// ProviderFactory opens local provider accounts and supplies the executor used by
// the inference service. It does not dispatch inference requests.
type ProviderFactory interface {
	ValidateTarget(ProviderTarget) error
	Open(context.Context, map[string]ProviderTarget) (map[string]ModelAccount, ModelExecutor, error)
}

type Configuration struct {
	mu                sync.Mutex
	directory         string
	engine            *Service
	authority         Trust
	vault             credentials.Store
	factory           ProviderFactory
	state             configurationState
	configUnavailable bool
}

func OpenConfiguration(ctx context.Context, directory string, engine *Service, authority Trust, vault credentials.Store, factory ProviderFactory) (*Configuration, error) {
	if directory == "" || engine == nil || authority == nil || vault == nil || factory == nil {
		return nil, errors.New("inference configuration dependencies required")
	}
	state, err := readConfigurationState(directory, factory)
	if err != nil {
		return nil, err
	}
	configuration := &Configuration{
		directory: directory,
		engine:    engine,
		authority: authority,
		vault:     vault,
		factory:   factory,
		state:     state,
	}
	config, accounts, executor, err := configuration.prepare(ctx, state)
	if err != nil {
		return nil, errors.New("inference configuration unavailable")
	}
	if err = engine.Configure(config, accounts, executor); err != nil {
		return nil, errors.New("inference configuration unavailable")
	}
	return configuration, nil
}

func (c *Configuration) prepare(ctx context.Context, state configurationState) (InferenceConfig, map[string]ModelAccount, ModelExecutor, error) {
	config := InferenceConfig{Routes: state.Routes}
	accounts, executor, err := c.open(ctx, state)
	if err != nil || validatePreparedConfiguration(config, accounts, executor) != nil {
		return InferenceConfig{}, nil, nil, errors.New("invalid inference configuration")
	}
	return config, accounts, executor, nil
}

func validatePreparedConfiguration(config InferenceConfig, accounts map[string]ModelAccount, executor ModelExecutor) error {
	if ValidateConfig(config) != nil || executor == nil {
		return errors.New("invalid inference configuration")
	}
	for _, route := range config.Routes {
		if accounts[route.TargetID] == nil {
			return errors.New("configured target missing")
		}
	}
	return nil
}

func (c *Configuration) open(ctx context.Context, state configurationState) (map[string]ModelAccount, ModelExecutor, error) {
	targets := make(map[string]ProviderTarget, len(state.Targets)+9)
	for id, target := range state.Targets {
		targets[id] = target
	}
	for provider, profile := range state.Providers {
		for purpose := range profile.Purposes {
			id := profileTargetID(provider, purpose)
			if _, exists := targets[id]; exists {
				return nil, nil, errors.New("duplicate inference target")
			}
			targets[id] = profileTarget(provider, purpose, profile)
		}
	}
	if len(targets) > 32 {
		return nil, nil, errors.New("invalid provider configuration")
	}
	for id, target := range targets {
		if !ValidAlias(id) || c.factory.ValidateTarget(target) != nil {
			return nil, nil, errors.New("invalid inference target")
		}
	}
	return c.factory.Open(ctx, targets)
}

func profileTargetID(provider, purpose string) string {
	return "managed_" + provider + "_" + purpose
}

func profileTarget(provider, purpose string, profile providerProfile) ProviderTarget {
	configured := profile.Purposes[purpose]
	return ProviderTarget{Provider: provider, BaseURL: profile.BaseURL, Model: configured.Model, APIKeyEnv: profile.APIKeyEnv}
}

func (c *Configuration) RequiredError() error {
	if c == nil {
		return errors.New("inference configuration unavailable")
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.configUnavailable {
		return errors.New("inference persistence uncertain")
	}
	return nil
}

func (c *Configuration) save(state configurationState) error {
	err := writeConfigurationState(c.directory, state)
	if storage.IsIndeterminate(err) {
		c.configUnavailable = true
		c.engine.DenyConfiguration()
	}
	return err
}
