package console

import (
	"context"
	"errors"
	"net/http"
	"strings"

	githubconnector "floe/server/internal/connectors/github"
	calendarconnector "floe/server/internal/connectors/googlecalendar"
	driveconnector "floe/server/internal/connectors/googledrive"
	homeconnector "floe/server/internal/connectors/homeassistant"
	microsoftcalendarconnector "floe/server/internal/connectors/microsoftcalendar"
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
	console.work = nil
	console.logistics = nil
	console.calendars = nil
	if configured := console.state.Connectors.GitHub; configured != nil {
		client, err := githubconnector.New(vaultTokenSource{vault: console.vault, name: githubTokenKey})
		if err != nil {
			return err
		}
		service, err := githubconnector.NewService(client, configured.Owner, configured.Repository)
		if err != nil {
			return err
		}
		console.work = append(console.work, service)
	}
	if configured := console.state.Connectors.Slack; configured != nil {
		client, err := slackconnector.New(vaultTokenSource{vault: console.vault, name: slackTokenKey})
		if err != nil {
			return err
		}
		service, err := slackconnector.NewService(client, configured.Channel, configured.Thread)
		if err != nil {
			return err
		}
		console.work = append(console.work, service)
	}
	if configured := console.state.Connectors.GoogleDrive; configured != nil && console.driveAuth != nil {
		client, err := driveconnector.New(console.driveAuth)
		if err != nil {
			return err
		}
		service, err := driveconnector.NewService(client, configured.FolderID)
		if err != nil {
			return err
		}
		console.work = append(console.work, service)
	}
	if configured := console.state.Connectors.GoogleCalendar; configured != nil && console.calendarAuth != nil {
		client, err := calendarconnector.New(console.calendarAuth, configured.CalendarID, "primary")
		if err != nil {
			return err
		}
		service, err := calendarconnector.NewService(client)
		if err != nil {
			return err
		}
		console.calendars = append(console.calendars, service)
	}
	if configured := console.state.Connectors.MicrosoftCalendar; configured != nil && console.microsoftCalendarAuth != nil {
		client, err := microsoftcalendarconnector.New(console.microsoftCalendarAuth, configured.CalendarID, "primary")
		if err != nil {
			return err
		}
		service, err := microsoftcalendarconnector.NewService(client)
		if err != nil {
			return err
		}
		console.calendars = append(console.calendars, service)
	}
	if configured := console.state.Connectors.HomeAssistant; configured != nil {
		client, err := homeconnector.New(vaultTokenSource{vault: console.vault, name: homeTokenKey}, configured.BaseURL, "primary")
		if err != nil {
			return err
		}
		service, err := homeconnector.NewService(client, configured.Entities)
		if err != nil {
			return err
		}
		console.logistics = append(console.logistics, service)
	}
	return nil
}

func (console *Console) updateGitHubConnector(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Enabled    bool   `json:"enabled"`
		Owner      string `json:"owner"`
		Repository string `json:"repository"`
		Token      string `json:"token"`
	}
	if !decode(writer, request, &input) || invalidConnectorToken(input.Token) {
		failure(writer, 400, "validation")
		return
	}
	next := cloneState(console.state)
	if !input.Enabled {
		next.Connectors.GitHub = nil
		if !console.saveConnectorState(writer, next, githubTokenKey, "") {
			return
		}
		return
	}
	client, err := githubconnector.New(vaultTokenSource{vault: console.vault, name: githubTokenKey})
	if err != nil {
		failure(writer, 400, "validation")
		return
	}
	if _, err = githubconnector.NewService(client, input.Owner, input.Repository); err != nil || input.Token == "" && console.state.Connectors.GitHub == nil {
		failure(writer, 400, "validation")
		return
	}
	next.Connectors.GitHub = &githubConnectorConfig{Owner: input.Owner, Repository: input.Repository}
	console.saveConnectorState(writer, next, githubTokenKey, input.Token)
}

func (console *Console) updateHomeAssistantConnector(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Enabled  bool     `json:"enabled"`
		BaseURL  string   `json:"base_url"`
		Entities []string `json:"entities"`
		Token    string   `json:"token"`
	}
	if !decode(writer, request, &input) || invalidConnectorToken(input.Token) {
		failure(writer, 400, "validation")
		return
	}
	next := cloneState(console.state)
	if !input.Enabled {
		next.Connectors.HomeAssistant = nil
		if !console.saveConnectorState(writer, next, homeTokenKey, "") {
			return
		}
		return
	}
	client, err := homeconnector.New(vaultTokenSource{vault: console.vault, name: homeTokenKey}, input.BaseURL, "primary")
	if err != nil {
		failure(writer, 400, "validation")
		return
	}
	if _, err = homeconnector.NewService(client, input.Entities); err != nil || input.Token == "" && console.state.Connectors.HomeAssistant == nil {
		failure(writer, 400, "validation")
		return
	}
	next.Connectors.HomeAssistant = &homeAssistantConnectorConfig{BaseURL: input.BaseURL, Entities: append([]string(nil), input.Entities...)}
	console.saveConnectorState(writer, next, homeTokenKey, input.Token)
}

func (console *Console) updateSlackConnector(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Enabled bool   `json:"enabled"`
		Channel string `json:"channel"`
		Thread  string `json:"thread"`
		Token   string `json:"token"`
	}
	if !decode(writer, request, &input) || invalidConnectorToken(input.Token) {
		failure(writer, 400, "validation")
		return
	}
	next := cloneState(console.state)
	if !input.Enabled {
		next.Connectors.Slack = nil
		console.saveConnectorState(writer, next, slackTokenKey, "")
		return
	}
	client, err := slackconnector.New(vaultTokenSource{vault: console.vault, name: slackTokenKey})
	if err != nil {
		failure(writer, 400, "validation")
		return
	}
	if _, err = slackconnector.NewService(client, input.Channel, input.Thread); err != nil || input.Token == "" && console.state.Connectors.Slack == nil {
		failure(writer, 400, "validation")
		return
	}
	next.Connectors.Slack = &slackConnectorConfig{Channel: input.Channel, Thread: input.Thread}
	console.saveConnectorState(writer, next, slackTokenKey, input.Token)
}

