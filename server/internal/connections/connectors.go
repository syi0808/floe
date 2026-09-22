package connections

import (
	"errors"
	"strings"
	"time"
)

type Definition struct {
	ID              string
	Name            string
	AuthKind        string
	CredentialName  string
	OAuthCredential string
	RequiredScopes  []string
	ScopeFields     []string
}

var Definitions = []Definition{
	{ID: "gmail", Name: "Gmail", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_GMAIL_OAUTH", RequiredScopes: []string{"https://www.googleapis.com/auth/gmail.readonly"}, ScopeFields: []string{}},
	{ID: "microsoft.mail", Name: "Microsoft Mail", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_MICROSOFT_MAIL_OAUTH", RequiredScopes: []string{"Mail.Read"}, ScopeFields: []string{}},
	{ID: "github.issues", Name: "GitHub Issues", AuthKind: "oauth_device", OAuthCredential: "FLOE_GITHUB_OAUTH", RequiredScopes: []string{"github.issues.read"}, ScopeFields: []string{"owner", "repository"}},
	{ID: "slack.conversations", Name: "Slack", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_SLACK_OAUTH", RequiredScopes: []string{"channels:history", "groups:history"}, ScopeFields: []string{"channel", "thread"}},
	{ID: "google_drive.files", Name: "Google Drive", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_DRIVE_OAUTH", RequiredScopes: []string{"https://www.googleapis.com/auth/drive.readonly"}, ScopeFields: []string{"folder_id"}},
	{ID: "calendar.google", Name: "Google Calendar", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_GOOGLE_CALENDAR_OAUTH", RequiredScopes: []string{"https://www.googleapis.com/auth/calendar.readonly"}, ScopeFields: []string{"calendar_id"}},
	{ID: "calendar.microsoft", Name: "Microsoft Calendar", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_MICROSOFT_CALENDAR_OAUTH", RequiredScopes: []string{"Calendars.Read"}, ScopeFields: []string{"calendar_id"}},
	{ID: "microsoft.teams", Name: "Microsoft Teams", AuthKind: "oauth_pkce", OAuthCredential: "FLOE_MICROSOFT_TEAMS_OAUTH", RequiredScopes: []string{"ChannelMessage.Read.All"}, ScopeFields: []string{"team_id", "channel_id"}},
	{ID: "home_assistant.states", Name: "Home Assistant", AuthKind: "secret", CredentialName: HomeTokenKey, RequiredScopes: []string{"home.states.read"}, ScopeFields: []string{"base_url", "entities"}},
}

func DefinitionFor(identifier string) (Definition, bool) {
	for _, definition := range Definitions {
		if definition.ID == identifier {
			return definition, true
		}
	}
	return Definition{}, false
}

func InvalidConnectorToken(token string) bool {
	return token != "" && len(token) < 8 || len(token) > 4096 || strings.ContainsAny(token, "\r\n\x00")
}

func ConnectorCapabilities(definition Definition) map[string]any {
	return map[string]any{
		"connect":      true,
		"cancel":       IsOAuthAuthKind(definition.AuthKind),
		"disconnect":   true,
		"scope_update": len(definition.ScopeFields) > 0,
	}
}

func OauthActionStatus(value any) (string, string, string, bool) {
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

func ConnectorAttemptResponse(attempt *Attempt) map[string]any {
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

func IsOAuthAuthKind(value string) bool {
	return value == "oauth_pkce" || value == "oauth_device"
}

func IsCalendarConnector(connectorID string) bool {
	return connectorID == "calendar.google" || connectorID == "calendar.microsoft"
}

func OnlyScopeFields(scope map[string]any, allowed ...string) bool {
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

func ScopeString(scope map[string]any, key string) (string, bool) {
	value, ok := scope[key].(string)
	return strings.TrimSpace(value), ok && value == strings.TrimSpace(value)
}

func ValidatedConnectorScope(definition Definition, scope map[string]any) (map[string]any, error) {
	if scope == nil {
		scope = map[string]any{}
	}
	if !OnlyScopeFields(scope, definition.ScopeFields...) {
		return nil, errors.New("unknown scope field")
	}
	switch definition.ID {
	case "gmail", "microsoft.mail":
		if len(scope) != 0 {
			return nil, errors.New("scope unsupported")
		}
		return map[string]any{}, nil
	case "github.issues":
		owner, ownerOK := ScopeString(scope, "owner")
		repository, repositoryOK := ScopeString(scope, "repository")
		if !ownerOK || !repositoryOK || owner == "" || repository == "" || len(owner) > 128 || len(repository) > 128 {
			return nil, errors.New("invalid github scope")
		}
		return map[string]any{"owner": owner, "repository": repository}, nil
	case "slack.conversations":
		channel, channelOK := ScopeString(scope, "channel")
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
		baseURL, baseURLOK := ScopeString(scope, "base_url")
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
		folderID, ok := ScopeString(scope, "folder_id")
		if !ok || folderID == "" || len(folderID) > 256 {
			return nil, errors.New("invalid drive scope")
		}
		return map[string]any{"folder_id": folderID}, nil
	case "calendar.google":
		calendarID, ok := ScopeString(scope, "calendar_id")
		if !ok || calendarID == "" || len(calendarID) > 256 {
			return nil, errors.New("invalid calendar scope")
		}
		return map[string]any{"calendar_id": calendarID}, nil
	case "calendar.microsoft":
		calendarID, ok := ScopeString(scope, "calendar_id")
		if !ok || calendarID == "" || len(calendarID) > 256 {
			return nil, errors.New("invalid calendar scope")
		}
		return map[string]any{"calendar_id": calendarID}, nil
	case "microsoft.teams":
		teamID, teamOK := ScopeString(scope, "team_id")
		channelID, channelOK := ScopeString(scope, "channel_id")
		if !teamOK || !channelOK || teamID == "" || channelID == "" || len(teamID) > 256 || len(channelID) > 256 {
			return nil, errors.New("invalid teams scope")
		}
		return map[string]any{"team_id": teamID, "channel_id": channelID}, nil
	default:
		return nil, errors.New("unknown connector")
	}
}

func CloneConnectorScope(scope map[string]any) map[string]any {
	copy := make(map[string]any, len(scope))
	for key, value := range scope {
		if values, ok := ConnectorScopeStrings(value); ok {
			copy[key] = values
		} else {
			copy[key] = value
		}
	}
	return copy
}

const (
	GithubTokenKey = "FLOE_CONNECTOR_GITHUB_TOKEN"
	SlackTokenKey  = "FLOE_CONNECTOR_SLACK_TOKEN"
	HomeTokenKey   = "FLOE_CONNECTOR_HOME_ASSISTANT_TOKEN"
)

func ConnectorScopeStrings(value any) ([]string, bool) {
	switch values := value.(type) {
	case []string:
		return append([]string(nil), values...), true
	case []any:
		items := make([]string, len(values))
		for index, value := range values {
			item, ok := value.(string)
			if !ok {
				return nil, false
			}
			items[index] = item
		}
		return items, true
	default:
		return nil, false
	}
}
