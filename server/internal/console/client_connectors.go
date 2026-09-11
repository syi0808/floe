package console

import (
	"context"
	"errors"
	"net/http"
	"strings"
	"time"

	"floe/server/internal/credentials"
)

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
}

type clientConnectorDefinition struct {
	ID             string
	Name           string
	AuthKind       string
	CredentialName string
	RequiredScopes []string
	ScopeFields    []string
	OAuthRuntime   func(*Console) ConnectorOAuthRuntime
	Available      func(*Console) bool
}

var clientConnectorDefinitions = []clientConnectorDefinition{
	{ID: "gmail", Name: "Gmail", AuthKind: "oauth_pkce", RequiredScopes: []string{"https://www.googleapis.com/auth/gmail.readonly"}, ScopeFields: []string{}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.gmail }, Available: func(console *Console) bool { return console.gmail != nil }},
	{ID: "microsoft.mail", Name: "Microsoft Mail", AuthKind: "oauth_pkce", RequiredScopes: []string{"Mail.Read"}, ScopeFields: []string{}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.microsoftAuth }, Available: func(console *Console) bool { return console.microsoftAuth != nil }},
	{ID: "github.issues", Name: "GitHub Issues", AuthKind: "secret", CredentialName: githubTokenKey, RequiredScopes: []string{"github.issues.read"}, ScopeFields: []string{"owner", "repository"}, Available: func(*Console) bool { return true }},
	{ID: "slack.conversations", Name: "Slack", AuthKind: "secret", CredentialName: slackTokenKey, RequiredScopes: []string{"slack.selected_conversation.read"}, ScopeFields: []string{"channel", "thread"}, Available: func(*Console) bool { return true }},
	{ID: "google_drive.files", Name: "Google Drive", AuthKind: "oauth_pkce", RequiredScopes: []string{"https://www.googleapis.com/auth/drive.readonly"}, ScopeFields: []string{"folder_id"}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.driveAuth }, Available: func(console *Console) bool { return console.driveAuth != nil }},
	{ID: "calendar.google", Name: "Google Calendar", AuthKind: "oauth_pkce", RequiredScopes: []string{"https://www.googleapis.com/auth/calendar.readonly"}, ScopeFields: []string{"calendar_id"}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.calendarAuth }, Available: func(console *Console) bool { return console.calendarAuth != nil }},
	{ID: "calendar.microsoft", Name: "Microsoft Calendar", AuthKind: "oauth_pkce", RequiredScopes: []string{"Calendars.Read"}, ScopeFields: []string{"calendar_id"}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.microsoftCalendarAuth }, Available: func(console *Console) bool { return console.microsoftCalendarAuth != nil }},
	{ID: "microsoft.teams", Name: "Microsoft Teams", AuthKind: "oauth_pkce", RequiredScopes: []string{"ChannelMessage.Read.All"}, ScopeFields: []string{"team_id", "channel_id"}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.microsoftTeamsAuth }, Available: func(console *Console) bool { return console.microsoftTeamsAuth != nil }},
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
		if connected {
			status = "connected"
			var latest *connectorAttempt
			for _, attempt := range console.connectorAttempts {
				if attempt.ConnectionID == connection.ConnectionID && (latest == nil || attempt.CreatedAt.After(latest.CreatedAt)) {
					latest = attempt
				}
			}
			if latest != nil && latest.Status == "pending" {
				status = "connecting"
			} else if latest != nil && latest.Status == "failed" {
				status = "error"
			}
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
			item["scope"] = console.connectorScope(definition.ID)
		}
		items = append(items, item)
	}
	reply(writer, http.StatusOK, map[string]any{"schema_version": 1, "person_id": scope.PersonID, "device_id": scope.DeviceID, "legacy_unscoped": scope.Legacy, "connectors": items})
}

