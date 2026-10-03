package node

import (
	"context"
	"floe/server/internal/inference"
	"floe/server/internal/inference/providers"
	"floe/server/internal/operation"
	httptransport "floe/server/internal/transport/http"
	"floe/server/internal/trust"
	"strings"
	"time"
)

func (c *Console) updateRoute(in httptransport.RouteRequest) operation.Result {
	if !inference.ValidPurpose(in.Purpose) || !inference.ValidEffort(in.ReasoningEffort) {
		return operation.Reject(operation.Invalid, "validation")
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	next := cloneState(c.state)
	purpose := inference.Purpose(in.Purpose)
	if in.Target == "" {
		delete(next.Routes, purpose)
	} else {
		if _, ok := c.configuredTarget(in.Target); !ok {
			return operation.Reject(operation.Invalid, "invalid_target")
		}
		next.Routes[purpose] = inference.PurposeRoute{TargetID: in.Target, ReasoningEffort: in.ReasoningEffort, Enabled: in.Enabled}
	}
	return c.commitConfiguration(next)
}
func (c *Console) commitConfiguration(st diskState) operation.Result {
	if c.configUnavailable || c.save(st) != nil {
		return operation.Reject(operation.Unavailable, "configuration_unavailable")
	}
	c.state = st
	if c.rebuild() != nil {
		return operation.Reject(operation.Unavailable, "configuration_unavailable")
	}
	return operation.Accept(map[string]bool{"ok": true})
}
func (c *Console) updateTarget(in httptransport.TargetRequest) operation.Result {
	if !inference.ValidAlias(in.ID) || len(in.APIKey) > 8192 || strings.ContainsAny(in.APIKey, "\r\n\x00") {
		return operation.Reject(operation.Invalid, "validation")
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	old := c.state.Targets[in.ID]
	if in.Provider == "codex_oauth" {
		in.BaseURL = codexEndpoint
		in.APIKey = ""
	}
	target := providers.ProviderTarget{Provider: in.Provider, BaseURL: in.BaseURL, Model: in.Model}
	if in.APIKey != "" {
		target.APIKeyEnv = "FLOE_KEY_" + strings.ToUpper(randomToken())
	} else if old.Provider == target.Provider && old.BaseURL == target.BaseURL {
		target.APIKeyEnv = old.APIKeyEnv
	}
	if providers.ValidateTarget(target) != nil {
		return operation.Reject(operation.Invalid, "invalid_target")
	}
	next := cloneState(c.state)
	next.Targets[in.ID] = target
	if len(next.Targets) > 32 {
		return operation.Reject(operation.Limited, "target_limit")
	}
	if in.APIKey != "" && c.vault.Put(target.APIKeyEnv, in.APIKey) != nil {
		return operation.Reject(operation.Unavailable, "credential_store_unavailable")
	}
	return c.commitConfiguration(next)
}
func (c *Console) managementState(operator trust.OperatorPrincipal) operation.Result {
	c.mu.Lock()
	state := cloneState(c.state)
	c.mu.Unlock()
	clients, err := c.trust.Clients()
	if err != nil {
		return trust.Result(err)
	}
	ids := []string{}
	scopes := map[string]any{}
	for _, client := range clients {
		ids = append(ids, client.ClientID)
		scopes[client.ClientID] = map[string]string{"person_id": client.PersonID, "device_id": client.DeviceID}
	}
	profiles := map[string]any{}
	inventory, inventoryErr := c.gateway.Snapshot(context.Background())
	for provider, p := range state.Providers {
		purposes := map[string]any{}
		for purpose, configured := range p.Purposes {
			id := profileTargetID(provider, purpose)
			route := state.Routes[inference.Purpose(purpose)]
			available := inventoryErr == nil && inventory.Get(inference.Purpose(purpose)).Status == inference.Available
			purposes[purpose] = map[string]any{"model": configured.Model, "reasoning_effort": configured.ReasoningEffort, "active": route.TargetID == id && route.Enabled, "available": available}
		}
		profiles[provider] = map[string]any{"base_url": p.BaseURL, "has_credential": p.APIKeyEnv != "", "purposes": purposes}
	}
	traces, err := c.gateway.Traces(operator, 20)
	if err != nil {
		return operation.Reject(operation.Unauthenticated, "unauthorized")
	}
	return operation.Accept(map[string]any{"providers": profiles, "clients": ids, "client_scopes": scopes, "pairing": c.pairing.Pending(), "address": "http://" + c.address, "traces": traces, "inventory": inventory})
}
func (c *Console) deleteClient(id string) operation.Result {
	receipt, err := c.trust.RevokeClient(context.Background(), id)
	if err != nil {
		return trust.Result(err)
	}
	c.pairing.ClearClient(id)
	if err = c.integrations.ApplyRevocation(context.Background(), receipt.Cleanup); err != nil {
		return operation.Reject(operation.Unavailable, "connection_cleanup_pending")
	}
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), 40*time.Second)
		defer cancel()
		_ = c.integrations.ResumeCleanup(ctx)
	}()
	return operation.Accept(map[string]any{"ok": true, "cleanup": "pending"})
}
func (c *Console) deleteTarget(id string) operation.Result {
	c.mu.Lock()
	defer c.mu.Unlock()
	next := cloneState(c.state)
	delete(next.Targets, id)
	for p, r := range next.Routes {
		if r.TargetID == id {
			delete(next.Routes, p)
		}
	}
	return c.commitConfiguration(next)
}
func (c *Console) codexAction(ctx context.Context, action string) operation.Result {
	if c.runtime == nil {
		return operation.Reject(operation.Unavailable, "codex_unavailable")
	}
	if action != "login" && action != "logout" && action != "status" && action != "cancel" {
		return operation.Reject(operation.Missing, "not_found")
	}
	ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	value, err := c.runtime.Action(ctx, action)
	if err != nil {
		return operation.Reject(operation.Unavailable, "codex_unavailable")
	}
	return operation.Accept(value)
}
