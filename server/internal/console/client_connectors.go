package console

import (
	"context"
	"errors"
	"net/http"
	"strings"
	"sync"
	"time"

	"floe/server/internal/credentials"
)

func invalidConnectorToken(token string) bool {
	return token != "" && len(token) < 8 || len(token) > 4096 || strings.ContainsAny(token, "\r\n\x00")
}

type connectorAttempt struct {
	ID               string
	ConnectorID      string
	ConnectionID     string
	PersonID         string
	DeviceID         string
	Status           string
	AuthorizationURL string
	ErrorCode        string
	CreatedAt        time.Time
	Scope            map[string]any
	Credential       string
	Polling          bool
}

type clientConnectorDefinition struct {
	ID              string
	Name            string
	AuthKind        string
	CredentialName  string
	OAuthCredential string
	RequiredScopes  []string
	ScopeFields     []string
	OAuthRuntime    func(*Console) ConnectorOAuthRuntime
	Available       func(*Console) bool
}

var clientConnectorDefinitions = []clientConnectorDefinition{
	{ID: "gmail", Name: "Gmail", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_GMAIL_OAUTH", RequiredScopes: []string{"https://www.googleapis.com/auth/gmail.readonly"}, ScopeFields: []string{}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.gmail }, Available: func(console *Console) bool { return console.gmail != nil }},
	{ID: "microsoft.mail", Name: "Microsoft Mail", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_MICROSOFT_MAIL_OAUTH", RequiredScopes: []string{"Mail.Read"}, ScopeFields: []string{}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.microsoftAuth }, Available: func(console *Console) bool { return console.microsoftAuth != nil }},
	{ID: "github.issues", Name: "GitHub Issues", AuthKind: "secret", CredentialName: githubTokenKey, RequiredScopes: []string{"github.issues.read"}, ScopeFields: []string{"owner", "repository"}, Available: func(*Console) bool { return true }},
	{ID: "slack.conversations", Name: "Slack", AuthKind: "secret", CredentialName: slackTokenKey, RequiredScopes: []string{"slack.selected_conversation.read"}, ScopeFields: []string{"channel", "thread"}, Available: func(*Console) bool { return true }},
	{ID: "google_drive.files", Name: "Google Drive", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_DRIVE_OAUTH", RequiredScopes: []string{"https://www.googleapis.com/auth/drive.readonly"}, ScopeFields: []string{"folder_id"}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.driveAuth }, Available: func(console *Console) bool { return console.driveAuth != nil }},
	{ID: "calendar.google", Name: "Google Calendar", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_GOOGLE_CALENDAR_OAUTH", RequiredScopes: []string{"https://www.googleapis.com/auth/calendar.readonly"}, ScopeFields: []string{"calendar_id"}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.calendarAuth }, Available: func(console *Console) bool { return console.calendarAuth != nil }},
	{ID: "calendar.microsoft", Name: "Microsoft Calendar", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_MICROSOFT_CALENDAR_OAUTH", RequiredScopes: []string{"Calendars.Read"}, ScopeFields: []string{"calendar_id"}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.microsoftCalendarAuth }, Available: func(console *Console) bool { return console.microsoftCalendarAuth != nil }},
	{ID: "microsoft.teams", Name: "Microsoft Teams", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_MICROSOFT_TEAMS_OAUTH", RequiredScopes: []string{"ChannelMessage.Read.All"}, ScopeFields: []string{"team_id", "channel_id"}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.microsoftTeamsAuth }, Available: func(console *Console) bool { return console.microsoftTeamsAuth != nil }},
	{ID: "home_assistant.states", Name: "Home Assistant", AuthKind: "secret", CredentialName: homeTokenKey, RequiredScopes: []string{"home.states.read"}, ScopeFields: []string{"base_url", "entities"}, Available: func(*Console) bool { return true }},
}

func clientConnectorDefinitionFor(identifier string) (clientConnectorDefinition, bool) {
	for _, definition := range clientConnectorDefinitions {
		if definition.ID == identifier {
			return definition, true
		}
	}
	return clientConnectorDefinition{}, false
}

func (console *Console) lockConnectorLifecycle(connectorID string) func() {
	console.connectorLifecycleMu.Lock()
	if console.connectorLifecycles == nil {
		console.connectorLifecycles = map[string]*sync.Mutex{}
	}
	lifecycle := console.connectorLifecycles[connectorID]
	if lifecycle == nil {
		lifecycle = &sync.Mutex{}
		console.connectorLifecycles[connectorID] = lifecycle
	}
	console.connectorLifecycleMu.Unlock()
	lifecycle.Lock()
	return lifecycle.Unlock
}

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

func connectorCapabilities(definition clientConnectorDefinition) map[string]any {
	return map[string]any{
		"connect":      true,
		"cancel":       definition.AuthKind == "oauth_pkce",
		"disconnect":   true,
		"scope_update": len(definition.ScopeFields) > 0,
	}
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
			if attempt.ConnectorID == definition.ID && attempt.PersonID == scope.PersonID && attempt.DeviceID == scope.DeviceID && (latest == nil || attempt.CreatedAt.After(latest.CreatedAt)) {
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
	if definition.AuthKind == "oauth_pkce" && input.Secret != "" || definition.AuthKind == "secret" && input.Secret == "" {
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
		if definition.AuthKind == "oauth_pkce" && existing.Credential != "" {
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
	for identifier, attempt := range console.connectorAttempts {
		if attempt.ConnectorID == definition.ID && attempt.PersonID == scope.PersonID {
			if attempt.Status == "pending" {
				if time.Since(attempt.CreatedAt) <= 10*time.Minute {
					console.mu.Unlock()
					failure(writer, http.StatusConflict, "connection_in_progress")
					return
				}
				if attempt.Credential != "" && console.vault.Delete(attempt.Credential) != nil {
					console.mu.Unlock()
					failure(writer, http.StatusServiceUnavailable, "credential_cleanup_failed")
					return
				}
			}
			if attempt.ErrorCode == "credential_cleanup_failed" && attempt.Credential != "" && console.vault.Delete(attempt.Credential) != nil {
				console.mu.Unlock()
				failure(writer, http.StatusServiceUnavailable, "credential_cleanup_failed")
				return
			}
			delete(console.connectorAttempts, identifier)
		}
	}
	connectionID, err := newConnectionID()
	if err != nil {
		console.mu.Unlock()
		failure(writer, http.StatusInternalServerError, "connection_identity_unavailable")
		return
	}
	record := connectionRecord{ConnectionID: connectionID, Revision: 1, ConnectorID: definition.ID, PersonID: scope.PersonID, Scope: selectedScope}
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
	console.mu.Unlock()
	ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
	defer cancel()
	value, err := runtime.Action(ctx, "login")
	if err != nil {
		console.failClientOAuthAttempt(attemptID, runtime, record.Credential, "connector_authorization_unavailable")
		failure(writer, http.StatusBadGateway, "connector_authorization_unavailable")
		return
	}
	status, authorizationURL, valid := oauthActionStatus(value)
	if !valid || status != "pending" && status != "connected" {
		console.failClientOAuthAttempt(attemptID, runtime, record.Credential, "invalid_connector_response")
		failure(writer, http.StatusBadGateway, "invalid_connector_response")
		return
	}
	console.mu.Lock()
	attempt = console.connectorAttempts[attemptID]
	_, raced := console.connectionForPerson(definition.ID, scope.PersonID)
	foreign := console.connectionExistsForOtherPerson(definition.ID, scope.PersonID)
	if attempt == nil || raced || foreign {
		console.mu.Unlock()
		console.failClientOAuthAttempt(attemptID, runtime, record.Credential, "connection_changed")
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	attempt.Status, attempt.AuthorizationURL, attempt.Polling = status, authorizationURL, false
	console.mu.Unlock()
	if status == "connected" && !console.finishClientOAuthAttempt(attemptID) {
		console.failClientOAuthAttempt(attemptID, runtime, record.Credential, "credential_commit_failed")
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
	if !exists || attempt.ConnectorID != definition.ID || attempt.PersonID != scope.PersonID || attempt.DeviceID != scope.DeviceID {
		console.mu.Unlock()
		failure(writer, http.StatusNotFound, "attempt_not_found")
		return
	}
	copy := *attempt
	runtime := ConnectorOAuthRuntime(nil)
	if definition.OAuthRuntime != nil && copy.Status == "pending" && !attempt.Polling {
		attempt.Polling = true
		runtime = definition.OAuthRuntime(console)
	}
	console.mu.Unlock()
	if runtime != nil {
		ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
		defer cancel()
		value, err := runtime.Action(ctx, "status")
		if err != nil {
			copy.ErrorCode = "connector_authorization_unavailable"
		} else if status, authorizationURL, valid := oauthActionStatus(value); valid {
			copy.Status, copy.AuthorizationURL, copy.ErrorCode = status, authorizationURL, ""
			if status == "disconnected" {
				copy.Status, copy.ErrorCode = "failed", "authorization_interrupted"
			}
		} else {
			copy.Status, copy.ErrorCode = "failed", "invalid_connector_response"
		}
		if copy.Status != "pending" {
			copy.AuthorizationURL = ""
		}
		if copy.Status == "connected" && !console.finishClientOAuthAttempt(attemptID) {
			copy.Status, copy.ErrorCode = "failed", "credential_commit_failed"
		}
		if copy.Status == "failed" {
			_, _ = runtime.Action(context.Background(), "cancel")
			if copy.Credential != "" && console.vault.Delete(copy.Credential) != nil {
				copy.ErrorCode = "credential_cleanup_failed"
			}
		}
		console.mu.Lock()
		if current := console.connectorAttempts[attemptID]; current != nil && current.PersonID == scope.PersonID {
			current.Status, current.AuthorizationURL, current.ErrorCode = copy.Status, copy.AuthorizationURL, copy.ErrorCode
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
	if !exists || attempt.ConnectorID != definition.ID || attempt.PersonID != scope.PersonID || attempt.DeviceID != scope.DeviceID {
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
	credentialName := attempt.Credential
	console.mu.Unlock()
	ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
	defer cancel()
	if _, err := runtime.Action(ctx, "cancel"); err != nil {
		failure(writer, http.StatusBadGateway, "connector_authorization_unavailable")
		return
	}
	if credentialName != "" && console.vault.Delete(credentialName) != nil {
		console.mu.Lock()
		if current := console.connectorAttempts[attemptID]; current != nil {
			current.Status, current.ErrorCode, current.AuthorizationURL = "failed", "credential_cleanup_failed", ""
		}
		console.mu.Unlock()
		failure(writer, http.StatusInternalServerError, "credential_cleanup_failed")
		return
	}
	console.mu.Lock()
	attempt = console.connectorAttempts[attemptID]
	if attempt == nil || attempt.PersonID != scope.PersonID || attempt.DeviceID != scope.DeviceID {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	attempt.Status, attempt.AuthorizationURL = "cancelled", ""
	console.mu.Unlock()
	copy := *attempt
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
	record.Scope = selectedScope
	record.Revision++
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
	previous := cloneState(console.state)
	delete(console.state.Connections, record.ConnectionID)
	if err := console.rebuildConnectorRuntimes(); err != nil || console.save(console.state) != nil {
		console.state = previous
		_ = console.rebuildConnectorRuntimes()
		console.mu.Unlock()
		failure(writer, http.StatusInternalServerError, "save_failed")
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
	vaultComplete := credentialName == ""
	if !vaultComplete {
		vaultComplete = console.vault.Delete(credentialName) == nil
	}
	console.mu.Lock()
	delete(console.connectorReservations, record.ConnectionID)
	cleanupErr := console.completeReservedCleanupLocked(record, runtimeComplete, vaultComplete)
	if !vaultComplete && cleanupErr == nil {
		if _, cleanupPending := console.state.Cleanups[record.PersonID]; !cleanupPending && console.personHasClientLocked(record.PersonID) && console.connectionCredentialReady(record) {
			if _, exists := console.state.Connections[record.ConnectionID]; !exists {
				next := cloneState(console.state)
				next.Connections[record.ConnectionID] = record
				if console.save(next) == nil {
					console.state = next
					_ = console.rebuildConnectorRuntimes()
				}
			}
		}
	}
	console.mu.Unlock()
	if !vaultComplete || cleanupErr != nil {
		failure(writer, http.StatusInternalServerError, "credential_cleanup_failed")
		return
	}
	reply(writer, http.StatusOK, map[string]any{"schema_version": 1, "person_id": scope.PersonID, "device_id": scope.DeviceID, "connection_id": record.ConnectionID, "connector_id": definition.ID, "disconnected": true})
}

func (console *Console) personHasClientLocked(personID string) bool {
	for _, client := range console.state.Clients {
		if client.PersonID == personID {
			return true
		}
	}
	return false
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

func oauthActionStatus(value any) (string, string, bool) {
	statusValue, ok := value.(map[string]any)
	if !ok {
		return "", "", false
	}
	status, ok := statusValue["status"].(string)
	if !ok || status != "pending" && status != "connected" && status != "disconnected" {
		return "", "", false
	}
	authorizationURL, _ := statusValue["auth_url"].(string)
	if status == "pending" && authorizationURL == "" {
		return "", "", false
	}
	return status, authorizationURL, true
}

func bindClientOAuthCredential(runtime ConnectorOAuthRuntime, record connectionRecord) error {
	return runtime.BindCredential(record.Credential)
}

func connectorAttemptResponse(attempt *connectorAttempt) map[string]any {
	value := map[string]any{
		"schema_version": 1, "attempt_id": attempt.ID, "connector_id": attempt.ConnectorID,
		"connection_id": attempt.ConnectionID, "person_id": attempt.PersonID,
		"device_id": attempt.DeviceID, "status": attempt.Status,
		"created_at": attempt.CreatedAt.UTC().Format(time.RFC3339),
	}
	if attempt.AuthorizationURL != "" {
		value["authorization_url"] = attempt.AuthorizationURL
	}
	if attempt.ErrorCode != "" {
		value["error"] = map[string]string{"code": attempt.ErrorCode}
	}
	return value
}

func (console *Console) newConnectorAttempt(connectorID, connectionID string, scope clientScope, status, authorizationURL string) *connectorAttempt {
	now := time.Now()
	for identifier, attempt := range console.connectorAttempts {
		if attempt.Status != "pending" && now.Sub(attempt.CreatedAt) > 10*time.Minute {
			delete(console.connectorAttempts, identifier)
		}
	}
	if len(console.connectorAttempts) >= 64 {
		var oldestID string
		var oldest time.Time
		for identifier, attempt := range console.connectorAttempts {
			if attempt.Status != "pending" && (oldestID == "" || attempt.CreatedAt.Before(oldest)) {
				oldestID, oldest = identifier, attempt.CreatedAt
			}
		}
		if oldestID != "" {
			delete(console.connectorAttempts, oldestID)
		}
	}
	attempt := &connectorAttempt{ID: randomToken(), ConnectorID: connectorID, ConnectionID: connectionID, PersonID: scope.PersonID, DeviceID: scope.DeviceID, Status: status, AuthorizationURL: authorizationURL, CreatedAt: now}
	console.connectorAttempts[attempt.ID] = attempt
	return attempt
}

func (console *Console) connectionCredentialReady(record connectionRecord) bool {
	if record.Credential == "" {
		return true
	}
	value, err := console.vault.Get(record.Credential)
	if err != nil || value == "" {
		return false
	}
	definition, exists := clientConnectorDefinitionFor(record.ConnectorID)
	if !exists || definition.AuthKind != "oauth_pkce" {
		return true
	}
	runtime := definition.OAuthRuntime(console)
	return runtime != nil && runtime.Ready()
}

func (console *Console) finishClientOAuthAttempt(attemptID string) bool {
	console.mu.Lock()
	defer console.mu.Unlock()
	attempt := console.connectorAttempts[attemptID]
	if attempt == nil || attempt.Status != "pending" && attempt.Status != "connected" {
		return false
	}
	record := connectionRecord{ConnectionID: attempt.ConnectionID, Revision: 1, ConnectorID: attempt.ConnectorID, PersonID: attempt.PersonID, Scope: cloneConnectorScope(attempt.Scope), Credential: attempt.Credential}
	if !console.connectionCredentialReady(record) {
		return false
	}
	if existing, exists := console.connectionForPerson(record.ConnectorID, record.PersonID); exists && existing.ConnectionID != record.ConnectionID {
		return false
	}
	previous := cloneState(console.state)
	console.state.Connections[record.ConnectionID] = record
	if console.rebuildConnectorRuntimes() != nil || console.save(console.state) != nil {
		console.state = previous
		_ = console.rebuildConnectorRuntimes()
		return false
	}
	attempt.Status, attempt.AuthorizationURL, attempt.ErrorCode = "connected", "", ""
	return true
}

func (console *Console) failClientOAuthAttempt(attemptID string, runtime ConnectorOAuthRuntime, credentialName, errorCode string) {
	_, _ = runtime.Action(context.Background(), "cancel")
	if credentialName != "" && console.vault.Delete(credentialName) != nil {
		errorCode = "credential_cleanup_failed"
	}
	console.mu.Lock()
	defer console.mu.Unlock()
	if attempt := console.connectorAttempts[attemptID]; attempt != nil {
		attempt.Status, attempt.AuthorizationURL, attempt.ErrorCode, attempt.Polling = "failed", "", errorCode, false
	}
}

func (console *Console) connectionForPerson(connectorID, personID string) (connectionRecord, bool) {
	for _, record := range console.state.Connections {
		if record.ConnectorID == connectorID && record.PersonID == personID {
			return record, true
		}
	}
	return connectionRecord{}, false
}

func (console *Console) connectionForConnector(connectorID string) (connectionRecord, bool) {
	for _, record := range console.state.Connections {
		if record.ConnectorID == connectorID {
			return record, true
		}
	}
	return connectionRecord{}, false
}

func (console *Console) connectionExistsForOtherPerson(connectorID, personID string) bool {
	for _, record := range console.state.Connections {
		if record.ConnectorID == connectorID && record.PersonID != personID {
			return true
		}
	}
	return false
}

func isCalendarConnector(connectorID string) bool {
	return connectorID == "calendar.google" || connectorID == "calendar.microsoft"
}

func (console *Console) calendarConnectionExistsForPerson(personID, exceptConnectorID string) bool {
	for _, record := range console.state.Connections {
		if record.PersonID == personID && record.ConnectorID != exceptConnectorID && isCalendarConnector(record.ConnectorID) {
			return true
		}
	}
	return false
}

func onlyScopeFields(scope map[string]any, allowed ...string) bool {
	for key := range scope {
		found := false
		for _, candidate := range allowed {
			found = found || key == candidate
		}
		if !found {
			return false
		}
	}
	return true
}

func scopeString(scope map[string]any, key string) (string, bool) {
	value, ok := scope[key].(string)
	return strings.TrimSpace(value), ok && value == strings.TrimSpace(value)
}

func validatedConnectorScope(definition clientConnectorDefinition, scope map[string]any) (map[string]any, error) {
	if scope == nil {
		scope = map[string]any{}
	}
	if !onlyScopeFields(scope, definition.ScopeFields...) {
		return nil, errors.New("unknown scope field")
	}
	switch definition.ID {
	case "gmail", "microsoft.mail":
		if len(scope) != 0 {
			return nil, errors.New("scope unsupported")
		}
		return map[string]any{}, nil
	case "github.issues":
		owner, ownerOK := scopeString(scope, "owner")
		repository, repositoryOK := scopeString(scope, "repository")
		if !ownerOK || !repositoryOK || owner == "" || repository == "" || len(owner) > 128 || len(repository) > 128 {
			return nil, errors.New("invalid github scope")
		}
		return map[string]any{"owner": owner, "repository": repository}, nil
	case "slack.conversations":
		channel, channelOK := scopeString(scope, "channel")
		thread := ""
		if value, exists := scope["thread"]; exists {
			var ok bool
			thread, ok = value.(string)
			if !ok || thread != strings.TrimSpace(thread) {
				return nil, errors.New("invalid slack thread")
			}
		}
		if !channelOK || channel == "" || len(channel) > 128 || len(thread) > 128 {
			return nil, errors.New("invalid slack scope")
		}
		return map[string]any{"channel": channel, "thread": thread}, nil
	case "home_assistant.states":
		baseURL, baseURLOK := scopeString(scope, "base_url")
		items, itemsOK := scope["entities"].([]any)
		entities := make([]string, 0, len(items))
		for _, item := range items {
			value, ok := item.(string)
			if !ok || value == "" || value != strings.TrimSpace(value) {
				return nil, errors.New("invalid entity")
			}
			entities = append(entities, value)
		}
		if !baseURLOK || !itemsOK || baseURL == "" || len(entities) == 0 || len(entities) > 128 {
			return nil, errors.New("invalid home assistant scope")
		}
		return map[string]any{"base_url": baseURL, "entities": entities}, nil
	case "google_drive.files":
		folderID, ok := scopeString(scope, "folder_id")
		if !ok || folderID == "" || len(folderID) > 256 {
			return nil, errors.New("invalid drive scope")
		}
		return map[string]any{"folder_id": folderID}, nil
	case "calendar.google":
		calendarID, ok := scopeString(scope, "calendar_id")
		if !ok || calendarID == "" || len(calendarID) > 256 {
			return nil, errors.New("invalid calendar scope")
		}
		return map[string]any{"calendar_id": calendarID}, nil
	case "calendar.microsoft":
		calendarID, ok := scopeString(scope, "calendar_id")
		if !ok || calendarID == "" || len(calendarID) > 256 {
			return nil, errors.New("invalid calendar scope")
		}
		return map[string]any{"calendar_id": calendarID}, nil
	case "microsoft.teams":
		teamID, teamOK := scopeString(scope, "team_id")
		channelID, channelOK := scopeString(scope, "channel_id")
		if !teamOK || !channelOK || teamID == "" || channelID == "" || len(teamID) > 256 || len(channelID) > 256 {
			return nil, errors.New("invalid teams scope")
		}
		return map[string]any{"team_id": teamID, "channel_id": channelID}, nil
	default:
		return nil, errors.New("unknown connector")
	}
}

func cloneConnectorScope(scope map[string]any) map[string]any {
	copy := make(map[string]any, len(scope))
	for key, value := range scope {
		if values, ok := connectorScopeStrings(value); ok {
			copy[key] = values
		} else {
			copy[key] = value
		}
	}
	return copy
}
