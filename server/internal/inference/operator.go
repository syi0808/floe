package inference

import (
	"context"
	"errors"
	"strings"

	"floe/server/internal/operation"
	"floe/server/internal/trust"
)

type RouteUpdate struct {
	OperationID     string
	Purpose         string
	Enabled         bool
	Target          string
	ReasoningEffort string
}

type TargetUpdate struct {
	OperationID    string
	ID             string
	Provider       string
	BaseURL        string
	Model          string
	APIKey         string
	BudgetOverride *ModelBudgetOverride
}

type ProviderUpdate struct {
	OperationID string
	Provider    string
	BaseURL     string
	APIKey      string
	Purposes    map[string]PurposeModel
}

type OperatorPurposeProfile struct {
	Model            string
	ReasoningEffort  string
	Active           bool
	Available        bool
	CapabilityStates CapabilityStates
	BudgetOverride   *ModelBudgetOverride
}

type OperatorProviderProfile struct {
	BaseURL       string
	HasCredential bool
	Purposes      map[string]OperatorPurposeProfile
}

type OperatorSnapshot struct {
	InventoryAvailable bool
	Profiles           map[string]OperatorProviderProfile
	Inventory          PurposeInventory
}

// UpdateRoute replaces or removes one purpose route after revalidating the
// current operator. The operator lock precedes the configuration and engine
// locks for every configuration mutation.
func (c *Configuration) UpdateRoute(ctx context.Context, operator trust.OperatorPrincipal, input RouteUpdate) error {
	return c.withCurrentOperator(operator, func() error {
		if !trust.ValidID(input.OperationID) {
			return operation.Fail(operation.Invalid, "validation")
		}
		c.mu.Lock()
		defer c.mu.Unlock()
		if result, found := c.operationStatus(ctx, input.OperationID, operationFingerprint(input)); found {
			return result
		}
		if c.configUnavailable {
			return operation.Fail(operation.Unavailable, "configuration_unavailable")
		}
		if !ValidPurpose(input.Purpose) || !ValidEffort(input.ReasoningEffort) {
			return operation.Fail(operation.Invalid, "validation")
		}
		next := cloneConfigurationState(c.state)
		purpose := Purpose(input.Purpose)
		if input.Target == "" {
			delete(next.Routes, purpose)
		} else {
			if _, ok := configuredTarget(next, input.Target); !ok {
				return operation.Fail(operation.Invalid, "invalid_target")
			}
			next.Routes[purpose] = PurposeRoute{TargetID: input.Target, ReasoningEffort: input.ReasoningEffort, Enabled: input.Enabled}
		}
		return c.commitOperation(ctx, input.OperationID, operationFingerprint(input), configurationContent(next))
	})
}

func (c *Configuration) UpdateTarget(ctx context.Context, operator trust.OperatorPrincipal, input TargetUpdate) error {
	return c.withCurrentOperator(operator, func() error {
		if !trust.ValidID(input.OperationID) {
			return operation.Fail(operation.Invalid, "validation")
		}
		fingerprint := operationFingerprint(input)
		c.mu.Lock()
		defer c.mu.Unlock()
		if result, found := c.operationStatus(ctx, input.OperationID, fingerprint); found {
			return result
		}
		if c.configUnavailable {
			return operation.Fail(operation.Unavailable, "configuration_unavailable")
		}
		if !ValidAlias(input.ID) || strings.HasPrefix(input.ID, "managed_") || len(input.APIKey) > 8192 || strings.ContainsAny(input.APIKey, "\r\n\x00") {
			return operation.Fail(operation.Invalid, "validation")
		}
		old := c.state.Targets[input.ID]
		providerName, baseURL, apiKey := input.Provider, input.BaseURL, input.APIKey
		if providerName == "codex_oauth" {
			baseURL = "https://chatgpt.com/backend-api/codex"
			apiKey = ""
		}
		target := ProviderTarget{Provider: providerName, BaseURL: baseURL, Model: input.Model, BudgetOverride: cloneModelBudgetOverride(input.BudgetOverride)}
		if apiKey == "" && old.Provider == target.Provider && old.BaseURL == target.BaseURL {
			target.APIKeyEnv = old.APIKeyEnv
		}
		if target.BudgetOverride != nil && target.BudgetOverride.Validate() != nil || c.factory.ValidateTarget(target) != nil {
			return operation.Fail(operation.Invalid, "invalid_target")
		}
		next := cloneConfigurationState(c.state)
		next.Targets[input.ID] = target
		if len(next.Targets) > 32 {
			return operation.Fail(operation.Limited, "target_limit")
		}
		content := configurationContent(next)
		if apiKey != "" {
			slot, err := c.freshCredentialSlot(ctx)
			if err != nil {
				return operation.Fail(operation.Unavailable, "credential_store_unavailable")
			}
			target.APIKeyEnv = slot
			content.Targets[input.ID] = target
			return c.beginCredentialTransition(ctx, input.OperationID, fingerprint, slot, apiKey, content)
		}
		return c.commitOperation(ctx, input.OperationID, fingerprint, content)
	})
}

