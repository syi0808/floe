// HTTP and admin-UI transport for the local server.

package httptransport

import (
	"context"
	"errors"
	"net/http"
	"reflect"
	"strings"
	"sync"
	"time"

	"floe/server/internal/credentials"
)

func (console *Console) serveClientConnectors(writer http.ResponseWriter, request *http.Request, scope clientScope) {
	if request.URL.Path == "/v1/connectors" || request.URL.Path == "/v1/connectors/" {
		if request.Method != http.MethodGet {
			failure(writer, http.StatusNotFound, "not_found")
			return
		}
		console.writeClientConnectorCatalog(writer, scope)
		return
	}
	parts := strings.Split(strings.TrimPrefix(request.URL.Path, "/v1/connectors/"), "/")
	definition, exists := clientConnectorDefinitionFor(parts[0])
	if !exists {
		failure(writer, http.StatusNotFound, "connector_not_found")
		return
	}
	if len(parts) == 2 && parts[1] == "connect" && request.Method == http.MethodPost {
		console.startClientConnector(writer, request, scope, definition)
		return
	}
	if len(parts) == 2 && parts[1] == "scope" && request.Method == http.MethodPatch {
		console.updateClientConnectorScope(writer, request, scope, definition)
		return
	}
	if len(parts) == 1 && request.Method == http.MethodDelete {
		console.disconnectClientConnector(writer, request, scope, definition)
		return
	}
	if len(parts) == 3 && parts[1] == "connection-attempts" && request.Method == http.MethodGet {
		console.writeClientConnectorAttempt(writer, request, scope, definition, parts[2])
		return
	}
	if len(parts) == 4 && parts[1] == "connection-attempts" && parts[3] == "cancel" && request.Method == http.MethodPost {
		console.cancelClientConnectorAttempt(writer, request, scope, definition, parts[2])
		return
	}
	failure(writer, http.StatusNotFound, "not_found")
}