func (console *Console) updateGoogleDriveConnector(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Enabled  bool   `json:"enabled"`
		FolderID string `json:"folder_id"`
	}
	if !decode(writer, request, &input) {
		failure(writer, 400, "validation")
		return
	}
	next := cloneState(console.state)
	if !input.Enabled {
		next.Connectors.GoogleDrive = nil
	} else {
		if console.driveAuth == nil {
			failure(writer, 503, "drive_unavailable")
			return
		}
		client, err := driveconnector.New(console.driveAuth)
		if err != nil {
			failure(writer, 400, "validation")
			return
		}
		if _, err := driveconnector.NewService(client, input.FolderID); err != nil {
			failure(writer, 400, "validation")
			return
		}
		next.Connectors.GoogleDrive = &googleDriveConnectorConfig{FolderID: input.FolderID}
	}
	if console.save(next) != nil {
		failure(writer, 500, "save_failed")
		return
	}
	console.state = next
	if err := console.rebuildConnectorRuntimes(); err != nil {
		failure(writer, 500, "invalid_connector_configuration")
		return
	}
	reply(writer, 200, map[string]bool{"ok": true})
}

func (console *Console) updateGoogleCalendarConnector(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Enabled    bool   `json:"enabled"`
		CalendarID string `json:"calendar_id"`
	}
	if !decode(writer, request, &input) {
		failure(writer, 400, "validation")
		return
	}
	next := cloneState(console.state)
	if !input.Enabled {
		next.Connectors.GoogleCalendar = nil
	} else {
		if console.calendarAuth == nil {
			failure(writer, 503, "calendar_unavailable")
			return
		}
		client, err := calendarconnector.New(console.calendarAuth, input.CalendarID, "primary")
		if err != nil {
			failure(writer, 400, "validation")
			return
		}
		if _, err := calendarconnector.NewService(client); err != nil {
			failure(writer, 400, "validation")
			return
		}
		next.Connectors.GoogleCalendar = &googleCalendarConnectorConfig{CalendarID: input.CalendarID}
	}
	if console.save(next) != nil {
		failure(writer, 500, "save_failed")
		return
	}
	console.state = next
	if err := console.rebuildConnectorRuntimes(); err != nil {
		failure(writer, 500, "invalid_connector_configuration")
		return
	}
	reply(writer, 200, map[string]bool{"ok": true})
}

func (console *Console) updateMicrosoftCalendarConnector(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Enabled    bool   `json:"enabled"`
		CalendarID string `json:"calendar_id"`
	}
	if !decode(writer, request, &input) {
		failure(writer, 400, "validation")
		return
	}
	next := cloneState(console.state)
	if !input.Enabled {
		next.Connectors.MicrosoftCalendar = nil
	} else {
		if console.microsoftCalendarAuth == nil {
			failure(writer, 503, "microsoft_calendar_unavailable")
			return
		}
		client, err := microsoftcalendarconnector.New(console.microsoftCalendarAuth, input.CalendarID, "primary")
		if err != nil {
			failure(writer, 400, "validation")
			return
		}
		if _, err := microsoftcalendarconnector.NewService(client); err != nil {
			failure(writer, 400, "validation")
			return
		}
		next.Connectors.MicrosoftCalendar = &microsoftCalendarConnectorConfig{CalendarID: input.CalendarID}
	}
	if console.save(next) != nil {
		failure(writer, 500, "save_failed")
		return
	}
	console.state = next
	if err := console.rebuildConnectorRuntimes(); err != nil {
		failure(writer, 500, "invalid_connector_configuration")
		return
	}
	reply(writer, 200, map[string]bool{"ok": true})
}

func invalidConnectorToken(token string) bool {
	return token != "" && len(token) < 8 || len(token) > 4096 || strings.ContainsAny(token, "\r\n\x00")
}

func (console *Console) saveConnectorState(writer http.ResponseWriter, next diskState, tokenKey, token string) bool {
	previous, _ := console.vault.Get(tokenKey)
	if token != "" && console.vault.Put(tokenKey, token) != nil {
		failure(writer, 503, "credential_store_unavailable")
		return false
	}
	if console.save(next) != nil {
		if token != "" {
			if previous == "" {
				_ = console.vault.Delete(tokenKey)
			} else {
				_ = console.vault.Put(tokenKey, previous)
			}
		}
		failure(writer, 500, "save_failed")
		return false
	}
	console.state = next
	if err := console.rebuildConnectorRuntimes(); err != nil {
		failure(writer, 500, "invalid_connector_configuration")
		return false
	}
	if token == "" && previous != "" && connectorTokenUnused(next, tokenKey) {
		if console.vault.Delete(tokenKey) != nil {
			failure(writer, 500, "credential_cleanup_failed")
			return false
		}
	}
	reply(writer, 200, map[string]bool{"ok": true})
	return true
}

func connectorTokenUnused(state diskState, tokenKey string) bool {
	return tokenKey == githubTokenKey && state.Connectors.GitHub == nil || tokenKey == slackTokenKey && state.Connectors.Slack == nil || tokenKey == homeTokenKey && state.Connectors.HomeAssistant == nil
}