func (c *Configuration) UpdateProvider(ctx context.Context, operator trust.OperatorPrincipal, input ProviderUpdate) error {
	return c.withCurrentOperator(operator, func() error {
		if !trust.ValidID(input.OperationID) {
			return operation.Fail(operation.Invalid, "validation")
		}
		fingerprint := operationFingerprint(input)
		c.mu.Lock()
		defer c.mu.Unlock()
		if result, found := c.operationStatus(ctx, input.OperationID, fingerprint); found {
			return result
		}
		if c.configUnavailable {
			return operation.Fail(operation.Unavailable, "configuration_unavailable")
		}
		if input.Purposes == nil || input.Provider != "codex_oauth" && input.Provider != "openai_compatible" || len(input.Purposes) > 3 || len(input.APIKey) > 8192 || strings.ContainsAny(input.APIKey, "\r\n\x00") {
			return operation.Fail(operation.Invalid, "validation")
		}
		next := cloneConfigurationState(c.state)
		for purpose, route := range next.Routes {
			if strings.HasPrefix(route.TargetID, "managed_"+input.Provider+"_") {
				delete(next.Routes, purpose)
			}
		}
		if len(input.Purposes) == 0 {
			delete(next.Providers, input.Provider)
			return c.commitOperation(ctx, input.OperationID, fingerprint, configurationContent(next))
		}
		baseURL, apiKey := input.BaseURL, input.APIKey
		if input.Provider == "codex_oauth" {
			baseURL = "https://chatgpt.com/backend-api/codex"
			apiKey = ""
		}
		profile := ProviderProfile{BaseURL: baseURL, Purposes: clonePurposeModels(input.Purposes)}
		old := next.Providers[input.Provider]
		if apiKey == "" && old.BaseURL == profile.BaseURL {
			profile.APIKeyEnv = old.APIKeyEnv
		}
		for purpose, configured := range profile.Purposes {
			if !ValidPurpose(purpose) || !ValidEffort(configured.ReasoningEffort) || configured.BudgetOverride != nil && configured.BudgetOverride.Validate() != nil || c.factory.ValidateTarget(profileTarget(input.Provider, purpose, profile)) != nil {
				return operation.Fail(operation.Invalid, "invalid_provider_configuration")
			}
			next.Routes[Purpose(purpose)] = PurposeRoute{TargetID: profileTargetID(input.Provider, purpose), ReasoningEffort: configured.ReasoningEffort, Enabled: true}
		}
		if apiKey != "" {
			slot, err := c.freshCredentialSlot(ctx)
			if err != nil {
				return operation.Fail(operation.Unavailable, "credential_store_unavailable")
			}
			profile.APIKeyEnv = slot
			next.Providers[input.Provider] = profile
			return c.beginCredentialTransition(ctx, input.OperationID, fingerprint, slot, apiKey, configurationContent(next))
		}
		next.Providers[input.Provider] = profile
		return c.commitOperation(ctx, input.OperationID, fingerprint, configurationContent(next))
	})
}

