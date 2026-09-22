package application

import (
	"context"
	"errors"
	"time"

	"floe/server/internal/connections"
)

func (console *Console) lockConnectorLifecycle(connectorID string) func() {
	lifecycle := console.connections.Lifecycle(connectorID)
	lifecycle.Lock()
	return lifecycle.Unlock
}

func bindClientOAuthCredential(runtime connections.ConnectorOAuthRuntime, record connections.Record) error {
	return runtime.BindCredential(record.Credential)
}

func (console *Console) newConnectorAttempt(connectorID, connectionID string, scope connections.Scope, status, authorizationURL string) *connections.Attempt {
	now := time.Now()
	for identifier, attempt := range console.connections.Attempts() {
		if attempt.Status != "pending" && now.Sub(attempt.CreatedAt) > 10*time.Minute {
			console.connections.DeleteAttempt(identifier)
		}
	}
	if len(console.connections.Attempts()) >= 64 {
		var oldestID string
		var oldest time.Time
		for identifier, attempt := range console.connections.Attempts() {
			if attempt.Status != "pending" && (oldestID == "" || attempt.CreatedAt.Before(oldest)) {
				oldestID, oldest = identifier, attempt.CreatedAt
			}
		}
		if oldestID != "" {
			console.connections.DeleteAttempt(oldestID)
		}
	}
	attempt := &connections.Attempt{ID: randomToken(), ClientID: scope.ClientID, ConnectorID: connectorID, ConnectionID: connectionID, PersonID: scope.PersonID, DeviceID: scope.DeviceID, Status: status, AuthorizationURL: authorizationURL, CreatedAt: now}
	console.connections.PutAttempt(attempt.ID, attempt)
	return attempt
}

func (console *Console) connectionCredentialReady(record connections.Record) bool {
	if record.Credential == "" {
		return true
	}
	value, err := console.vault.Get(record.Credential)
	if err != nil || value == "" {
		return false
	}
	definition, exists := connections.DefinitionFor(record.ConnectorID)
	if !exists || !connections.IsOAuthAuthKind(definition.AuthKind) {
		return true
	}
	runtime := console.connectorOAuthRuntime(definition.ID)
	return runtime != nil && runtime.Ready()
}

func (console *Console) finishClientOAuthAttempt(attemptID string) bool {
	console.mu.Lock()
	attempt := console.connections.GetAttempt(attemptID)
	durable, durableExists := console.state.Attempts[attemptID]
	if attempt == nil || !durableExists || attempt.Status != "pending" && attempt.Status != "connected" || durable.ConnectionID != attempt.ConnectionID {
		console.mu.Unlock()
		return false
	}
	definition, definitionExists := connections.DefinitionFor(attempt.ConnectorID)
	providerRuntime := connections.ConnectorOAuthRuntime(nil)
	if definitionExists && connections.IsOAuthAuthKind(definition.AuthKind) {
		providerRuntime = console.connectorOAuthRuntime(definition.ID)
	}
	attemptConnectionID := attempt.ConnectionID
	attemptConnectorID := attempt.ConnectorID
	attemptPersonID := attempt.PersonID
	attemptScope := connections.CloneConnectorScope(attempt.Scope)
	attemptCredential := attempt.Credential
	incarnation, epoch := durable.Incarnation, durable.Epoch
	console.mu.Unlock()

	providerIdentity := ""
	identityUnverified := true
	if providerRuntime != nil {
		if identityProvider, ok := providerRuntime.(connections.ProviderIdentityRuntime); ok {
			identityContext, cancel := context.WithTimeout(context.Background(), 10*time.Second)
			providerIdentity, _ = identityProvider.ProviderIdentity(identityContext)
			cancel()
			identityUnverified = providerIdentity == ""
		}
	}

	console.mu.Lock()
	attempt = console.connections.GetAttempt(attemptID)
	durable, durableExists = console.state.Attempts[attemptID]
	if attempt == nil || !durableExists || attempt.ConnectionID != attemptConnectionID || attempt.ConnectorID != attemptConnectorID || attempt.PersonID != attemptPersonID || durable.Incarnation != incarnation || durable.Epoch != epoch {
		console.mu.Unlock()
		return false
	}
	record := connections.Record{ConnectionID: attemptConnectionID, Revision: 1, ConnectorID: attemptConnectorID, PersonID: attemptPersonID, Scope: attemptScope, Credential: attemptCredential, Incarnation: incarnation, Epoch: epoch, ProviderIdentity: providerIdentity, IdentityUnverified: identityUnverified}
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
	console.connections.PutAttempt(attempt.ID, attempt)
	console.mu.Unlock()
	return true
}

func (console *Console) failClientOAuthAttempt(attemptID string, runtime connections.ConnectorOAuthRuntime) {
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	_ = console.cleanupClientOAuthAttempt(ctx, attemptID, runtime)
}

func (console *Console) cleanupClientOAuthAttempt(ctx context.Context, attemptID string, runtime connections.ConnectorOAuthRuntime) error {
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
	console.connections.DeleteAttempt(attemptID)
	record := connections.Record{ConnectionID: attempt.ConnectionID, ConnectorID: attempt.ConnectorID, PersonID: attempt.PersonID, Credential: attempt.Credential}
	console.connections.Reserve(attempt.ConnectionID, record)
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
	console.connections.ReleaseReservation(attempt.ConnectionID)
	err := console.completeReservedCleanupLocked(record, runtimeComplete, vaultComplete)
	console.mu.Unlock()
	if !runtimeComplete || !vaultComplete || err != nil {
		return errors.New("connection cleanup pending")
	}
	return nil
}

func (console *Console) connectionForPerson(connectorID, personID string) (connections.Record, bool) {
	for _, record := range console.state.Connections {
		if record.ConnectorID == connectorID && record.PersonID == personID {
			return record, true
		}
	}
	return connections.Record{}, false
}

func (console *Console) connectionForConnector(connectorID string) (connections.Record, bool) {
	for _, record := range console.state.Connections {
		if record.ConnectorID == connectorID {
			return record, true
		}
	}
	return connections.Record{}, false
}

func (console *Console) connectionExistsForOtherPerson(connectorID, personID string) bool {
	for _, record := range console.state.Connections {
		if record.ConnectorID == connectorID && record.PersonID != personID {
			return true
		}
	}
	return false
}

func (console *Console) calendarConnectionExistsForPerson(personID, exceptConnectorID string) bool {
	for _, record := range console.state.Connections {
		if record.PersonID == personID && record.ConnectorID != exceptConnectorID && connections.IsCalendarConnector(record.ConnectorID) {
			return true
		}
	}
	return false
}
