package application

import (
	"context"
	"reflect"
	"time"

	"floe/server/internal/connections"
	"floe/server/internal/credentials"
	"floe/server/internal/operation"
)

func (console *Console) connectorCatalog(scope connections.Scope) (outcome operation.Result) {
	console.mu.Lock()
	defer console.mu.Unlock()
	items := make([]any, 0, len(connections.Definitions))
	for _, definition := range connections.Definitions {
		connection, connected := console.connectionForPerson(definition.ID, scope.PersonID)
		status := "disconnected"
		if connected && console.connectionCredentialReady(connection) {
			status = "connected"
		} else if connected {
			status = "error"
		}
		var latest *connections.Attempt
		for _, attempt := range console.connections.Attempts() {
			if attempt.ConnectorID == definition.ID && attempt.ClientID == scope.ClientID && (latest == nil || attempt.CreatedAt.After(latest.CreatedAt)) {
				latest = attempt
			}
		}
		if latest != nil && latest.Status == "pending" {
			if time.Since(latest.CreatedAt) <= 10*time.Minute {
				status = "connecting"
			} else {
				status = "error"
			}
		} else if latest != nil && latest.Status == "failed" {
			status = "error"
		}
		available := console.connectorAvailable(definition)
		if !available {
			status = "unavailable"
		}
		item := map[string]any{
			"id": definition.ID, "name": definition.Name, "auth_kind": definition.AuthKind,
			"available": available, "status": status, "required_scopes": definition.RequiredScopes,
			"scope_fields": definition.ScopeFields, "capabilities": connections.ConnectorCapabilities(definition),
		}
		if connected {
			item["connection_id"] = connection.ConnectionID
			item["connection_revision"] = connection.Revision
			item["incarnation"] = connection.Incarnation
			item["epoch"] = connection.Epoch
			item["execution_owner"] = console.state.ExecutionOwnerID
			item["identity_unverified"] = connection.IdentityUnverified
			item["scope"] = connections.CloneConnectorScope(connection.Scope)
		}
		items = append(items, item)
	}
	outcome = operation.Result{Category: operation.Ready, Value: map[string]any{"schema_version": 1, "person_id": scope.PersonID, "device_id": scope.DeviceID, "connectors": items}}
	return
}
func (console *Console) startConnector(ctx context.Context, scope connections.Scope, definition connections.Definition, input connections.ConnectRequest) (outcome operation.Result) {
	unlockLifecycle := console.lockConnectorLifecycle(definition.ID)
	defer unlockLifecycle()
	if !console.requireCurrentClientScope(&outcome, scope) {
		return
	}
	if input.SchemaVersion != 1 || connections.InvalidConnectorToken(input.Secret) {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	if connections.IsOAuthAuthKind(definition.AuthKind) && input.Secret != "" || definition.AuthKind == "secret" && input.Secret == "" {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	selectedScope, err := connections.ValidatedConnectorScope(definition, input.Scope)
	if err != nil {
		outcome = operation.Reject(operation.Invalid, "invalid_scope")
		return
	}
	console.mu.Lock()
	if !console.requireCurrentClientScopeLocked(&outcome, scope) {
		console.mu.Unlock()
		return
	}
	if existing, exists := console.connectionForPerson(definition.ID, scope.PersonID); exists {
		if console.connectionCredentialReady(existing) {
			console.mu.Unlock()
			outcome = operation.Reject(operation.Conflict, "already_connected")
			return
		}
		if connections.IsOAuthAuthKind(definition.AuthKind) && existing.Credential != "" {
			if console.vault.Delete(existing.Credential) != nil {
				console.mu.Unlock()
				outcome = operation.Reject(operation.Unavailable, "credential_cleanup_failed")
				return
			}
		}
		previous := cloneState(console.state)
		delete(console.state.Connections, existing.ConnectionID)
		if console.rebuildConnectorRuntimes() != nil || console.save(console.state) != nil {
			console.state = previous
			_ = console.rebuildConnectorRuntimes()
			console.mu.Unlock()
			outcome = operation.Reject(operation.Internal, "save_failed")
			return
		}
	}
	if console.connectionExistsForOtherPerson(definition.ID, scope.PersonID) {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Denied, "connection_owned_by_another_person")
		return
	}
	if connections.IsCalendarConnector(definition.ID) && console.calendarConnectionExistsForPerson(scope.PersonID, definition.ID) {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "calendar_connection_exists")
		return
	}
	if !console.connectorAvailable(definition) {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Unavailable, "connector_unavailable")
		return
	}
	for _, attempt := range console.connections.Attempts() {
		if attempt.ConnectorID == definition.ID && attempt.PersonID == scope.PersonID {
			console.mu.Unlock()
			outcome = operation.Reject(operation.Conflict, "connection_in_progress")
			return
		}
	}
	connectionID, err := newConnectionID()
	if err != nil {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Internal, "connection_identity_unavailable")
		return
	}
	incarnation, err := newConnectionID()
	if err != nil {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Internal, "connection_identity_unavailable")
		return
	}
	record := connections.Record{ConnectionID: connectionID, Revision: 1, ConnectorID: definition.ID, PersonID: scope.PersonID, Scope: selectedScope, Incarnation: incarnation, Epoch: 1, IdentityUnverified: true}
	credentialNamespace := definition.CredentialName
	if credentialNamespace == "" {
		credentialNamespace = definition.OAuthCredential
	}
	if credentialNamespace != "" {
		credentialName, err := credentials.ConnectionName(credentialNamespace, connectionID, scope.PersonID)
		if err != nil {
			console.mu.Unlock()
			outcome = operation.Reject(operation.Unavailable, "credential_scope_unavailable")
			return
		}
		record.Credential = credentialName
	}
	previous := cloneState(console.state)
	if definition.AuthKind == "secret" {
		if console.vault.Put(record.Credential, input.Secret) != nil {
			console.state = previous
			console.mu.Unlock()
			outcome = operation.Reject(operation.Unavailable, "credential_store_unavailable")
			return
		}
		console.state.Connections[connectionID] = record
		if err := console.rebuildConnectorRuntimes(); err != nil || console.save(console.state) != nil {
			_ = console.vault.Delete(record.Credential)
			console.state = previous
			_ = console.rebuildConnectorRuntimes()
			console.mu.Unlock()
			outcome = operation.Reject(operation.Internal, "save_failed")
			return
		}
		attempt := console.newConnectorAttempt(definition.ID, connectionID, scope, "connected", "")
		responseAttempt := *attempt
		console.mu.Unlock()
		outcome = operation.Result{Category: operation.Created, Value: connections.ConnectorAttemptResponse(&responseAttempt)}
		return
	}
	runtime := console.connectorOAuthRuntime(definition.ID)
	if err := bindClientOAuthCredential(runtime, record); err != nil {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Unavailable, "credential_scope_unavailable")
		return
	}
	attempt := console.newConnectorAttempt(definition.ID, connectionID, scope, "pending", "")
	attempt.Scope, attempt.Credential, attempt.Polling = connections.CloneConnectorScope(record.Scope), record.Credential, true
	console.connections.PutAttempt(attempt.ID, attempt)
	attemptID := attempt.ID
	next := cloneState(console.state)
	next.Attempts[attemptID] = connectionAttemptRecord{
		AttemptID: attemptID, ClientID: scope.ClientID, PersonID: scope.PersonID, DeviceID: scope.DeviceID,
		ConnectorID: definition.ID, ConnectionID: connectionID, Credential: record.Credential,
		Incarnation: record.Incarnation, Epoch: record.Epoch,
		Scope: connections.CloneConnectorScope(record.Scope), CreatedAtUnixMs: attempt.CreatedAt.UnixMilli(), CleanupKind: "oauth_logout",
	}
	if err := console.save(next); err != nil {
		console.connections.DeleteAttempt(attemptID)
		console.mu.Unlock()
		outcome = operation.Reject(operation.Internal, "save_failed")
		return
	}
	console.state = next
	console.connections.Reserve(record.ConnectionID, record)
	console.mu.Unlock()
	ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	value, err := runtime.Action(ctx, "login")
	console.mu.Lock()
	console.connections.ReleaseReservation(record.ConnectionID)
	_, attemptStillOwned := console.state.Attempts[attemptID]
	if !attemptStillOwned {
		_ = console.retryPersonCleanupLocked(scope.PersonID)
	}
	console.mu.Unlock()
	if !attemptStillOwned {
		outcome = operation.Reject(operation.Conflict, "connection_changed")
		return
	}
	if err != nil {
		console.failClientOAuthAttempt(attemptID, runtime)
		outcome = operation.Reject(operation.Upstream, "connector_authorization_unavailable")
		return
	}
	status, authorizationURL, userCode, valid := connections.OauthActionStatus(value)
	if !valid || status != "pending" && status != "connected" {
		console.failClientOAuthAttempt(attemptID, runtime)
		outcome = operation.Reject(operation.Upstream, "invalid_connector_response")
		return
	}
	console.mu.Lock()
	attempt = console.connections.GetAttempt(attemptID)
	_, raced := console.connectionForPerson(definition.ID, scope.PersonID)
	foreign := console.connectionExistsForOtherPerson(definition.ID, scope.PersonID)
	if attempt == nil || raced || foreign {
		console.mu.Unlock()
		console.failClientOAuthAttempt(attemptID, runtime)
		outcome = operation.Reject(operation.Conflict, "connection_changed")
		return
	}
	attempt.Status, attempt.AuthorizationURL, attempt.UserCode, attempt.Polling = status, authorizationURL, userCode, false
	console.connections.PutAttempt(attempt.ID, attempt)
	console.mu.Unlock()
	if status == "connected" && !console.finishClientOAuthAttempt(attemptID) {
		console.failClientOAuthAttempt(attemptID, runtime)
		outcome = operation.Reject(operation.Internal, "credential_commit_failed")
		return
	}
	console.mu.Lock()
	currentAttempt := console.connections.GetAttempt(attemptID)
	if currentAttempt == nil {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "connection_changed")
		return
	}
	responseAttempt := *currentAttempt
	console.mu.Unlock()
	outcome = operation.Result{Category: operation.Created, Value: connections.ConnectorAttemptResponse(&responseAttempt)}
	return
}
func (console *Console) connectorAttempt(ctx context.Context, scope connections.Scope, definition connections.Definition, attemptID string) (outcome operation.Result) {
	unlockLifecycle := console.lockConnectorLifecycle(definition.ID)
	defer unlockLifecycle()
	if !console.requireCurrentClientScope(&outcome, scope) {
		return
	}
	console.mu.Lock()
	if !console.requireCurrentClientScopeLocked(&outcome, scope) {
		console.mu.Unlock()
		return
	}
	attempt := console.connections.GetAttempt(attemptID)
	exists := attempt != nil
	if !exists || attempt.ConnectorID != definition.ID || attempt.ClientID != scope.ClientID {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Missing, "attempt_not_found")
		return
	}
	copy := *attempt
	runtime := connections.ConnectorOAuthRuntime(nil)
	if connections.IsOAuthAuthKind(definition.AuthKind) && copy.Status == "pending" && !attempt.Polling {
		attempt.Polling = true
		console.connections.PutAttempt(attempt.ID, attempt)
		runtime = console.connectorOAuthRuntime(definition.ID)
		if durable, exists := console.state.Attempts[attemptID]; exists {
			console.connections.Reserve(durable.ConnectionID, connections.Record{
				ConnectionID: durable.ConnectionID, ConnectorID: durable.ConnectorID,
				PersonID: durable.PersonID, Credential: durable.Credential,
			})
		}
	}
	console.mu.Unlock()
	if runtime != nil {
		ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
		defer cancel()
		value, err := runtime.Action(ctx, "status")
		console.mu.Lock()
		console.connections.ReleaseReservation(copy.ConnectionID)
		_, attemptStillOwned := console.state.Attempts[attemptID]
		if !attemptStillOwned {
			_ = console.retryPersonCleanupLocked(scope.PersonID)
		}
		console.mu.Unlock()
		if !attemptStillOwned {
			outcome = operation.Reject(operation.Conflict, "connection_changed")
			return
		}
		if err != nil {
			copy.ErrorCode = "connector_authorization_unavailable"
		} else if status, authorizationURL, userCode, valid := connections.OauthActionStatus(value); valid {
			copy.Status, copy.AuthorizationURL, copy.UserCode, copy.ErrorCode = status, authorizationURL, userCode, ""
			if status == "disconnected" {
				copy.Status, copy.ErrorCode = "failed", "authorization_interrupted"
			}
		} else {
			copy.Status, copy.ErrorCode = "failed", "invalid_connector_response"
		}
		if copy.Status != "pending" {
			copy.AuthorizationURL, copy.UserCode = "", ""
		}
		if copy.Status == "connected" && !console.finishClientOAuthAttempt(attemptID) {
			copy.Status, copy.ErrorCode = "failed", "credential_commit_failed"
		}
		if copy.Status == "failed" {
			if console.cleanupClientOAuthAttempt(ctx, attemptID, runtime) != nil {
				copy.ErrorCode = "connection_cleanup_pending"
			}
		}
		console.mu.Lock()
		if current := console.connections.GetAttempt(attemptID); current != nil && current.ClientID == scope.ClientID {
			current.Status, current.AuthorizationURL, current.UserCode, current.ErrorCode = copy.Status, copy.AuthorizationURL, copy.UserCode, copy.ErrorCode
			console.connections.PutAttempt(current.ID, current)
			current.Polling = false
			console.connections.PutAttempt(current.ID, current)
			copy = *current
		}
		console.mu.Unlock()
	}
	outcome = operation.Result{Category: operation.Ready, Value: connections.ConnectorAttemptResponse(&copy)}
	return
}
func (console *Console) cancelConnectorAttempt(ctx context.Context, scope connections.Scope, definition connections.Definition, attemptID string) (outcome operation.Result) {
	unlockLifecycle := console.lockConnectorLifecycle(definition.ID)
	defer unlockLifecycle()
	if !console.requireCurrentClientScope(&outcome, scope) {
		return
	}
	console.mu.Lock()
	if !console.requireCurrentClientScopeLocked(&outcome, scope) {
		console.mu.Unlock()
		return
	}
	attempt := console.connections.GetAttempt(attemptID)
	exists := attempt != nil
	if !exists || attempt.ConnectorID != definition.ID || attempt.ClientID != scope.ClientID {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Missing, "attempt_not_found")
		return
	}
	if !connections.IsOAuthAuthKind(definition.AuthKind) {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "capability_not_supported")
		return
	}
	if attempt.Polling {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "connection_in_progress")
		return
	}
	if attempt.Status != "pending" && attempt.ErrorCode != "credential_cleanup_failed" {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "attempt_not_pending")
		return
	}
	runtime := console.connectorOAuthRuntime(definition.ID)
	copy := *attempt
	console.mu.Unlock()
	ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	if console.cleanupClientOAuthAttempt(ctx, attemptID, runtime) != nil {
		outcome = operation.Reject(operation.Internal, "connection_cleanup_pending")
		return
	}
	copy.Status, copy.AuthorizationURL = "cancelled", ""
	outcome = operation.Result{Category: operation.Ready, Value: connections.ConnectorAttemptResponse(&copy)}
	return
}
func (console *Console) updateConnectorScope(scope connections.Scope, definition connections.Definition, input connections.ScopeRequest) (outcome operation.Result) {
	unlockLifecycle := console.lockConnectorLifecycle(definition.ID)
	defer unlockLifecycle()
	if !console.requireCurrentClientScope(&outcome, scope) {
		return
	}
	if len(definition.ScopeFields) == 0 {
		outcome = operation.Reject(operation.Conflict, "capability_not_supported")
		return
	}
	if input.SchemaVersion != 1 {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	console.mu.Lock()
	defer console.mu.Unlock()
	if !console.requireCurrentClientScopeLocked(&outcome, scope) {
		return
	}
	record, exists := console.connectionForPerson(definition.ID, scope.PersonID)
	if !exists {
		outcome = operation.Reject(operation.Conflict, "connection_changed")
		return
	}
	if record.ConnectionID != input.ConnectionID || record.Revision != input.ConnectionRevision {
		outcome = operation.Reject(operation.Conflict, "connection_changed")
		return
	}
	selectedScope, err := connections.ValidatedConnectorScope(definition, input.Scope)
	if err != nil {
		outcome = operation.Reject(operation.Invalid, "invalid_scope")
		return
	}
	previous := cloneState(console.state)
	if reflect.DeepEqual(record.Scope, selectedScope) {
		outcome = operation.Result{Category: operation.Ready, Value: map[string]any{"schema_version": 1, "person_id": scope.PersonID, "device_id": scope.DeviceID, "connection_id": record.ConnectionID, "connection_revision": record.Revision, "connector_id": definition.ID, "scope": connections.CloneConnectorScope(selectedScope)}}
		return
	}
	record.Scope = selectedScope
	record.Revision++
	if record.Epoch == 0 || record.Epoch == ^uint64(0) {
		outcome = operation.Reject(operation.Conflict, "connection_epoch_invalid")
		return
	}
	record.Epoch++
	console.state.Connections[record.ConnectionID] = record
	if console.rebuildConnectorRuntimes() != nil || console.save(console.state) != nil {
		console.state = previous
		_ = console.rebuildConnectorRuntimes()
		outcome = operation.Reject(operation.Invalid, "invalid_scope")
		return
	}
	outcome = operation.Result{Category: operation.Ready, Value: map[string]any{"schema_version": 1, "person_id": scope.PersonID, "device_id": scope.DeviceID, "connection_id": record.ConnectionID, "connection_revision": record.Revision, "connector_id": definition.ID, "scope": connections.CloneConnectorScope(selectedScope)}}
	return
}
func (console *Console) disconnectConnector(ctx context.Context, scope connections.Scope, definition connections.Definition, input connections.DisconnectRequest) (outcome operation.Result) {
	unlockLifecycle := console.lockConnectorLifecycle(definition.ID)
	defer unlockLifecycle()
	if !console.requireCurrentClientScope(&outcome, scope) {
		return
	}
	if input.SchemaVersion != 1 {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	console.mu.Lock()
	if !console.requireCurrentClientScopeLocked(&outcome, scope) {
		console.mu.Unlock()
		return
	}
	record, exists := console.connectionForPerson(definition.ID, scope.PersonID)
	if !exists {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "connection_changed")
		return
	}
	if record.ConnectionID != input.ConnectionID || record.Revision != input.ConnectionRevision {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Conflict, "connection_changed")
		return
	}
	runtime := connections.ConnectorOAuthRuntime(nil)
	if connections.IsOAuthAuthKind(definition.AuthKind) {
		runtime = console.connectorOAuthRuntime(definition.ID)
	}
	credentialName := record.Credential
	next := cloneState(console.state)
	cleanup := next.Cleanups[record.PersonID]
	cleanup.PersonID = record.PersonID
	cleanup.Connections = append(cleanup.Connections, connectionCleanupStep{
		ConnectionID:    record.ConnectionID,
		ConnectorID:     record.ConnectorID,
		Credential:      record.Credential,
		RuntimeComplete: definition.AuthKind == "secret",
		VaultComplete:   record.Credential == "",
	})
	next.Cleanups[record.PersonID] = cleanup
	delete(next.Connections, record.ConnectionID)
	if err := console.save(next); err != nil {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Internal, "save_failed")
		return
	}
	console.state = next
	if err := console.rebuildConnectorRuntimes(); err != nil {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Internal, "invalid_connector_configuration")
		return
	}
	console.connections.Reserve(record.ConnectionID, record)
	for identifier, attempt := range console.connections.Attempts() {
		if attempt.ConnectionID == record.ConnectionID {
			console.connections.DeleteAttempt(identifier)
		}
	}
	console.mu.Unlock()
	runtimeComplete := runtime == nil
	if runtime != nil {
		ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
		_, err := runtime.Action(ctx, "logout")
		runtimeComplete = err == nil
		cancel()
	}
	if runtimeComplete && credentialName != "" {
		console.mu.Lock()
		_ = console.completeReservedCleanupLocked(record, true, false)
		console.mu.Unlock()
	}
	vaultComplete := credentialName == ""
	if runtimeComplete && !vaultComplete {
		vaultComplete = console.vault.Delete(credentialName) == nil
	}
	console.mu.Lock()
	console.connections.ReleaseReservation(record.ConnectionID)
	cleanupErr := console.completeReservedCleanupLocked(record, runtimeComplete, vaultComplete)
	console.mu.Unlock()
	if !runtimeComplete || !vaultComplete || cleanupErr != nil {
		outcome = operation.Reject(operation.Internal, "connection_cleanup_pending")
		return
	}
	outcome = operation.Result{Category: operation.Ready, Value: map[string]any{"schema_version": 1, "person_id": scope.PersonID, "device_id": scope.DeviceID, "connection_id": record.ConnectionID, "connector_id": definition.ID, "disconnected": true}}
	return
}
func (console *Console) requireCurrentClientScope(outcome *operation.Result, scope connections.Scope) bool {
	console.mu.Lock()
	defer console.mu.Unlock()
	return console.requireCurrentClientScopeLocked(outcome, scope)
}
func (console *Console) requireCurrentClientScopeLocked(outcome *operation.Result, scope connections.Scope) bool {
	client, exists := console.state.Clients[scope.ClientID]
	if !exists || client.PersonID != scope.PersonID || client.DeviceID != scope.DeviceID {
		*outcome = operation.Reject(operation.Unauthenticated, "unauthorized")
		return false
	}
	if _, cleanupPending := console.state.Cleanups[scope.PersonID]; cleanupPending {
		*outcome = operation.Reject(operation.Conflict, "person_cleanup_pending")
		return false
	}
	return true
}
