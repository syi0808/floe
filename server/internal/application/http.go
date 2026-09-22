package application

import (
	"context"
	"errors"
	"net/http"

	"floe/server/internal/authorization"
	"floe/server/internal/connections"
	"floe/server/internal/operation"
	"floe/server/internal/pairing"
	httptransport "floe/server/internal/transport/http"
)

func (console *Console) ServeHTTP(writer http.ResponseWriter, request *http.Request) {
	console.handler.ServeHTTP(writer, request)
}

func (console *Console) authorityTransport() httptransport.AuthorityHandler {
	handler := httptransport.AuthorityHandler{ProducerMetadata: console.producerMetadata}
	if engine := console.authorityEngine(); engine != nil {
		handler.Service = engine
	}
	if producer := console.admissions.Producer(); producer != nil {
		handler.Sign = producer.SignChallenge
	}
	return handler
}

func (console *Console) newHandler(adminHash string) *httptransport.Handler {
	return &httptransport.Handler{
		Address: console.address, Sessions: httptransport.NewSessions(adminHash),
		Pairing:      func() *pairing.Operations { return console.pairing },
		Authenticate: console.authenticate,
		Management: httptransport.Management{
			State: console.managementState, Codex: console.codexAction,
			Route: console.updateRoute, Target: console.updateTarget, Provider: console.updateProvider,
			Test: console.testTarget, DeleteClient: console.deleteClient, DeleteTarget: console.deleteTarget,
			Authority: console.authorityTransport,
		},
	}
}

func (console *Console) authenticate(token string) (httptransport.Client, operation.Result) {
	console.mu.Lock()
	var scope connections.Scope
	if token != "" {
		hash := digest(token)
		for identifier, value := range console.state.Clients {
			if hash == value.TokenHash {
				scope = connections.Scope{ClientID: identifier, PersonID: value.PersonID, DeviceID: value.DeviceID}
			}
		}
	}
	gateway := console.gateway
	gmail := console.gmail
	microsoftMail := console.microsoftMail
	work := make(map[string]connections.WorkContextRuntime, len(console.work))
	for connectorID, runtime := range console.work {
		work[connectorID] = runtime
	}
	logistics := make(map[string]connections.LogisticsRuntime, len(console.logistics))
	for connectorID, runtime := range console.logistics {
		logistics[connectorID] = runtime
	}
	calendars := make(map[string]connections.CalendarRuntime, len(console.calendars))
	for connectorID, runtime := range console.calendars {
		calendars[connectorID] = runtime
	}
	connectionRecords := cloneState(console.state).Connections
	console.mu.Unlock()
	if !validPersonID(scope.PersonID) || !validDeviceID(scope.DeviceID) {
		return httptransport.Client{}, operation.Reject(operation.Unauthenticated, "unauthorized")
	}
	console.mu.Lock()
	_, cleanupPending := console.state.Cleanups[scope.PersonID]
	console.mu.Unlock()
	if cleanupPending {
		return httptransport.Client{}, operation.Reject(operation.Unavailable, "person_cleanup_pending")
	}

	sources := connections.Sources{ByConnector: map[string]connections.SnapshotSource{}, ByConnection: map[string]connections.SnapshotSource{}}
	if gmail != nil {
		sources.ByConnector["gmail"] = connections.LegacySnapshotSource{Runtime: gmail}
	}
	if microsoftMail != nil {
		sources.ByConnector["microsoft.mail"] = microsoftMail
	}
	for connectionID, runtime := range work {
		sources.ByConnection[connectionID] = runtime
	}
	for connectionID, runtime := range logistics {
		sources.ByConnection[connectionID] = runtime
	}
	for connectionID, runtime := range calendars {
		sources.ByConnection[connectionID] = runtime
	}
	authority := &authorization.SourceService{
		Admissions: console.admissions, Authority: console, Engine: console.authorityEngine,
		Metadata: console.producerMetadata, ExecutionOwner: console.executionOwner,
		PreflightCalendarIdentity: console.preflightCalendarIdentity, Records: connectionRecords, Calendars: calendars,
	}
	if gmail != nil && connections.OwnedByConnector(connectionRecords, "gmail", scope) {
		authority.Communication = append(authority.Communication, legacyCommunicationAdapter{runtime: gmail})
	}
	if microsoftMail != nil && connections.OwnedByConnector(connectionRecords, "microsoft.mail", scope) {
		authority.Communication = append(authority.Communication, microsoftMail)
	}
	for connectionID, runtime := range work {
		if connections.OwnedByConnection(connectionRecords, connectionID, scope) {
			authority.Work = append(authority.Work, runtime)
		}
	}
	for connectionID, runtime := range logistics {
		if connections.OwnedByConnection(connectionRecords, connectionID, scope) {
			authority.Logistics = append(authority.Logistics, runtime)
		}
	}
	client := httptransport.Client{
		Scope: scope, Authority: console.authorityTransport(), Sources: authority,
		List: func(ctx context.Context) operation.Result {
			owned, err := connections.List(ctx, scope, connectionRecords, sources, console)
			if err != nil {
				if errors.Is(err, connections.ErrScopeUnavailable) {
					return operation.Reject(operation.Unavailable, "connection_scope_unavailable")
				}
				return operation.Reject(operation.Unavailable, "connections_unavailable")
			}
			return operation.Accept(map[string]any{"schema_version": 1, "person_id": scope.PersonID, "device_id": scope.DeviceID, "connections": owned})
		},
		Inference:  httptransport.AuthenticatedInference(gateway, console.internalToken),
		Connectors: console.connectorOperations(scope),
	}
	return client, operation.Accept(nil)
}

func (console *Console) connectorOperations(scope connections.Scope) httptransport.ConnectorOperations {
	return httptransport.ConnectorOperations{
		Catalog: func() operation.Result { return console.connectorCatalog(scope) },
		Start: func(ctx context.Context, connectorID string, input connections.ConnectRequest) operation.Result {
			definition, exists := connections.DefinitionFor(connectorID)
			if !exists {
				return operation.Reject(operation.Missing, "connector_not_found")
			}
			return console.startConnector(ctx, scope, definition, input)
		},
		Attempt: func(ctx context.Context, connectorID, attemptID string) operation.Result {
			definition, exists := connections.DefinitionFor(connectorID)
			if !exists {
				return operation.Reject(operation.Missing, "connector_not_found")
			}
			return console.connectorAttempt(ctx, scope, definition, attemptID)
		},
		Cancel: func(ctx context.Context, connectorID, attemptID string) operation.Result {
			definition, exists := connections.DefinitionFor(connectorID)
			if !exists {
				return operation.Reject(operation.Missing, "connector_not_found")
			}
			return console.cancelConnectorAttempt(ctx, scope, definition, attemptID)
		},
		Update: func(connectorID string, input connections.ScopeRequest) operation.Result {
			definition, exists := connections.DefinitionFor(connectorID)
			if !exists {
				return operation.Reject(operation.Missing, "connector_not_found")
			}
			return console.updateConnectorScope(scope, definition, input)
		},
		Disconnect: func(ctx context.Context, connectorID string, input connections.DisconnectRequest) operation.Result {
			definition, exists := connections.DefinitionFor(connectorID)
			if !exists {
				return operation.Reject(operation.Missing, "connector_not_found")
			}
			return console.disconnectConnector(ctx, scope, definition, input)
		},
	}
}
