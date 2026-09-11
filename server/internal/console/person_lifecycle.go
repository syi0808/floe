package console

type personConnectionCleanup struct {
	credentials []string
	runtimes    []ConnectorOAuthRuntime
}

func (console *Console) removePersonConnectionsLocked(state *diskState, personID string) (personConnectionCleanup, error) {
	for _, client := range state.Clients {
		if client.PersonID == personID {
			return personConnectionCleanup{}, nil
		}
	}
	cleanup := personConnectionCleanup{}
	for connectionID, record := range state.Connections {
		if record.PersonID != personID {
			continue
		}
		if definition, exists := clientConnectorDefinitionFor(record.ConnectorID); exists {
			if record.Credential != "" {
				cleanup.credentials = append(cleanup.credentials, record.Credential)
			}
			if definition.AuthKind != "secret" && definition.OAuthCredential != "" {
				if definition.OAuthRuntime != nil {
					if runtime := definition.OAuthRuntime(console); runtime != nil {
						cleanup.runtimes = append(cleanup.runtimes, runtime)
					}
				}
			}
		}
		delete(state.Connections, connectionID)
		for attemptID, attempt := range console.connectorAttempts {
			if attempt.ConnectionID == connectionID {
				delete(console.connectorAttempts, attemptID)
			}
		}
	}
	return cleanup, nil
}
