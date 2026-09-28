package application

import (
	"context"
	"errors"

	"floe/server/internal/connections"
	githubconnector "floe/server/internal/connectors/github"
	calendarconnector "floe/server/internal/connectors/googlecalendar"
	driveconnector "floe/server/internal/connectors/googledrive"
	homeconnector "floe/server/internal/connectors/homeassistant"
	microsoftcalendarconnector "floe/server/internal/connectors/microsoftcalendar"
	microsoftteamsconnector "floe/server/internal/connectors/microsoftteams"
	slackconnector "floe/server/internal/connectors/slack"
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
	console.work = map[string]connections.WorkContextRuntime{}
	console.logistics = map[string]connections.LogisticsRuntime{}
	console.calendars = map[string]connections.CalendarRuntime{}
	for _, connection := range console.state.Connections {
		if err := console.rebuildConnectorRuntime(connection); err != nil {
			return err
		}
	}
	return nil
}

func (console *Console) rebuildConnectorRuntime(connection connections.Record) error {
	scope := connection.Scope
	switch connection.ConnectorID {
	case "github.issues":
		if console.githubAuth == nil {
			return nil
		}
		client, err := githubconnector.New(console.githubAuth)
		if err != nil {
			return err
		}
		service, err := githubconnector.NewService(client, scope["owner"].(string), scope["repository"].(string))
		if err == nil {
			console.work[connection.ConnectionID] = service
		}
		return err
	case "slack.conversations":
		if console.slackAuth == nil {
			return nil
		}
		client, err := slackconnector.New(console.slackAuth)
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
		calendarIDs, ok := connections.ConnectorScopeStrings(scope["calendar_ids"])
		if !ok || len(calendarIDs) == 0 {
			return errors.New("invalid calendar scope")
		}
		clients := make([]*calendarconnector.Client, len(calendarIDs))
		for index, calendarID := range calendarIDs {
			client, err := calendarconnector.New(console.calendarAuth, calendarID, connection.ConnectionID)
			if err != nil {
				return err
			}
			clients[index] = client
		}
		service, err := calendarconnector.NewService(clients...)
		if err == nil {
			console.calendars[connection.ConnectionID] = service
		}
		return err
	case "calendar.microsoft":
		if console.microsoftCalendarAuth == nil {
			return nil
		}
		calendarIDs, ok := connections.ConnectorScopeStrings(scope["calendar_ids"])
		if !ok || len(calendarIDs) == 0 {
			return errors.New("invalid calendar scope")
		}
		clients := make([]*microsoftcalendarconnector.Client, len(calendarIDs))
		for index, calendarID := range calendarIDs {
			client, err := microsoftcalendarconnector.New(console.microsoftCalendarAuth, calendarID, connection.ConnectionID)
			if err != nil {
				return err
			}
			clients[index] = client
		}
		service, err := microsoftcalendarconnector.NewService(clients...)
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
		entities, ok := connections.ConnectorScopeStrings(scope["entities"])
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
