package console

import (
	"context"
	"errors"
	"sort"
	"time"
)

var errConnectorLifecycleInProgress = errors.New("connector lifecycle in progress")

func (console *Console) removeClientAttemptsLocked(state *diskState, clientID string) []string {
	client, exists := state.Clients[clientID]
	if !exists {
		return nil
	}
	transientAttempts := []string{}
	cleanup := state.Cleanups[client.PersonID]
	cleanup.PersonID = client.PersonID
	seenConnections := map[string]bool{}
	for _, step := range cleanup.Connections {
		seenConnections[step.ConnectionID] = true
	}
	for attemptID, attempt := range state.Attempts {
		if attempt.ClientID != clientID {
			continue
		}
		if !seenConnections[attempt.ConnectionID] {
			cleanup.Connections = append(cleanup.Connections, connectionCleanupStep{
				ConnectionID: attempt.ConnectionID, ConnectorID: attempt.ConnectorID, Credential: attempt.Credential,
			})
			seenConnections[attempt.ConnectionID] = true
		}
		delete(state.Attempts, attemptID)
		transientAttempts = append(transientAttempts, attemptID)
	}
	for attemptID, attempt := range console.connectorAttempts {
		if attempt.ClientID == clientID {
			transientAttempts = append(transientAttempts, attemptID)
		}
	}
	if len(cleanup.Connections) == 0 {
		return transientAttempts
	}
	sort.Slice(cleanup.Connections, func(left, right int) bool {
		if cleanup.Connections[left].ConnectorID == cleanup.Connections[right].ConnectorID {
			return cleanup.Connections[left].ConnectionID < cleanup.Connections[right].ConnectionID
		}
		return cleanup.Connections[left].ConnectorID < cleanup.Connections[right].ConnectorID
	})
	state.Cleanups[client.PersonID] = cleanup
	return transientAttempts
}

func (console *Console) removePersonConnectionsLocked(state *diskState, personID string) []string {
	for _, client := range state.Clients {
		if client.PersonID == personID {
			return nil
		}
	}
	transientAttempts := []string{}
	cleanup := state.Cleanups[personID]
	cleanup.PersonID = personID
	seenCredentials := map[string]bool{}
	seenConnections := map[string]bool{}
	for _, step := range cleanup.Connections {
		seenConnections[step.ConnectionID] = true
		if step.Credential != "" {
			seenCredentials[step.Credential] = true
		}
	}
	appendStep := func(connectionID, connectorID, credential string) {
		if seenConnections[connectionID] || credential != "" && seenCredentials[credential] {
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
		seenConnections[connectionID] = true
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
				transientAttempts = append(transientAttempts, attemptID)
			}
		}
	}
	for attemptID, attempt := range console.connectorAttempts {
		if attempt.PersonID != personID {
			continue
		}
		appendStep(attempt.ConnectionID, attempt.ConnectorID, attempt.Credential)
		transientAttempts = append(transientAttempts, attemptID)
	}
	for attemptID, attempt := range state.Attempts {
		if attempt.PersonID != personID {
			continue
		}
		appendStep(attempt.ConnectionID, attempt.ConnectorID, attempt.Credential)
		delete(state.Attempts, attemptID)
	}
	for _, record := range console.connectorReservations {
		if record.PersonID == personID {
			appendStep(record.ConnectionID, record.ConnectorID, record.Credential)
		}
	}
	sort.Slice(cleanup.Connections, func(left, right int) bool {
		if cleanup.Connections[left].ConnectorID == cleanup.Connections[right].ConnectorID {
			return cleanup.Connections[left].ConnectionID < cleanup.Connections[right].ConnectionID
		}
		return cleanup.Connections[left].ConnectorID < cleanup.Connections[right].ConnectorID
	})
	if len(cleanup.Connections) != 0 {
		state.Cleanups[personID] = cleanup
	}
	return transientAttempts
}

func (console *Console) recoverConnectionAttemptsLocked() error {
	if len(console.state.Attempts) == 0 {
		return nil
	}
	next := cloneState(console.state)
	identifiers := make([]string, 0, len(next.Attempts))
	for identifier := range next.Attempts {
		identifiers = append(identifiers, identifier)
	}
	sort.Strings(identifiers)
	for _, identifier := range identifiers {
		attempt := next.Attempts[identifier]
		cleanup := next.Cleanups[attempt.PersonID]
		cleanup.PersonID = attempt.PersonID
		cleanup.Connections = append(cleanup.Connections, connectionCleanupStep{
			ConnectionID: attempt.ConnectionID, ConnectorID: attempt.ConnectorID, Credential: attempt.Credential,
		})
		next.Cleanups[attempt.PersonID] = cleanup
		delete(next.Attempts, identifier)
	}
	for personID, cleanup := range next.Cleanups {
		sort.Slice(cleanup.Connections, func(left, right int) bool {
			if cleanup.Connections[left].ConnectorID == cleanup.Connections[right].ConnectorID {
				return cleanup.Connections[left].ConnectionID < cleanup.Connections[right].ConnectionID
			}
			return cleanup.Connections[left].ConnectorID < cleanup.Connections[right].ConnectorID
		})
		next.Cleanups[personID] = cleanup
	}
	if err := console.save(next); err != nil {
		return err
	}
	console.state = next
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
		if _, reserved := console.connectorReservations[step.ConnectionID]; reserved {
			cleanupErrors = append(cleanupErrors, errConnectorLifecycleInProgress)
			continue
		}
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
					step.RuntimeComplete = true
					if err := console.persistPersonCleanupProgressLocked(cleanup); err != nil {
						return err
					}
				}
			}
		}
		if step.RuntimeComplete && !step.VaultComplete {
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

func (console *Console) completeReservedCleanupLocked(record connectionRecord, runtimeComplete, vaultComplete bool) error {
	cleanup, exists := console.state.Cleanups[record.PersonID]
	if !exists {
		return nil
	}
	found := false
	for index := range cleanup.Connections {
		if cleanup.Connections[index].ConnectionID != record.ConnectionID {
			continue
		}
		cleanup.Connections[index].RuntimeComplete = cleanup.Connections[index].RuntimeComplete || runtimeComplete
		cleanup.Connections[index].VaultComplete = cleanup.Connections[index].VaultComplete || vaultComplete
		found = true
		break
	}
	if !found {
		return nil
	}
	complete := true
	for _, step := range cleanup.Connections {
		if !step.RuntimeComplete || !step.VaultComplete {
			complete = false
			break
		}
	}
	next := cloneState(console.state)
	if complete {
		delete(next.Cleanups, record.PersonID)
	} else {
		next.Cleanups[record.PersonID] = cleanup
	}
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
