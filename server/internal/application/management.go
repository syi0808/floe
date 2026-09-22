package application

import (
	"context"
	"errors"
	"strings"
	"time"

	"floe/server/internal/authorization"
	"floe/server/internal/inference"
	"floe/server/internal/operation"
	httptransport "floe/server/internal/transport/http"
)

func (console *Console) updateRoute(input httptransport.RouteRequest) (outcome operation.Result) {
	console.mu.Lock()
	defer console.mu.Unlock()
	if !inference.ValidClass(input.Class) {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	next := cloneState(console.state)
	if input.Target == "" {
		delete(next.Routes, input.Class)
	} else {
		next.Routes[input.Class] = inference.Route{Target: input.Target, ReasoningEffort: input.ReasoningEffort}
		if _, err := inference.New(inference.Config{Targets: next.Targets, Routes: next.Routes}, console.internalToken, console.lookup, console.runtime); err != nil {
			outcome = operation.Reject(operation.Invalid, "invalid_route")
			return
		}
	}
	if console.save(next) != nil {
		outcome = operation.Reject(operation.Internal, "save_failed")
		return
	}
	console.state = next
	console.rebuild()
	outcome = operation.Result{Category: operation.Ready, Value: map[string]bool{"ok": true}}
	return
}
func (console *Console) updateTarget(input httptransport.TargetRequest) (outcome operation.Result) {
	console.mu.Lock()
	defer console.mu.Unlock()
	if !identifierPattern.MatchString(input.ID) || len(input.APIKey) > 8192 || strings.ContainsAny(input.APIKey, "\r\n\x00") {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	old := console.state.Targets[input.ID]
	if input.Provider == "codex_oauth" {
		input.BaseURL = "https://chatgpt.com/backend-api/codex"
		input.APIKey = ""
	}
	target := inference.Target{Provider: input.Provider, BaseURL: input.BaseURL, Model: input.Model}
	if input.APIKey != "" {
		target.APIKeyEnv = "FLOE_KEY_" + strings.ToUpper(randomToken())
	} else if old.Provider == target.Provider && old.BaseURL == target.BaseURL {
		target.APIKeyEnv = old.APIKeyEnv
	}
	lookup := console.lookup
	if input.APIKey != "" {
		lookup = func(string) string { return input.APIKey }
	}
	if _, err := inference.New(inference.Config{Targets: map[string]inference.Target{input.ID: target}}, console.internalToken, lookup, console.runtime); err != nil {
		outcome = operation.Reject(operation.Invalid, "invalid_target")
		return
	}
	next := cloneState(console.state)
	next.Targets[input.ID] = target
	if len(next.Targets) > 32 {
		outcome = operation.Reject(operation.Invalid, "too_many_targets")
		return
	}
	if input.APIKey != "" && console.vault.Put(target.APIKeyEnv, input.APIKey) != nil {
		outcome = operation.Reject(operation.Unavailable, "credential_store_unavailable")
		return
	}
	if console.save(next) != nil {
		if input.APIKey != "" {
			_ = console.vault.Delete(target.APIKeyEnv)
		}
		outcome = operation.Reject(operation.Internal, "save_failed")
		return
	}
	console.state = next
	console.rebuild()
	if old.APIKeyEnv != "" && old.APIKeyEnv != target.APIKeyEnv && console.vault.Delete(old.APIKeyEnv) != nil {
		outcome = operation.Reject(operation.Internal, "credential_cleanup_failed")
		return
	}
	outcome = operation.Result{Category: operation.Ready, Value: map[string]bool{"ok": true}}
	return
}
func (console *Console) managementState() (outcome operation.Result) {
	console.mu.Lock()
	state := cloneState(console.state)
	unavailable := make(map[string]bool, len(console.unavailable))
	for identifier, value := range console.unavailable {
		unavailable[identifier] = value
	}
	clients := make([]string, 0, len(state.Clients))
	clientScopes := make(map[string]any, len(state.Clients))
	for identifier := range state.Clients {
		clients = append(clients, identifier)
		client := state.Clients[identifier]
		clientScopes[identifier] = map[string]any{"person_id": client.PersonID, "device_id": client.DeviceID}
	}
	runtime, gateway, address := console.runtime, console.gateway, console.address
	console.mu.Unlock()
	pending := console.pairing.Pending()

	providers := map[string]any{}
	for provider, profile := range state.Providers {
		classes := map[string]any{}
		for class, configured := range profile.Classes {
			identifier := profileTargetID(provider, class)
			available := !unavailable[identifier]
			if provider == "codex_oauth" {
				available = available && runtime != nil && runtime.Ready()
			}
			classes[class] = map[string]any{"model": configured.Model, "reasoning_effort": configured.ReasoningEffort, "active": state.Routes[class].Target == identifier, "available": available}
		}
		providers[provider] = map[string]any{"base_url": profile.BaseURL, "has_credential": profile.APIKeyEnv != "", "classes": classes}
	}
	outcome = operation.Result{Category: operation.Ready, Value: map[string]any{"providers": providers, "clients": clients, "client_scopes": clientScopes, "pairing": pending, "address": "http://" + address, "traces": gateway.Traces(20)}}
	return
}
func (console *Console) deleteClient(clientID string) (outcome operation.Result) {
	console.mu.Lock()
	locked, committed := true, false
	defer func() {
		if locked {
			console.mu.Unlock()
		}
		if committed {
			console.pairing.ClearClient(clientID)
		}
	}()
	next := cloneState(console.state)
	removed, exists := next.Clients[clientID]
	if !exists {
		outcome = operation.Reject(operation.Missing, "client_not_found")
		return
	}
	transientAttempts := console.removeClientAttemptsLocked(&next, clientID)
	delete(next.Clients, clientID)
	for keyID, trusted := range next.TrustedIssuers {
		if trusted.ClientID == clientID {
			delete(next.TrustedIssuers, keyID)
			next.RevokedIssuerKeys[keyID] = true
		}
	}
	transientAttempts = append(transientAttempts, console.removePersonConnectionsLocked(&next, removed.PersonID)...)
	if console.save(next) != nil {
		outcome = operation.Reject(operation.Internal, "save_failed")
		return
	}
	console.state = next
	committed = true
	for _, attemptID := range transientAttempts {
		console.connections.DeleteAttempt(attemptID)
	}
	postDeleteError := ""
	if err := console.rebuildConnectorRuntimes(); err != nil {
		postDeleteError = "invalid_connector_configuration"
	}

	if postDeleteError == "" {
		if err := console.retryPersonCleanupLocked(removed.PersonID); err != nil && !errors.Is(err, errConnectorLifecycleInProgress) {
			postDeleteError = "connection_cleanup_pending"
		}
	}
	if engine := console.authorityEngine(); engine != nil {
		principal := authorization.Principal{ClientID: clientID, PersonID: removed.PersonID, DeviceID: removed.DeviceID, Authenticated: true}
		console.mu.Unlock()
		locked = false
		engine.ForgetPrincipal(principal)
	}
	if postDeleteError != "" {
		outcome = operation.Reject(operation.Internal, postDeleteError)
		return
	}
	return operation.Accept(map[string]bool{"ok": true})
}
func (console *Console) deleteTarget(targetID string) (outcome operation.Result) {
	console.mu.Lock()
	defer console.mu.Unlock()
	next := cloneState(console.state)
	old := next.Targets[targetID]
	delete(next.Targets, targetID)
	for class, route := range next.Routes {
		if route.Target == targetID {
			delete(next.Routes, class)
		}
	}
	if console.save(next) != nil {
		outcome = operation.Reject(operation.Internal, "save_failed")
		return
	}
	console.state = next
	console.rebuild()
	if old.APIKeyEnv != "" && console.vault.Delete(old.APIKeyEnv) != nil {
		outcome = operation.Reject(operation.Internal, "credential_cleanup_failed")
		return
	}
	return operation.Accept(map[string]bool{"ok": true})
}

func (console *Console) codexAction(ctx context.Context, action string) operation.Result {
	if console.runtime == nil {
		return operation.Reject(operation.Unavailable, "codex_unavailable")
	}
	ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	value, err := console.runtime.Action(ctx, action)
	if err != nil {
		return operation.Reject(operation.Upstream, "codex_unavailable")
	}
	return operation.Accept(value)
}
