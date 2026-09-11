package console

import (
	"context"
	"errors"

	githubconnector "floe/server/internal/connectors/github"
	calendarconnector "floe/server/internal/connectors/googlecalendar"
	driveconnector "floe/server/internal/connectors/googledrive"
	homeconnector "floe/server/internal/connectors/homeassistant"
	microsoftcalendarconnector "floe/server/internal/connectors/microsoftcalendar"
	microsoftteamsconnector "floe/server/internal/connectors/microsoftteams"
	slackconnector "floe/server/internal/connectors/slack"
)

const (
	githubTokenKey = "FLOE_CONNECTOR_GITHUB_TOKEN"
	slackTokenKey  = "FLOE_CONNECTOR_SLACK_TOKEN"
	homeTokenKey   = "FLOE_CONNECTOR_HOME_ASSISTANT_TOKEN"
)

type vaultTokenSource struct {
	vault Vault
	name  string
}

func (source vaultTokenSource) Token(context.Context) (string, error) {
	token, err := source.vault.Get(source.name)
	if err != nil || token == "" {
		return "", errors.New("credential unavailable")
	}
	return token, nil
}

func (console *Console) rebuildConnectorRuntimes() error {
	console.work = map[string]WorkContextRuntime{}
	console.logistics = map[string]LogisticsRuntime{}
	console.calendars = map[string]CalendarRuntime{}
	for _, connection := range console.state.Connections {
		if err := console.rebuildConnectorRuntime(connection); err != nil {
			return err
		}
	}
	return nil
}

func (console *Console) rebuildConnectorRuntime(connection connectionRecord) error {
	scope := connection.Scope
	switch connection.ConnectorID {
	case "github.issues":
		client, err := githubconnector.New(vaultTokenSource{vault: console.vault, name: connection.Credential})
		if err != nil {
			return err
		}
		service, err := githubconnector.NewService(client, scope["owner"].(string), scope["repository"].(string))
		if err == nil {
			console.work[connection.ConnectionID] = service
		}
		return err
	case "slack.conversations":
		client, err := slackconnector.New(vaultTokenSource{vault: console.vault, name: connection.Credential})
		if err != nil {
			return err
		}
		service, err := slackconnector.NewService(client, scope["channel"].(string), scope["thread"].(string))
		if err == nil {
			console.work[connection.ConnectionID] = service
		}
		return err
	case "google_drive.files":
		if console.driveAuth == nil {
			return nil
		}
		client, err := driveconnector.New(console.driveAuth)
		if err != nil {
			return err
		}
		service, err := driveconnector.NewService(client, scope["folder_id"].(string))
		if err == nil {
			console.work[connection.ConnectionID] = service
		}
		return err
	case "calendar.google":
		if console.calendarAuth == nil {
			return nil
		}
		client, err := calendarconnector.New(console.calendarAuth, scope["calendar_id"].(string), connection.ConnectionID)
		if err != nil {
			return err
		}
		service, err := calendarconnector.NewService(client)
		if err == nil {
			console.calendars[connection.ConnectionID] = service
		}
		return err
	case "calendar.microsoft":
		if console.microsoftCalendarAuth == nil {
			return nil
		}
		client, err := microsoftcalendarconnector.New(console.microsoftCalendarAuth, scope["calendar_id"].(string), connection.ConnectionID)
		if err != nil {
			return err
		}
		service, err := microsoftcalendarconnector.NewService(client)
		if err == nil {
			console.calendars[connection.ConnectionID] = service
		}
		return err
	case "microsoft.teams":
		if console.microsoftTeamsAuth == nil {
			return nil
		}
		client, err := microsoftteamsconnector.New(console.microsoftTeamsAuth)
		if err != nil {
			return err
		}
		service, err := microsoftteamsconnector.NewService(client, scope["team_id"].(string), scope["channel_id"].(string))
		if err == nil {
			console.work[connection.ConnectionID] = service
		}
		return err
	case "home_assistant.states":
		entities, ok := connectorScopeStrings(scope["entities"])
		if !ok {
			return errors.New("invalid connector scope")
		}
		client, err := homeconnector.New(vaultTokenSource{vault: console.vault, name: connection.Credential}, scope["base_url"].(string), connection.ConnectionID)
		if err != nil {
			return err
		}
		service, err := homeconnector.NewService(client, entities)
		if err == nil {
			console.logistics[connection.ConnectionID] = service
		}
		return err
	}
	return nil
}

func connectorScopeStrings(value any) ([]string, bool) {
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
