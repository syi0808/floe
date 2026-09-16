package connections

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

func invalidConnectorToken(token string) bool {
	return token != "" && len(token) < 8 || len(token) > 4096 || strings.ContainsAny(token, "\r\n\x00")
}

type connectorAttempt struct {
	ID               string
	ClientID         string
	ConnectorID      string
	ConnectionID     string
	PersonID         string
	DeviceID         string
	Status           string
	AuthorizationURL string
	UserCode         string
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
	{ID: "github.issues", Name: "GitHub Issues", AuthKind: "oauth_device", OAuthCredential: "FLOE_GITHUB_OAUTH", RequiredScopes: []string{"github.issues.read"}, ScopeFields: []string{"owner", "repository"}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.githubAuth }, Available: func(console *Console) bool { return console.githubAuth != nil }},
	{ID: "slack.conversations", Name: "Slack", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_SLACK_OAUTH", RequiredScopes: []string{"channels:history", "groups:history"}, ScopeFields: []string{"channel", "thread"}, OAuthRuntime: func(console *Console) ConnectorOAuthRuntime { return console.slackAuth }, Available: func(console *Console) bool { return console.slackAuth != nil }},
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

func connectorCapabilities(definition clientConnectorDefinition) map[string]any {
	return map[string]any{
		"connect":      true,
		"cancel":       isOAuthAuthKind(definition.AuthKind),
		"disconnect":   true,
		"scope_update": len(definition.ScopeFields) > 0,
	}
}

func oauthActionStatus(value any) (string, string, string, bool) {
	statusValue, ok := value.(map[string]any)
	if !ok {
		return "", "", "", false
	}
	status, ok := statusValue["status"].(string)
	if !ok || status != "pending" && status != "connected" && status != "disconnected" {
		return "", "", "", false
	}
	authorizationURL, _ := statusValue["auth_url"].(string)
	userCode, _ := statusValue["user_code"].(string)
	if status == "pending" && authorizationURL == "" {
		return "", "", "", false
	}
	return status, authorizationURL, userCode, true
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
	if attempt.UserCode != "" {
		value["user_code"] = attempt.UserCode
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
	attempt := &connectorAttempt{ID: randomToken(), ClientID: scope.ClientID, ConnectorID: connectorID, ConnectionID: connectionID, PersonID: scope.PersonID, DeviceID: scope.DeviceID, Status: status, AuthorizationURL: authorizationURL, CreatedAt: now}
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
	if !exists || !isOAuthAuthKind(definition.AuthKind) {
		return true
	}
	runtime := definition.OAuthRuntime(console)
	return runtime != nil && runtime.Ready()
}

func isOAuthAuthKind(value string) bool {
	return value == "oauth_pkce" || value == "oauth_device"
}

func (console *Console) finishClientOAuthAttempt(attemptID string) bool {
	console.mu.Lock()
	attempt := console.connectorAttempts[attemptID]
	durable, durableExists := console.state.Attempts[attemptID]
	if attempt == nil || !durableExists || attempt.Status != "pending" && attempt.Status != "connected" || durable.ConnectionID != attempt.ConnectionID {
		console.mu.Unlock()
		return false
	}
	definition, definitionExists := clientConnectorDefinitionFor(attempt.ConnectorID)
	providerRuntime := ConnectorOAuthRuntime(nil)
	if definitionExists && definition.OAuthRuntime != nil {
		providerRuntime = definition.OAuthRuntime(console)
	}
	attemptConnectionID := attempt.ConnectionID
	attemptConnectorID := attempt.ConnectorID
	attemptPersonID := attempt.PersonID
	attemptScope := cloneConnectorScope(attempt.Scope)
	attemptCredential := attempt.Credential
	incarnation, epoch := durable.Incarnation, durable.Epoch
	console.mu.Unlock()

	providerIdentity := ""
	identityUnverified := true
	if providerRuntime != nil {
		if identityProvider, ok := providerRuntime.(ProviderIdentityRuntime); ok {
			identityContext, cancel := context.WithTimeout(context.Background(), 10*time.Second)
			providerIdentity, _ = identityProvider.ProviderIdentity(identityContext)
			cancel()
			identityUnverified = providerIdentity == ""
		}
	}

	console.mu.Lock()
	attempt = console.connectorAttempts[attemptID]
	durable, durableExists = console.state.Attempts[attemptID]
	if attempt == nil || !durableExists || attempt.ConnectionID != attemptConnectionID || attempt.ConnectorID != attemptConnectorID || attempt.PersonID != attemptPersonID || durable.Incarnation != incarnation || durable.Epoch != epoch {
		console.mu.Unlock()
		return false
	}
	record := connectionRecord{ConnectionID: attemptConnectionID, Revision: 1, ConnectorID: attemptConnectorID, PersonID: attemptPersonID, Scope: attemptScope, Credential: attemptCredential, Incarnation: incarnation, Epoch: epoch, ProviderIdentity: providerIdentity, IdentityUnverified: identityUnverified}
	if !console.connectionCredentialReady(record) {
		console.mu.Unlock()
		return false
	}
	if existing, exists := console.connectionForPerson(record.ConnectorID, record.PersonID); exists && existing.ConnectionID != record.ConnectionID {
		console.mu.Unlock()
		return false
	}
	next := cloneState(console.state)
	next.Connections[record.ConnectionID] = record
	delete(next.Attempts, attemptID)
	previous := console.state
	console.state = next
	if console.rebuildConnectorRuntimes() != nil || console.save(next) != nil {
		console.state = previous
		_ = console.rebuildConnectorRuntimes()
		console.mu.Unlock()
		return false
	}
	attempt.Status, attempt.AuthorizationURL, attempt.ErrorCode = "connected", "", ""
	console.mu.Unlock()
	return true
}

func (console *Console) failClientOAuthAttempt(attemptID string, runtime ConnectorOAuthRuntime) {
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	_ = console.cleanupClientOAuthAttempt(ctx, attemptID, runtime)
}

func (console *Console) cleanupClientOAuthAttempt(ctx context.Context, attemptID string, runtime ConnectorOAuthRuntime) error {
	console.mu.Lock()
	attempt, exists := console.state.Attempts[attemptID]
	if !exists {
		console.mu.Unlock()
		return errors.New("attempt cleanup unavailable")
	}
	next := cloneState(console.state)
	delete(next.Attempts, attemptID)
	cleanup := next.Cleanups[attempt.PersonID]
	cleanup.PersonID = attempt.PersonID
	cleanup.Connections = append(cleanup.Connections, connectionCleanupStep{
		ConnectionID: attempt.ConnectionID, ConnectorID: attempt.ConnectorID, Credential: attempt.Credential,
	})
	next.Cleanups[attempt.PersonID] = cleanup
	if err := console.save(next); err != nil {
		console.mu.Unlock()
		return err
	}
	console.state = next
	delete(console.connectorAttempts, attemptID)
	record := connectionRecord{ConnectionID: attempt.ConnectionID, ConnectorID: attempt.ConnectorID, PersonID: attempt.PersonID, Credential: attempt.Credential}
	console.connectorReservations[attempt.ConnectionID] = record
	console.mu.Unlock()

	runtimeComplete := false
	if runtime != nil && runtime.BindCredential(attempt.Credential) == nil {
		_, err := runtime.Action(ctx, "logout")
		runtimeComplete = err == nil
	}
	if runtimeComplete {
		console.mu.Lock()
		_ = console.completeReservedCleanupLocked(record, true, false)
		console.mu.Unlock()
	}
	vaultComplete := attempt.Credential == ""
	if runtimeComplete && !vaultComplete {
		vaultComplete = console.vault.Delete(attempt.Credential) == nil
	}
	console.mu.Lock()
	delete(console.connectorReservations, attempt.ConnectionID)
	err := console.completeReservedCleanupLocked(record, runtimeComplete, vaultComplete)
	console.mu.Unlock()
	if !runtimeComplete || !vaultComplete || err != nil {
		return errors.New("connection cleanup pending")
	}
	return nil
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
