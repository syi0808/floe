package console

import (
	"context"
	"errors"
	"time"
)

func (console *Console) removePersonConnectionsLocked(state *diskState, personID string) error {
	for _, client := range state.Clients {
		if client.PersonID == personID {
			return nil
		}
	}
	cleanup := personCleanup{PersonID: personID}
	seenCredentials := map[string]bool{}
	appendStep := func(connectionID, connectorID, credential string) {
		if credential != "" && seenCredentials[credential] {
			return
		}
		definition, exists := clientConnectorDefinitionFor(connectorID)
		if !exists {
			return
		}
		cleanup.Connections = append(cleanup.Connections, connectionCleanupStep{
			ConnectionID: connectionID, ConnectorID: connectorID, Credential: credential,
			RuntimeComplete: definition.AuthKind == "secret", VaultComplete: credential == "",
		})
		if credential != "" {
			seenCredentials[credential] = true
		}
	}
	for connectionID, record := range state.Connections {
		if record.PersonID != personID {
			continue
		}
		appendStep(record.ConnectionID, record.ConnectorID, record.Credential)
		delete(state.Connections, connectionID)
		for attemptID, attempt := range console.connectorAttempts {
			if attempt.ConnectionID == connectionID {
				delete(console.connectorAttempts, attemptID)
			}
		}
	}
	for attemptID, attempt := range console.connectorAttempts {
		if attempt.PersonID != personID {
			continue
		}
		appendStep(attempt.ConnectionID, attempt.ConnectorID, attempt.Credential)
		delete(console.connectorAttempts, attemptID)
	}
	if len(cleanup.Connections) != 0 {
		state.Cleanups[personID] = cleanup
	}
	return nil
}

func (console *Console) retryPersonCleanupLocked(personID string) error {
	cleanup, exists := console.state.Cleanups[personID]
	if !exists {
		return nil
	}
	var cleanupErrors []error
	for index := range cleanup.Connections {
		step := cleanup.Connections[index]
		if !step.RuntimeComplete {
			definition, exists := clientConnectorDefinitionFor(step.ConnectorID)
			if !exists || definition.OAuthRuntime == nil || definition.OAuthRuntime(console) == nil {
				cleanupErrors = append(cleanupErrors, errors.New("cleanup runtime unavailable"))
			} else {
				runtime := definition.OAuthRuntime(console)
				err := runtime.BindCredential(step.Credential)
				if err == nil {
					ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
					_, err = runtime.Action(ctx, "logout")
					cancel()
				}
				if err != nil {
					cleanupErrors = append(cleanupErrors, err)
				} else {
					cleanup.Connections[index].RuntimeComplete = true
					if err := console.persistPersonCleanupProgressLocked(cleanup); err != nil {
						return err
					}
				}
			}
		}
		if !step.VaultComplete {
			if err := console.vault.Delete(step.Credential); err != nil {
				cleanupErrors = append(cleanupErrors, err)
			} else {
				cleanup.Connections[index].VaultComplete = true
				if err := console.persistPersonCleanupProgressLocked(cleanup); err != nil {
					return err
				}
			}
		}
	}
	if len(cleanupErrors) != 0 {
		return errors.Join(cleanupErrors...)
	}
	next := cloneState(console.state)
	delete(next.Cleanups, personID)
	if err := console.save(next); err != nil {
		return err
	}
	console.state = next
	return nil
}

func (console *Console) persistPersonCleanupProgressLocked(cleanup personCleanup) error {
	next := cloneState(console.state)
	next.Cleanups[cleanup.PersonID] = cleanup
	if err := console.save(next); err != nil {
		return err
	}
	console.state = next
	return nil
}
