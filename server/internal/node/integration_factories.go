package node

import (
	"context"
	"errors"
	"floe/server/internal/connections"
	githubconnector "floe/server/internal/connectors/github"
	"floe/server/internal/connectors/gmail"
	googleauth "floe/server/internal/connectors/googleauth"
	calendarconnector "floe/server/internal/connectors/googlecalendar"
	driveconnector "floe/server/internal/connectors/googledrive"
	homeconnector "floe/server/internal/connectors/homeassistant"
	"floe/server/internal/connectors/lifecycle"
	microsoftauth "floe/server/internal/connectors/microsoftauth"
	microsoftcalendarconnector "floe/server/internal/connectors/microsoftcalendar"
	"floe/server/internal/connectors/microsoftmail"
	microsoftteamsconnector "floe/server/internal/connectors/microsoftteams"
	slackconnector "floe/server/internal/connectors/slack"
	workoauth "floe/server/internal/connectors/workoauth"
	"floe/server/internal/credentials"
	"floe/server/internal/integrations"
	"path/filepath"
)

type sourceOAuth interface {
	connections.DriveAuthRuntime
	Close()
}
type identifiedOAuth interface {
	sourceOAuth
	connections.ProviderIdentityRuntime
	connections.ProviderIdentityStatusRuntime
	connections.ProviderIdentityFenceRuntime
}

