package inference

import (
	"context"
	"errors"
	"sync"
)

type ProviderTarget struct {
	Provider       string               `json:"provider"`
	BaseURL        string               `json:"base_url"`
	Model          string               `json:"model"`
	APIKeyEnv      string               `json:"api_key_env,omitempty"`
	Capabilities   []string             `json:"capabilities"`
	BudgetOverride *ModelBudgetOverride `json:"budget_override,omitempty"`
}

// ProviderFactory opens local provider accounts and supplies the executor used by
// the inference service. It does not dispatch inference requests.
type ProviderFactory interface {
	ValidateTarget(ProviderTarget) error
	Open(context.Context, map[string]ProviderTarget) (map[string]ModelAccount, ModelExecutor, error)
}

type Configuration struct {
	mu                sync.Mutex
	repository        ConfigRepository
	engine            *Service
	authority         Trust
	credentials       ProviderCredentialAccess
	factory           ProviderFactory
	state             ConfigState
	configUnavailable bool
}

func OpenConfiguration(ctx context.Context, repository ConfigRepository, engine *Service, authority Trust, credentials ProviderCredentialAccess, factory ProviderFactory) (*Configuration, error) {
	if repository == nil || engine == nil || authority == nil || credentials == nil || factory == nil {
		return nil, errors.New("inference configuration dependencies required")
	}
	read := repository.LoadConfig()
	var state ConfigState
	switch read.Disposition {
	case ConfigReadAbsent:
		state = emptyConfigurationState()
	case ConfigReadPresent:
		if err := validateConfigurationState(read.State, factory); err != nil {
			return nil, err
		}
		state = cloneConfigurationState(read.State)
	default:
		return nil, errors.New("inference configuration unavailable")
	}
	configuration := &Configuration{
		repository:  repository,
		engine:      engine,
		authority:   authority,
		credentials: credentials,
		factory:     factory,
		state:       state,
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

func (c *Configuration) prepare(ctx context.Context, state ConfigState) (InferenceConfig, map[string]ModelAccount, ModelExecutor, error) {
	config := InferenceConfig{Routes: cloneConfigurationState(state).Routes}
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
		if accounts[route.TargetID] == nil || !ValidCapabilities(accounts[route.TargetID].Capabilities()) {
			return errors.New("configured target missing")
		}
	}
	return nil
}

func (c *Configuration) open(ctx context.Context, state ConfigState) (map[string]ModelAccount, ModelExecutor, error) {
	targets := make(map[string]ProviderTarget, len(state.Targets)+9)
	for id, target := range state.Targets {
		target.Capabilities = append([]string(nil), target.Capabilities...)
		target.BudgetOverride = cloneModelBudgetOverride(target.BudgetOverride)
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

func profileTarget(provider, purpose string, profile ProviderProfile) ProviderTarget {
	configured := profile.Purposes[purpose]
	return ProviderTarget{Provider: provider, BaseURL: profile.BaseURL, Model: configured.Model, APIKeyEnv: profile.APIKeyEnv, Capabilities: append([]string(nil), configured.Capabilities...), BudgetOverride: cloneModelBudgetOverride(configured.BudgetOverride)}
}

func (c *Configuration) RequiredError() error {
	if c == nil {
		return errors.New("inference configuration unavailable")
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.configUnavailable || c.repository.Health() != ConfigRepositoryReady {
		return errors.New("inference persistence uncertain")
	}
	return nil
}

func (c *Configuration) save(state ConfigState) error {
	outcome := c.repository.SaveConfig(cloneConfigurationState(state))
	switch outcome.Disposition {
	case ConfigWriteCommitted:
		return nil
	case ConfigWriteIndeterminate, ConfigWriteIntegrityFailure:
		c.configUnavailable = true
		c.engine.DenyConfiguration()
	}
	if outcome.Cause != nil {
		return outcome.Cause
	}
	return errors.New("inference configuration unavailable")
}
