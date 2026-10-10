package httptransport

import (
	"net/http"
	"strings"

	"floe/server/internal/integrations"
	"floe/server/internal/trust"
)

func ServeConnectors(writer http.ResponseWriter, request *http.Request, principal trust.Principal, service *integrations.Service) {
	if request.URL.Path == "/v1/connectors" || request.URL.Path == "/v1/connectors/" {
		if request.Method != http.MethodGet {
			failure(writer, http.StatusNotFound, "not_found")
			return
		}
		result, err := service.Catalog(request.Context(), principal)
		if err != nil {
			writeOperationError(writer, err)
			return
		}
		reply(writer, http.StatusOK, catalogProjection(result))
		return
	}
	parts := strings.Split(strings.TrimPrefix(request.URL.Path, "/v1/connectors/"), "/")
	if _, exists := integrations.DefinitionFor(parts[0]); !exists {
		failure(writer, http.StatusNotFound, "connector_not_found")
		return
	}
	switch {
	case len(parts) == 2 && parts[1] == "connect" && request.Method == http.MethodPost:
		dispatch(writer, request, func(input integrations.ConnectRequest) (attemptDTO, error) {
			result, err := service.Start(request.Context(), principal, parts[0], input)
			if err != nil {
				return attemptDTO{}, err
			}
			return attemptProjection(result), nil
		})
	case len(parts) == 2 && parts[1] == "scope" && request.Method == http.MethodPatch:
		dispatch(writer, request, func(input integrations.ScopeRequest) (scopeResultDTO, error) {
			result, err := service.UpdateScope(request.Context(), principal, parts[0], input)
			if err != nil {
				return scopeResultDTO{}, err
			}
			return scopeResultDTO{SchemaVersion: result.SchemaVersion, PersonID: result.PersonID, DeviceID: result.DeviceID, ConnectionID: result.ConnectionID, ConnectionRevision: result.ConnectionRevision, ConnectorID: result.ConnectorID, Scope: result.Scope}, nil
		})
	case len(parts) == 2 && parts[1] == "disconnect" && request.Method == http.MethodPost:
		dispatch(writer, request, func(input integrations.DisconnectRequest) (disconnectDTO, error) {
			result, err := service.Disconnect(request.Context(), principal, parts[0], input)
			if err != nil {
				return disconnectDTO{}, err
			}
			return disconnectDTO{SchemaVersion: result.SchemaVersion, OperationID: result.OperationID, PersonID: result.PersonID, DeviceID: result.DeviceID, ConnectionID: result.ConnectionID, ConnectorID: result.ConnectorID, ConnectionRevision: result.ConnectionRevision, CleanupState: result.CleanupState}, nil
		})
	case len(parts) == 3 && parts[1] == "connection-attempts" && request.Method == http.MethodGet:
		result, err := service.Poll(request.Context(), principal, parts[0], parts[2])
		if err != nil {
			writeOperationError(writer, err)
			return
		}
		reply(writer, http.StatusOK, attemptProjection(result))
	case len(parts) == 4 && parts[1] == "connection-attempts" && parts[3] == "cancel" && request.Method == http.MethodPost:
		dispatch(writer, request, func(in integrations.CancelSetupRequest) (attemptDTO, error) {
			result, err := service.Cancel(request.Context(), principal, parts[0], parts[2], in)
			if err != nil {
				return attemptDTO{}, err
			}
			return attemptProjection(result), nil
		})
	default:
		failure(writer, http.StatusNotFound, "not_found")
	}
}