func integrationFactories(directory string, vault credentials.Store, env func(string) string) map[string]integrations.RuntimeFactory {
	factories := map[string]integrations.RuntimeFactory{}
	for _, d := range connections.Definitions {
		d := d
		available := false
		switch d.ID {
		case "gmail", "google_drive.files", "calendar.google":
			available = env("FLOE_GOOGLE_OAUTH_CLIENT_ID") != ""
		case "microsoft.mail", "calendar.microsoft", "microsoft.teams":
			available = env("FLOE_MICROSOFT_OAUTH_CLIENT_ID") != ""
		case "github.issues":
			available = env("FLOE_GITHUB_OAUTH_CLIENT_ID") != ""
		case "slack.conversations":
			available = env("FLOE_SLACK_OAUTH_CLIENT_ID") != ""
		case "home_assistant.states":
			available = true
		}
		if !available {
			continue
		}
		factories[d.ID] = integrations.FactoryFunc(func(ctx context.Context, config integrations.RuntimeConfig) (integrations.Runtime, error) {
			return openIntegration(ctx, directory, vault, env, config)
		})
	}
	return factories
}
func openIntegration(ctx context.Context, directory string, vault credentials.Store, env func(string) string, c integrations.RuntimeConfig) (out integrations.Runtime, err error) {
	r := c.Record
	scope := r.Scope
	var auth sourceOAuth
	switch r.ConnectorID {
	case "gmail", "google_drive.files", "calendar.google":
		cfg := googleauth.Config{ClientID: env("FLOE_GOOGLE_OAUTH_CLIENT_ID"), ClientSecret: env("FLOE_GOOGLE_OAUTH_CLIENT_SECRET")}
		switch r.ConnectorID {
		case "gmail":
			auth, err = googleauth.New(vault, cfg)
		case "google_drive.files":
			auth, err = googleauth.NewDrive(vault, cfg)
		case "calendar.google":
			auth, err = googleauth.NewCalendar(vault, cfg)
		}
	case "microsoft.mail", "calendar.microsoft", "microsoft.teams":
		cfg := microsoftauth.Config{ClientID: env("FLOE_MICROSOFT_OAUTH_CLIENT_ID"), ClientSecret: env("FLOE_MICROSOFT_OAUTH_CLIENT_SECRET")}
		switch r.ConnectorID {
		case "microsoft.mail":
			auth, err = microsoftauth.New(vault, cfg)
		case "calendar.microsoft":
			auth, err = microsoftauth.NewCalendar(vault, cfg)
		case "microsoft.teams":
			auth, err = microsoftauth.NewTeams(vault, cfg)
		}
	case "github.issues":
		auth, err = workoauth.NewGitHub(vault, workoauth.Config{ClientID: env("FLOE_GITHUB_OAUTH_CLIENT_ID")})
	case "slack.conversations":
		auth, err = workoauth.NewSlack(vault, workoauth.Config{ClientID: env("FLOE_SLACK_OAUTH_CLIENT_ID"), ClientSecret: env("FLOE_SLACK_OAUTH_CLIENT_SECRET")})
	case "home_assistant.states":
		client, e := homeconnector.New(vaultTokenSource{vault, c.Binding.Slot}, scope["base_url"].(string), r.ConnectionID)
		if e != nil {
			return out, e
		}
		entities, ok := connections.ConnectorScopeStrings(scope["entities"])
		if !ok {
			return out, errors.New("invalid scope")
		}
		service, e := homeconnector.NewService(client, entities)
		if e != nil {
			return out, e
		}
		out.Setup = lifecycle.NewSecret(vault, c.Binding)
		out.Snapshot = service
		out.Logistics = service
		return lifecycle.RegisterReaders(out, c, homeconnector.ConnectorDescriptor()), nil
	default:
		return out, errors.New("connector unavailable")
	}
	if err != nil {
		return out, err
	}
	if err = auth.BindCredential(c.Binding.Slot); err != nil {
		auth.Close()
		return out, err
	}
	out.Close = auth.Close
	out.Setup = lifecycle.NewOAuth(auth, c.Binding)
	if r.ConnectorID == "calendar.google" || r.ConnectorID == "calendar.microsoft" {
		identity, ok := auth.(identifiedOAuth)
		if !ok {
			auth.Close()
			return out, errors.New("identity unavailable")
		}
		out.Identity = lifecycle.NewIdentity(identity, c.Binding, r.ConnectorID)
		out.IdentitySupported = true
	}
	defer func() {
		if err != nil && out.Close != nil {
			out.Close()
		}
	}()
	switch r.ConnectorID {
	case "gmail":
		query := env("FLOE_GMAIL_QUERY")
		if query == "" {
			query = "newer_than:30d -in:spam -in:trash"
		}
		var service *gmail.Service
		service, err = gmail.NewService(filepath.Join(directory, "connectors", r.ConnectionID), r.ConnectionID, query, auth)
		if err == nil {
			adapter := gmail.Reader{Service: service}
			out.Snapshot = adapter
			out.Communication = adapter
			out.Logistics = adapter
		}
	case "microsoft.mail":
		var client *microsoftmail.Client
		client, err = microsoftmail.New(auth, r.ConnectionID)
		if err == nil {
			var service *microsoftmail.Service
			service, err = microsoftmail.NewService(client)
			out.Snapshot = service
			out.Communication = service
		}
	case "github.issues":
		var client *githubconnector.Client
		client, err = githubconnector.New(auth)
		if err == nil {
			var service *githubconnector.Service
			service, err = githubconnector.NewService(client, scope["owner"].(string), scope["repository"].(string))
			out.Snapshot = service
			out.Work = service
		}
	case "slack.conversations":
		var client *slackconnector.Client
		client, err = slackconnector.New(auth)
		if err == nil {
			var service *slackconnector.Service
			service, err = slackconnector.NewService(client, scope["channel"].(string), scope["thread"].(string))
			out.Snapshot = service
			out.Work = service
		}
	case "google_drive.files":
		var client *driveconnector.Client
		client, err = driveconnector.New(auth)
		if err == nil {
			var service *driveconnector.Service
			service, err = driveconnector.NewService(client, scope["folder_id"].(string))
			out.Snapshot = service
			out.Work = service
		}
	case "microsoft.teams":
		var client *microsoftteamsconnector.Client
		client, err = microsoftteamsconnector.New(auth)
		if err == nil {
			var service *microsoftteamsconnector.Service
			service, err = microsoftteamsconnector.NewService(client, scope["team_id"].(string), scope["channel_id"].(string))
			out.Snapshot = service
			out.Work = service
		}
	case "calendar.google":
		ids, ok := connections.ConnectorScopeStrings(scope["calendar_ids"])
		if !ok {
			return out, errors.New("invalid scope")
		}
		clients := make([]*calendarconnector.Client, len(ids))
		for i, id := range ids {
			clients[i], err = calendarconnector.New(auth, id, r.ConnectionID)
			if err != nil {
				return out, err
			}
		}
		var service *calendarconnector.Service
		service, err = calendarconnector.NewService(clients...)
		out.Snapshot = service
		out.Calendar = service
	case "calendar.microsoft":
		ids, ok := connections.ConnectorScopeStrings(scope["calendar_ids"])
		if !ok {
			return out, errors.New("invalid scope")
		}
		clients := make([]*microsoftcalendarconnector.Client, len(ids))
		for i, id := range ids {
			clients[i], err = microsoftcalendarconnector.New(auth, id, r.ConnectionID)
			if err != nil {
				return out, err
			}
		}
		var service *microsoftcalendarconnector.Service
		service, err = microsoftcalendarconnector.NewService(clients...)
		out.Snapshot = service
		out.Calendar = service
	}
	if err == nil {
		var descriptor integrations.Descriptor
		switch r.ConnectorID {
		case "gmail":
			descriptor = gmail.ConnectorDescriptor()
		case "microsoft.mail":
			descriptor = microsoftmail.ConnectorDescriptor()
		case "github.issues":
			descriptor = githubconnector.ConnectorDescriptor()
		case "slack.conversations":
			descriptor = slackconnector.ConnectorDescriptor()
		case "google_drive.files":
			descriptor = driveconnector.ConnectorDescriptor()
		case "microsoft.teams":
			descriptor = microsoftteamsconnector.ConnectorDescriptor()
		case "calendar.google":
			descriptor = calendarconnector.ConnectorDescriptor()
		case "calendar.microsoft":
			descriptor = microsoftcalendarconnector.ConnectorDescriptor()
		}
		out = lifecycle.RegisterReaders(out, c, descriptor)
	}
	return out, err
}

type vaultTokenSource struct {
	vault credentials.Store
	name  string
}

func (v vaultTokenSource) Token(context.Context) (string, error) {
	token, err := v.vault.Get(v.name)
	if err != nil || token == "" {
		return "", errors.New("credential unavailable")
	}
	return token, nil
}