func (c *Configuration) DeleteTarget(ctx context.Context, operator trust.OperatorPrincipal, id, operationID string) error {
	return c.withCurrentOperator(operator, func() error {
		if !trust.ValidID(operationID) {
			return operation.Fail(operation.Invalid, "validation")
		}
		fingerprint := operationFingerprint(struct {
			ID string `json:"id"`
		}{id})
		c.mu.Lock()
		defer c.mu.Unlock()
		if result, found := c.operationStatus(ctx, operationID, fingerprint); found {
			return result
		}
		if c.configUnavailable {
			return operation.Fail(operation.Unavailable, "configuration_unavailable")
		}
		if !ValidAlias(id) {
			return operation.Fail(operation.Invalid, "validation")
		}
		next := cloneConfigurationState(c.state)
		delete(next.Targets, id)
		for purpose, route := range next.Routes {
			if route.TargetID == id {
				delete(next.Routes, purpose)
			}
		}
		return c.commitOperation(ctx, operationID, fingerprint, configurationContent(next))
	})
}

func (c *Configuration) Snapshot(ctx context.Context, operator trust.OperatorPrincipal) (OperatorSnapshot, error) {
	if c == nil || c.authority == nil {
		return OperatorSnapshot{}, errors.New("inference configuration unavailable")
	}
	var snapshot OperatorSnapshot
	err := c.authority.WithCurrentOperator(operator, func() error {
		c.mu.Lock()
		defer c.mu.Unlock()
		if c.configUnavailable {
			return operation.Fail(operation.Unavailable, "configuration_unavailable")
		}
		inventory, inventoryErr := c.engine.Snapshot(ctx)
		profiles := make(map[string]OperatorProviderProfile, len(c.state.Providers))
		for provider, configured := range c.state.Providers {
			purposes := make(map[string]OperatorPurposeProfile, len(configured.Purposes))
			for purpose, model := range configured.Purposes {
				targetID := profileTargetID(provider, purpose)
				route := c.state.Routes[Purpose(purpose)]
				available := inventoryErr == nil && inventory.Get(Purpose(purpose)).Status == Available
				capabilityStates, _ := c.engine.CapabilityStatesForTarget(ctx, targetID)
				purposes[purpose] = OperatorPurposeProfile{
					Model:            model.Model,
					ReasoningEffort:  model.ReasoningEffort,
					Active:           route.TargetID == targetID && route.Enabled,
					Available:        available,
					CapabilityStates: capabilityStates,
					BudgetOverride:   cloneModelBudgetOverride(model.BudgetOverride),
				}
			}
			profiles[provider] = OperatorProviderProfile{BaseURL: configured.BaseURL, HasCredential: configured.APIKeyEnv != "", Purposes: purposes}
		}
		snapshot = OperatorSnapshot{Profiles: profiles, Inventory: inventory, InventoryAvailable: inventoryErr == nil}
		return nil
	})
	if err != nil {
		return OperatorSnapshot{}, err
	}
	return snapshot, nil
}

func (c *Configuration) withCurrentOperator(operator trust.OperatorPrincipal, apply func() error) error {
	if c == nil || c.authority == nil {
		return operation.Fail(operation.Unavailable, "configuration_unavailable")
	}
	return operation.Normalize(c.authority.WithCurrentOperator(operator, apply), operation.Unavailable, "operation_unavailable")
}

func configuredTarget(state ConfigState, id string) (ProviderTarget, bool) {
	if target, ok := state.Targets[id]; ok {
		return target, true
	}
	for provider, profile := range state.Providers {
		for purpose := range profile.Purposes {
			if profileTargetID(provider, purpose) == id {
				return profileTarget(provider, purpose, profile), true
			}
		}
	}
	return ProviderTarget{}, false
}

func clonePurposeModels(purposes map[string]PurposeModel) map[string]PurposeModel {
	out := make(map[string]PurposeModel, len(purposes))
	for purpose, configured := range purposes {
		configured.BudgetOverride = cloneModelBudgetOverride(configured.BudgetOverride)
		out[purpose] = configured
	}
	return out
}