func (console *Console) writeClientConnectorCatalog(writer http.ResponseWriter, scope clientScope) {
	console.mu.Lock()
	defer console.mu.Unlock()
	items := make([]any, 0, len(clientConnectorDefinitions))
	for _, definition := range clientConnectorDefinitions {
		connection, connected := console.connectionForPerson(definition.ID, scope.PersonID)
		status := "disconnected"
		if connected && console.connectionCredentialReady(connection) {
			status = "connected"
		} else if connected {
			status = "error"
		}
		var latest *connectorAttempt
		for _, attempt := range console.connectorAttempts {
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
		available := definition.Available(console)
		if !available {
			status = "unavailable"
		}
		item := map[string]any{
			"id": definition.ID, "name": definition.Name, "auth_kind": definition.AuthKind,
			"available": available, "status": status, "required_scopes": definition.RequiredScopes,
			"scope_fields": definition.ScopeFields, "capabilities": connectorCapabilities(definition),
		}
		if connected {
			item["connection_id"] = connection.ConnectionID
			item["connection_revision"] = connection.Revision
			item["incarnation"] = connection.Incarnation
			item["epoch"] = connection.Epoch
			item["execution_owner"] = console.state.ExecutionOwnerID
			item["identity_unverified"] = connection.IdentityUnverified
			item["scope"] = cloneConnectorScope(connection.Scope)
		}
		items = append(items, item)
	}
	reply(writer, http.StatusOK, map[string]any{"schema_version": 1, "person_id": scope.PersonID, "device_id": scope.DeviceID, "connectors": items})
}

func (console *Console) startClientConnector(writer http.ResponseWriter, request *http.Request, scope clientScope, definition clientConnectorDefinition) {
	unlockLifecycle := console.lockConnectorLifecycle(definition.ID)
	defer unlockLifecycle()
	if !console.requireCurrentClientScope(writer, scope) {
		return
	}
	var input struct {
		SchemaVersion int            `json:"schema_version"`
		Secret        string         `json:"secret"`
		Scope         map[string]any `json:"scope"`
	}
	if !decode(writer, request, &input) || input.SchemaVersion != 1 || invalidConnectorToken(input.Secret) {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	if isOAuthAuthKind(definition.AuthKind) && input.Secret != "" || definition.AuthKind == "secret" && input.Secret == "" {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	selectedScope, err := validatedConnectorScope(definition, input.Scope)
	if err != nil {
		failure(writer, http.StatusBadRequest, "invalid_scope")
		return
	}
	console.mu.Lock()
	if !console.requireCurrentClientScopeLocked(writer, scope) {
		console.mu.Unlock()
		return
	}
	if existing, exists := console.connectionForPerson(definition.ID, scope.PersonID); exists {
		if console.connectionCredentialReady(existing) {
			console.mu.Unlock()
			failure(writer, http.StatusConflict, "already_connected")
			return
		}
		if isOAuthAuthKind(definition.AuthKind) && existing.Credential != "" {
			if console.vault.Delete(existing.Credential) != nil {
				console.mu.Unlock()
				failure(writer, http.StatusServiceUnavailable, "credential_cleanup_failed")
				return
			}
		}
		previous := cloneState(console.state)
		delete(console.state.Connections, existing.ConnectionID)
		if console.rebuildConnectorRuntimes() != nil || console.save(console.state) != nil {
			console.state = previous
			_ = console.rebuildConnectorRuntimes()
			console.mu.Unlock()
			failure(writer, http.StatusInternalServerError, "save_failed")
			return
		}
	}
	if console.connectionExistsForOtherPerson(definition.ID, scope.PersonID) {
		console.mu.Unlock()
		failure(writer, http.StatusForbidden, "connection_owned_by_another_person")
		return
	}
	if isCalendarConnector(definition.ID) && console.calendarConnectionExistsForPerson(scope.PersonID, definition.ID) {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "calendar_connection_exists")
		return
	}
	if !definition.Available(console) {
		console.mu.Unlock()
		failure(writer, http.StatusServiceUnavailable, "connector_unavailable")
		return
	}
	for _, attempt := range console.connectorAttempts {
		if attempt.ConnectorID == definition.ID && attempt.PersonID == scope.PersonID {
			console.mu.Unlock()
			failure(writer, http.StatusConflict, "connection_in_progress")
			return
		}
	}
	connectionID, err := newConnectionID()
	if err != nil {
		console.mu.Unlock()
		failure(writer, http.StatusInternalServerError, "connection_identity_unavailable")
		return
	}
	incarnation, err := newConnectionID()
	if err != nil {
		console.mu.Unlock()
		failure(writer, http.StatusInternalServerError, "connection_identity_unavailable")
		return
	}
	record := connectionRecord{ConnectionID: connectionID, Revision: 1, ConnectorID: definition.ID, PersonID: scope.PersonID, Scope: selectedScope, Incarnation: incarnation, Epoch: 1, IdentityUnverified: true}
	credentialNamespace := definition.CredentialName
	if credentialNamespace == "" {
		credentialNamespace = definition.OAuthCredential
	}
	if credentialNamespace != "" {
		credentialName, err := credentials.ConnectionName(credentialNamespace, connectionID, scope.PersonID)
		if err != nil {
			console.mu.Unlock()
			failure(writer, http.StatusServiceUnavailable, "credential_scope_unavailable")
			return
		}
		record.Credential = credentialName
	}
	previous := cloneState(console.state)
	if definition.AuthKind == "secret" {
		if console.vault.Put(record.Credential, input.Secret) != nil {
			console.state = previous
			console.mu.Unlock()
			failure(writer, http.StatusServiceUnavailable, "credential_store_unavailable")
			return
		}
		console.state.Connections[connectionID] = record
		if err := console.rebuildConnectorRuntimes(); err != nil || console.save(console.state) != nil {
			_ = console.vault.Delete(record.Credential)
			console.state = previous
			_ = console.rebuildConnectorRuntimes()
			console.mu.Unlock()
			failure(writer, http.StatusInternalServerError, "save_failed")
			return
		}
		attempt := console.newConnectorAttempt(definition.ID, connectionID, scope, "connected", "")
		responseAttempt := *attempt
		console.mu.Unlock()
		reply(writer, http.StatusCreated, connectorAttemptResponse(&responseAttempt))
		return
	}
	runtime := definition.OAuthRuntime(console)
	if err := bindClientOAuthCredential(runtime, record); err != nil {
		console.mu.Unlock()
		failure(writer, http.StatusServiceUnavailable, "credential_scope_unavailable")
		return
	}
	attempt := console.newConnectorAttempt(definition.ID, connectionID, scope, "pending", "")
	attempt.Scope, attempt.Credential, attempt.Polling = cloneConnectorScope(record.Scope), record.Credential, true
	attemptID := attempt.ID
	next := cloneState(console.state)
	next.Attempts[attemptID] = connectionAttemptRecord{
		AttemptID: attemptID, ClientID: scope.ClientID, PersonID: scope.PersonID, DeviceID: scope.DeviceID,
		ConnectorID: definition.ID, ConnectionID: connectionID, Credential: record.Credential,
		Incarnation: record.Incarnation, Epoch: record.Epoch,
		Scope: cloneConnectorScope(record.Scope), CreatedAtUnixMs: attempt.CreatedAt.UnixMilli(), CleanupKind: "oauth_logout",
	}
	if err := console.save(next); err != nil {
		delete(console.connectorAttempts, attemptID)
		console.mu.Unlock()
		failure(writer, http.StatusInternalServerError, "save_failed")
		return
	}
	console.state = next
	console.connectorReservations[record.ConnectionID] = record
	console.mu.Unlock()
	ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
	defer cancel()
	value, err := runtime.Action(ctx, "login")
	console.mu.Lock()
	delete(console.connectorReservations, record.ConnectionID)
	_, attemptStillOwned := console.state.Attempts[attemptID]
	if !attemptStillOwned {
		_ = console.retryPersonCleanupLocked(scope.PersonID)
	}
	console.mu.Unlock()
	if !attemptStillOwned {
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	if err != nil {
		console.failClientOAuthAttempt(attemptID, runtime)
		failure(writer, http.StatusBadGateway, "connector_authorization_unavailable")
		return
	}
	status, authorizationURL, userCode, valid := oauthActionStatus(value)
	if !valid || status != "pending" && status != "connected" {
		console.failClientOAuthAttempt(attemptID, runtime)
		failure(writer, http.StatusBadGateway, "invalid_connector_response")
		return
	}
	console.mu.Lock()
	attempt = console.connectorAttempts[attemptID]
	_, raced := console.connectionForPerson(definition.ID, scope.PersonID)
	foreign := console.connectionExistsForOtherPerson(definition.ID, scope.PersonID)
	if attempt == nil || raced || foreign {
		console.mu.Unlock()
		console.failClientOAuthAttempt(attemptID, runtime)
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	attempt.Status, attempt.AuthorizationURL, attempt.UserCode, attempt.Polling = status, authorizationURL, userCode, false
	console.mu.Unlock()
	if status == "connected" && !console.finishClientOAuthAttempt(attemptID) {
		console.failClientOAuthAttempt(attemptID, runtime)
		failure(writer, http.StatusInternalServerError, "credential_commit_failed")
		return
	}
	console.mu.Lock()
	currentAttempt := console.connectorAttempts[attemptID]
	if currentAttempt == nil {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	responseAttempt := *currentAttempt
	console.mu.Unlock()
	reply(writer, http.StatusCreated, connectorAttemptResponse(&responseAttempt))
}

func (console *Console) writeClientConnectorAttempt(writer http.ResponseWriter, request *http.Request, scope clientScope, definition clientConnectorDefinition, attemptID string) {
	unlockLifecycle := console.lockConnectorLifecycle(definition.ID)
	defer unlockLifecycle()
	if !console.requireCurrentClientScope(writer, scope) {
		return
	}
	console.mu.Lock()
	if !console.requireCurrentClientScopeLocked(writer, scope) {
		console.mu.Unlock()
		return
	}
	attempt, exists := console.connectorAttempts[attemptID]
	if !exists || attempt.ConnectorID != definition.ID || attempt.ClientID != scope.ClientID {
		console.mu.Unlock()
		failure(writer, http.StatusNotFound, "attempt_not_found")
		return
	}
	copy := *attempt
	runtime := ConnectorOAuthRuntime(nil)
	if definition.OAuthRuntime != nil && copy.Status == "pending" && !attempt.Polling {
		attempt.Polling = true
		runtime = definition.OAuthRuntime(console)
		if durable, exists := console.state.Attempts[attemptID]; exists {
			console.connectorReservations[durable.ConnectionID] = connectionRecord{
				ConnectionID: durable.ConnectionID, ConnectorID: durable.ConnectorID,
				PersonID: durable.PersonID, Credential: durable.Credential,
			}
		}
	}
	console.mu.Unlock()
	if runtime != nil {
		ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
		defer cancel()
		value, err := runtime.Action(ctx, "status")
		console.mu.Lock()
		delete(console.connectorReservations, copy.ConnectionID)
		_, attemptStillOwned := console.state.Attempts[attemptID]
		if !attemptStillOwned {
			_ = console.retryPersonCleanupLocked(scope.PersonID)
		}
		console.mu.Unlock()
		if !attemptStillOwned {
			failure(writer, http.StatusConflict, "connection_changed")
			return
		}
		if err != nil {
			copy.ErrorCode = "connector_authorization_unavailable"
		} else if status, authorizationURL, userCode, valid := oauthActionStatus(value); valid {
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
			if console.cleanupClientOAuthAttempt(request.Context(), attemptID, runtime) != nil {
				copy.ErrorCode = "connection_cleanup_pending"
			}
		}
		console.mu.Lock()
		if current := console.connectorAttempts[attemptID]; current != nil && current.ClientID == scope.ClientID {
			current.Status, current.AuthorizationURL, current.UserCode, current.ErrorCode = copy.Status, copy.AuthorizationURL, copy.UserCode, copy.ErrorCode
			current.Polling = false
			copy = *current
		}
		console.mu.Unlock()
	}
	reply(writer, http.StatusOK, connectorAttemptResponse(&copy))
}

func (console *Console) cancelClientConnectorAttempt(writer http.ResponseWriter, request *http.Request, scope clientScope, definition clientConnectorDefinition, attemptID string) {
	unlockLifecycle := console.lockConnectorLifecycle(definition.ID)
	defer unlockLifecycle()
	if !console.requireCurrentClientScope(writer, scope) {
		return
	}
	console.mu.Lock()
	if !console.requireCurrentClientScopeLocked(writer, scope) {
		console.mu.Unlock()
		return
	}
	attempt, exists := console.connectorAttempts[attemptID]
	if !exists || attempt.ConnectorID != definition.ID || attempt.ClientID != scope.ClientID {
		console.mu.Unlock()
		failure(writer, http.StatusNotFound, "attempt_not_found")
		return
	}
	if definition.OAuthRuntime == nil {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "capability_not_supported")
		return
	}
	if attempt.Polling {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "connection_in_progress")
		return
	}
	if attempt.Status != "pending" && attempt.ErrorCode != "credential_cleanup_failed" {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "attempt_not_pending")
		return
	}
	runtime := definition.OAuthRuntime(console)
	copy := *attempt
	console.mu.Unlock()
	ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
	defer cancel()
	if console.cleanupClientOAuthAttempt(ctx, attemptID, runtime) != nil {
		failure(writer, http.StatusInternalServerError, "connection_cleanup_pending")
		return
	}
	copy.Status, copy.AuthorizationURL = "cancelled", ""
	reply(writer, http.StatusOK, connectorAttemptResponse(&copy))
}

func (console *Console) updateClientConnectorScope(writer http.ResponseWriter, request *http.Request, scope clientScope, definition clientConnectorDefinition) {
	unlockLifecycle := console.lockConnectorLifecycle(definition.ID)
	defer unlockLifecycle()
	if !console.requireCurrentClientScope(writer, scope) {
		return
	}
	if len(definition.ScopeFields) == 0 {
		failure(writer, http.StatusConflict, "capability_not_supported")
		return
	}
	var input struct {
		SchemaVersion      int            `json:"schema_version"`
		ConnectionID       string         `json:"connection_id"`
		ConnectionRevision uint64         `json:"connection_revision"`
		Scope              map[string]any `json:"scope"`
	}
	if !decode(writer, request, &input) || input.SchemaVersion != 1 {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	console.mu.Lock()
	defer console.mu.Unlock()
	if !console.requireCurrentClientScopeLocked(writer, scope) {
		return
	}
	record, exists := console.connectionForPerson(definition.ID, scope.PersonID)
	if !exists {
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	if record.ConnectionID != input.ConnectionID || record.Revision != input.ConnectionRevision {
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	selectedScope, err := validatedConnectorScope(definition, input.Scope)
	if err != nil {
		failure(writer, http.StatusBadRequest, "invalid_scope")
		return
	}
	previous := cloneState(console.state)
	if reflect.DeepEqual(record.Scope, selectedScope) {
		reply(writer, http.StatusOK, map[string]any{"schema_version": 1, "person_id": scope.PersonID, "device_id": scope.DeviceID, "connection_id": record.ConnectionID, "connection_revision": record.Revision, "connector_id": definition.ID, "scope": cloneConnectorScope(selectedScope)})
		return
	}
	record.Scope = selectedScope
	record.Revision++
	if record.Epoch == 0 || record.Epoch == ^uint64(0) {
		failure(writer, http.StatusConflict, "connection_epoch_invalid")
		return
	}
	record.Epoch++
	console.state.Connections[record.ConnectionID] = record
	if console.rebuildConnectorRuntimes() != nil || console.save(console.state) != nil {
		console.state = previous
		_ = console.rebuildConnectorRuntimes()
		failure(writer, http.StatusBadRequest, "invalid_scope")
		return
	}
	reply(writer, http.StatusOK, map[string]any{"schema_version": 1, "person_id": scope.PersonID, "device_id": scope.DeviceID, "connection_id": record.ConnectionID, "connection_revision": record.Revision, "connector_id": definition.ID, "scope": cloneConnectorScope(selectedScope)})
}

func (console *Console) disconnectClientConnector(writer http.ResponseWriter, request *http.Request, scope clientScope, definition clientConnectorDefinition) {
	unlockLifecycle := console.lockConnectorLifecycle(definition.ID)
	defer unlockLifecycle()
	if !console.requireCurrentClientScope(writer, scope) {
		return
	}
	var input struct {
		SchemaVersion      int    `json:"schema_version"`
		ConnectionID       string `json:"connection_id"`
		ConnectionRevision uint64 `json:"connection_revision"`
	}
	if !decode(writer, request, &input) || input.SchemaVersion != 1 {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	console.mu.Lock()
	if !console.requireCurrentClientScopeLocked(writer, scope) {
		console.mu.Unlock()
		return
	}
	record, exists := console.connectionForPerson(definition.ID, scope.PersonID)
	if !exists {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	if record.ConnectionID != input.ConnectionID || record.Revision != input.ConnectionRevision {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	runtime := ConnectorOAuthRuntime(nil)
	if definition.OAuthRuntime != nil {
		runtime = definition.OAuthRuntime(console)
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
		failure(writer, http.StatusInternalServerError, "save_failed")
		return
	}
	console.state = next
	if err := console.rebuildConnectorRuntimes(); err != nil {
		console.mu.Unlock()
		failure(writer, http.StatusInternalServerError, "invalid_connector_configuration")
		return
	}
	console.connectorReservations[record.ConnectionID] = record
	for identifier, attempt := range console.connectorAttempts {
		if attempt.ConnectionID == record.ConnectionID {
			delete(console.connectorAttempts, identifier)
		}
	}
	console.mu.Unlock()
	runtimeComplete := runtime == nil
	if runtime != nil {
		ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
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
	delete(console.connectorReservations, record.ConnectionID)
	cleanupErr := console.completeReservedCleanupLocked(record, runtimeComplete, vaultComplete)
	console.mu.Unlock()
	if !runtimeComplete || !vaultComplete || cleanupErr != nil {
		failure(writer, http.StatusInternalServerError, "connection_cleanup_pending")
		return
	}
	reply(writer, http.StatusOK, map[string]any{"schema_version": 1, "person_id": scope.PersonID, "device_id": scope.DeviceID, "connection_id": record.ConnectionID, "connector_id": definition.ID, "disconnected": true})
}

func (console *Console) requireCurrentClientScopeLocked(writer http.ResponseWriter, scope clientScope) bool {
	client, exists := console.state.Clients[scope.ClientID]
	if !exists || client.PersonID != scope.PersonID || client.DeviceID != scope.DeviceID {
		failure(writer, http.StatusUnauthorized, "unauthorized")
		return false
	}
	if _, cleanupPending := console.state.Cleanups[scope.PersonID]; cleanupPending {
		failure(writer, http.StatusConflict, "person_cleanup_pending")
		return false
	}
	return true
}

func (console *Console) requireCurrentClientScope(writer http.ResponseWriter, scope clientScope) bool {
	console.mu.Lock()
	defer console.mu.Unlock()
	return console.requireCurrentClientScopeLocked(writer, scope)
}