func (console *Console) startClientConnector(writer http.ResponseWriter, request *http.Request, scope clientScope, definition clientConnectorDefinition) {
	if scope.Legacy {
		failure(writer, http.StatusForbidden, "person_scope_required")
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
	console.mu.Lock()
	if _, exists := console.connectionForPerson(definition.ID, scope.PersonID); exists {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "already_connected")
		return
	}
	if console.connectionExistsForOtherPerson(definition.ID, scope.PersonID) {
		console.mu.Unlock()
		failure(writer, http.StatusForbidden, "connection_owned_by_another_person")
		return
	}
	if !definition.Available(console) {
		console.mu.Unlock()
		failure(writer, http.StatusServiceUnavailable, "connector_unavailable")
		return
	}
	connectionID := definition.ID + "." + digest(scope.PersonID + "\x00" + definition.ID)[:16]
	record := connectionRecord{ConnectionID: connectionID, ConnectorID: definition.ID, PersonID: scope.PersonID}
	previous := cloneState(console.state)
	if err := console.applyClientConnectorScope(definition, input.Scope); err != nil {
		console.state = previous
		console.mu.Unlock()
		failure(writer, http.StatusBadRequest, "invalid_scope")
		return
	}
	if definition.AuthKind == "secret" {
		credentialName, err := credentials.ConnectionName(definition.CredentialName, connectionID, scope.PersonID)
		if err != nil || console.vault.Put(credentialName, input.Secret) != nil {
			console.state = previous
			console.mu.Unlock()
			failure(writer, http.StatusServiceUnavailable, "credential_store_unavailable")
			return
		}
		if err := console.setClientSecretCredential(definition.ID, credentialName); err != nil || console.persistClientConnection(record) != nil {
			_ = console.vault.Delete(credentialName)
			console.state = previous
			_ = console.rebuildConnectorRuntimes()
			console.mu.Unlock()
			failure(writer, http.StatusInternalServerError, "save_failed")
			return
		}
		attempt := console.newConnectorAttempt(definition.ID, connectionID, scope, "connected", "")
		console.mu.Unlock()
		reply(writer, http.StatusCreated, connectorAttemptResponse(attempt))
		return
	}
	runtime := definition.OAuthRuntime(console)
	console.state = previous
	console.mu.Unlock()
	ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
	defer cancel()
	value, err := runtime.Action(ctx, "login")
	if err != nil {
		failure(writer, http.StatusBadGateway, "connector_authorization_unavailable")
		return
	}
	status, authorizationURL, valid := oauthActionStatus(value)
	if !valid || status != "pending" && status != "connected" {
		failure(writer, http.StatusBadGateway, "invalid_connector_response")
		return
	}
	console.mu.Lock()
	current := cloneState(console.state)
	_, raced := console.connectionForPerson(definition.ID, scope.PersonID)
	foreign := console.connectionExistsForOtherPerson(definition.ID, scope.PersonID)
	if raced || foreign {
		console.mu.Unlock()
		_, _ = runtime.Action(context.Background(), "cancel")
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	if err := console.applyClientConnectorScope(definition, input.Scope); err != nil || console.rebuildConnectorRuntimes() != nil || console.persistClientConnection(record) != nil {
		console.state = current
		_ = console.rebuildConnectorRuntimes()
		console.mu.Unlock()
		_, _ = runtime.Action(context.Background(), "cancel")
		failure(writer, http.StatusInternalServerError, "save_failed")
		return
	}
	attempt := console.newConnectorAttempt(definition.ID, connectionID, scope, status, authorizationURL)
	console.mu.Unlock()
	reply(writer, http.StatusCreated, connectorAttemptResponse(attempt))
}

func (console *Console) writeClientConnectorAttempt(writer http.ResponseWriter, request *http.Request, scope clientScope, definition clientConnectorDefinition, attemptID string) {
	console.mu.Lock()
	attempt, exists := console.connectorAttempts[attemptID]
	if !exists || attempt.ConnectorID != definition.ID || attempt.PersonID != scope.PersonID || attempt.DeviceID != scope.DeviceID {
		console.mu.Unlock()
		failure(writer, http.StatusNotFound, "attempt_not_found")
		return
	}
	copy := *attempt
	runtime := ConnectorOAuthRuntime(nil)
	if definition.OAuthRuntime != nil && copy.Status == "pending" {
		runtime = definition.OAuthRuntime(console)
	}
	console.mu.Unlock()
	if runtime != nil {
		ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
		defer cancel()
		value, err := runtime.Action(ctx, "status")
		if err != nil {
			copy.Status, copy.ErrorCode = "failed", "connector_authorization_unavailable"
		} else if status, authorizationURL, valid := oauthActionStatus(value); valid {
			copy.Status, copy.AuthorizationURL = status, authorizationURL
		} else {
			copy.Status, copy.ErrorCode = "failed", "invalid_connector_response"
		}
		console.mu.Lock()
		if current := console.connectorAttempts[attemptID]; current != nil && current.PersonID == scope.PersonID {
			*current = copy
		}
		console.mu.Unlock()
	}
	reply(writer, http.StatusOK, connectorAttemptResponse(&copy))
}

func (console *Console) cancelClientConnectorAttempt(writer http.ResponseWriter, request *http.Request, scope clientScope, definition clientConnectorDefinition, attemptID string) {
	if scope.Legacy {
		failure(writer, http.StatusForbidden, "person_scope_required")
		return
	}
	console.mu.Lock()
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
	if attempt.Status != "pending" {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "attempt_not_pending")
		return
	}
	runtime := definition.OAuthRuntime(console)
	console.mu.Unlock()
	ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
	defer cancel()
	if _, err := runtime.Action(ctx, "cancel"); err != nil {
		failure(writer, http.StatusBadGateway, "connector_authorization_unavailable")
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
	previous := cloneState(console.state)
	console.clearClientConnectorConfiguration(definition.ID)
	delete(console.state.Connections, attempt.ConnectionID)
	if err := console.rebuildConnectorRuntimes(); err != nil || console.save(console.state) != nil {
		console.state = previous
		_ = console.rebuildConnectorRuntimes()
		console.mu.Unlock()
		failure(writer, http.StatusInternalServerError, "save_failed")
		return
	}
	console.mu.Unlock()
	copy := *attempt
	reply(writer, http.StatusOK, connectorAttemptResponse(&copy))
}

func (console *Console) updateClientConnectorScope(writer http.ResponseWriter, request *http.Request, scope clientScope, definition clientConnectorDefinition) {
	if scope.Legacy {
		failure(writer, http.StatusForbidden, "person_scope_required")
		return
	}
	if len(definition.ScopeFields) == 0 {
		failure(writer, http.StatusConflict, "capability_not_supported")
		return
	}
	var input struct {
		SchemaVersion int            `json:"schema_version"`
		Scope         map[string]any `json:"scope"`
	}
	if !decode(writer, request, &input) || input.SchemaVersion != 1 {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	console.mu.Lock()
	defer console.mu.Unlock()
	record, exists := console.connectionForPerson(definition.ID, scope.PersonID)
	if !exists {
		if console.connectionExistsForOtherPerson(definition.ID, scope.PersonID) {
			failure(writer, http.StatusForbidden, "connection_owned_by_another_person")
		} else {
			failure(writer, http.StatusNotFound, "connection_not_found")
		}
		return
	}
	previous := cloneState(console.state)
	if err := console.applyClientConnectorScope(definition, input.Scope); err != nil || console.rebuildConnectorRuntimes() != nil || console.save(console.state) != nil {
		console.state = previous
		_ = console.rebuildConnectorRuntimes()
		failure(writer, http.StatusBadRequest, "invalid_scope")
		return
	}
	reply(writer, http.StatusOK, map[string]any{"schema_version": 1, "connection_id": record.ConnectionID, "connector_id": definition.ID, "scope": console.connectorScope(definition.ID)})
}

func (console *Console) disconnectClientConnector(writer http.ResponseWriter, request *http.Request, scope clientScope, definition clientConnectorDefinition) {
	if scope.Legacy {
		failure(writer, http.StatusForbidden, "person_scope_required")
		return
	}
	console.mu.Lock()
	record, exists := console.connectionForPerson(definition.ID, scope.PersonID)
	if !exists {
		foreign := console.connectionExistsForOtherPerson(definition.ID, scope.PersonID)
		console.mu.Unlock()
		if foreign {
			failure(writer, http.StatusForbidden, "connection_owned_by_another_person")
		} else {
			failure(writer, http.StatusNotFound, "connection_not_found")
		}
		return
	}
	runtime := ConnectorOAuthRuntime(nil)
	if definition.OAuthRuntime != nil {
		runtime = definition.OAuthRuntime(console)
	}
	credentialName := console.clientSecretCredential(definition.ID)
	console.mu.Unlock()
	if runtime != nil {
		ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
		defer cancel()
		if _, err := runtime.Action(ctx, "logout"); err != nil {
			failure(writer, http.StatusBadGateway, "connector_authorization_unavailable")
			return
		}
	}
	console.mu.Lock()
	current, stillOwned := console.connectionForPerson(definition.ID, scope.PersonID)
	if !stillOwned || current.ConnectionID != record.ConnectionID {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	previous := cloneState(console.state)
	console.clearClientConnectorConfiguration(definition.ID)
	delete(console.state.Connections, record.ConnectionID)
	if err := console.rebuildConnectorRuntimes(); err != nil || console.save(console.state) != nil {
		console.state = previous
		_ = console.rebuildConnectorRuntimes()
		console.mu.Unlock()
		failure(writer, http.StatusInternalServerError, "save_failed")
		return
	}
	for identifier, attempt := range console.connectorAttempts {
		if attempt.ConnectionID == record.ConnectionID {
			delete(console.connectorAttempts, identifier)
		}
	}
	console.mu.Unlock()
	if credentialName != "" && console.vault.Delete(credentialName) != nil {
		failure(writer, http.StatusInternalServerError, "credential_cleanup_failed")
		return
	}
	reply(writer, http.StatusOK, map[string]any{"schema_version": 1, "disconnected": true})
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

func connectorAttemptResponse(attempt *connectorAttempt) map[string]any {
	value := map[string]any{
		"schema_version": 1, "attempt_id": attempt.ID, "connector_id": attempt.ConnectorID,
		"connection_id": attempt.ConnectionID, "status": attempt.Status,
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
		if now.Sub(attempt.CreatedAt) > 10*time.Minute {
			delete(console.connectorAttempts, identifier)
		}
	}
	if len(console.connectorAttempts) >= 64 {
		var oldestID string
		var oldest time.Time
		for identifier, attempt := range console.connectorAttempts {
			if oldestID == "" || attempt.CreatedAt.Before(oldest) {
				oldestID, oldest = identifier, attempt.CreatedAt
			}
		}
		delete(console.connectorAttempts, oldestID)
	}
	attempt := &connectorAttempt{ID: randomToken(), ConnectorID: connectorID, ConnectionID: connectionID, PersonID: scope.PersonID, DeviceID: scope.DeviceID, Status: status, AuthorizationURL: authorizationURL, CreatedAt: now}
	console.connectorAttempts[attempt.ID] = attempt
	return attempt
}

func (console *Console) connectionForPerson(connectorID, personID string) (connectionRecord, bool) {
	for _, record := range console.state.Connections {
		if record.ConnectorID == connectorID && record.PersonID == personID {
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

func (console *Console) persistClientConnection(record connectionRecord) error {
	next := cloneState(console.state)
	next.Connections[record.ConnectionID] = record
	if err := console.save(next); err != nil {
		return err
	}
	console.state = next
	return nil
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

func (console *Console) applyClientConnectorScope(definition clientConnectorDefinition, scope map[string]any) error {
	if scope == nil {
		scope = map[string]any{}
	}
	if !onlyScopeFields(scope, definition.ScopeFields...) {
		return errors.New("unknown scope field")
	}
	switch definition.ID {
	case "gmail", "microsoft.mail":
		if len(scope) != 0 {
			return errors.New("scope unsupported")
		}
	case "github.issues":
		owner, ownerOK := scopeString(scope, "owner")
		repository, repositoryOK := scopeString(scope, "repository")
		if !ownerOK || !repositoryOK || owner == "" || repository == "" || len(owner) > 128 || len(repository) > 128 {
			return errors.New("invalid github scope")
		}
		credential := console.clientSecretCredential(definition.ID)
		console.state.Connectors.GitHub = &githubConnectorConfig{Owner: owner, Repository: repository, Credential: credential}
	case "slack.conversations":
		channel, channelOK := scopeString(scope, "channel")
		thread := ""
		if value, exists := scope["thread"]; exists {
			var ok bool
			thread, ok = value.(string)
			if !ok || thread != strings.TrimSpace(thread) {
				return errors.New("invalid slack thread")
			}
		}
		if !channelOK || channel == "" || len(channel) > 128 || len(thread) > 128 {
			return errors.New("invalid slack scope")
		}
		credential := console.clientSecretCredential(definition.ID)
		console.state.Connectors.Slack = &slackConnectorConfig{Channel: channel, Thread: thread, Credential: credential}
	case "home_assistant.states":
		baseURL, baseURLOK := scopeString(scope, "base_url")
		items, itemsOK := scope["entities"].([]any)
		entities := make([]string, 0, len(items))
		for _, item := range items {
			value, ok := item.(string)
			if !ok || value == "" || value != strings.TrimSpace(value) {
				return errors.New("invalid entity")
			}
			entities = append(entities, value)
		}
		if !baseURLOK || !itemsOK || baseURL == "" || len(entities) == 0 || len(entities) > 128 {
			return errors.New("invalid home assistant scope")
		}
		credential := console.clientSecretCredential(definition.ID)
		console.state.Connectors.HomeAssistant = &homeAssistantConnectorConfig{BaseURL: baseURL, Entities: entities, Credential: credential}
	case "google_drive.files":
		folderID, ok := scopeString(scope, "folder_id")
		if !ok || folderID == "" || len(folderID) > 256 {
			return errors.New("invalid drive scope")
		}
		console.state.Connectors.GoogleDrive = &googleDriveConnectorConfig{FolderID: folderID}
	case "calendar.google":
		calendarID, ok := scopeString(scope, "calendar_id")
		if !ok || calendarID == "" || len(calendarID) > 256 {
			return errors.New("invalid calendar scope")
		}
		console.state.Connectors.GoogleCalendar = &googleCalendarConnectorConfig{CalendarID: calendarID}
	case "calendar.microsoft":
		calendarID, ok := scopeString(scope, "calendar_id")
		if !ok || calendarID == "" || len(calendarID) > 256 {
			return errors.New("invalid calendar scope")
		}
		console.state.Connectors.MicrosoftCalendar = &microsoftCalendarConnectorConfig{CalendarID: calendarID}
	case "microsoft.teams":
		teamID, teamOK := scopeString(scope, "team_id")
		channelID, channelOK := scopeString(scope, "channel_id")
		if !teamOK || !channelOK || teamID == "" || channelID == "" || len(teamID) > 256 || len(channelID) > 256 {
			return errors.New("invalid teams scope")
		}
		console.state.Connectors.MicrosoftTeams = &microsoftTeamsConnectorConfig{TeamID: teamID, ChannelID: channelID}
	default:
		return errors.New("unknown connector")
	}
	return nil
}

func (console *Console) setClientSecretCredential(connectorID, credentialName string) error {
	switch connectorID {
	case "github.issues":
		console.state.Connectors.GitHub.Credential = credentialName
	case "slack.conversations":
		console.state.Connectors.Slack.Credential = credentialName
	case "home_assistant.states":
		console.state.Connectors.HomeAssistant.Credential = credentialName
	default:
		return errors.New("not a secret connector")
	}
	return console.rebuildConnectorRuntimes()
}

func (console *Console) clientSecretCredential(connectorID string) string {
	switch connectorID {
	case "github.issues":
		if console.state.Connectors.GitHub != nil {
			return console.state.Connectors.GitHub.Credential
		}
	case "slack.conversations":
		if console.state.Connectors.Slack != nil {
			return console.state.Connectors.Slack.Credential
		}
	case "home_assistant.states":
		if console.state.Connectors.HomeAssistant != nil {
			return console.state.Connectors.HomeAssistant.Credential
		}
	}
	return ""
}

func (console *Console) clearClientConnectorConfiguration(connectorID string) {
	switch connectorID {
	case "github.issues":
		console.state.Connectors.GitHub = nil
	case "slack.conversations":
		console.state.Connectors.Slack = nil
	case "home_assistant.states":
		console.state.Connectors.HomeAssistant = nil
	case "google_drive.files":
		console.state.Connectors.GoogleDrive = nil
	case "calendar.google":
		console.state.Connectors.GoogleCalendar = nil
	case "calendar.microsoft":
		console.state.Connectors.MicrosoftCalendar = nil
	case "microsoft.teams":
		console.state.Connectors.MicrosoftTeams = nil
	}
}

func (console *Console) connectorScope(connectorID string) map[string]any {
	switch connectorID {
	case "github.issues":
		if value := console.state.Connectors.GitHub; value != nil {
			return map[string]any{"owner": value.Owner, "repository": value.Repository}
		}
	case "slack.conversations":
		if value := console.state.Connectors.Slack; value != nil {
			return map[string]any{"channel": value.Channel, "thread": value.Thread}
		}
	case "home_assistant.states":
		if value := console.state.Connectors.HomeAssistant; value != nil {
			return map[string]any{"base_url": value.BaseURL, "entities": value.Entities}
		}
	case "google_drive.files":
		if value := console.state.Connectors.GoogleDrive; value != nil {
			return map[string]any{"folder_id": value.FolderID}
		}
	case "calendar.google":
		if value := console.state.Connectors.GoogleCalendar; value != nil {
			return map[string]any{"calendar_id": value.CalendarID}
		}
	case "calendar.microsoft":
		if value := console.state.Connectors.MicrosoftCalendar; value != nil {
			return map[string]any{"calendar_id": value.CalendarID}
		}
	case "microsoft.teams":
		if value := console.state.Connectors.MicrosoftTeams; value != nil {
			return map[string]any{"team_id": value.TeamID, "channel_id": value.ChannelID}
		}
	}
	return map[string]any{}
}
