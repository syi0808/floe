package application

import (
	"floe/server/internal/connections"
)

func (console *Console) connectorOAuthRuntime(connectorID string) connections.ConnectorOAuthRuntime {
	switch connectorID {
	case "gmail":
		return console.gmail
	case "microsoft.mail":
		return console.microsoftAuth
	case "github.issues":
		return console.githubAuth
	case "slack.conversations":
		return console.slackAuth
	case "google_drive.files":
		return console.driveAuth
	case "calendar.google":
		return console.calendarAuth
	case "calendar.microsoft":
		return console.microsoftCalendarAuth
	case "microsoft.teams":
		return console.microsoftTeamsAuth
	default:
		return nil
	}
}

func (console *Console) connectorAvailable(definition connections.Definition) bool {
	return definition.AuthKind == "secret" || console.connectorOAuthRuntime(definition.ID) != nil
}
