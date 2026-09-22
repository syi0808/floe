package httptransport

import (
	"net/http"
	"strings"

	"floe/server/internal/connections"
	"floe/server/internal/operation"
)

func ServeConnectors(writer http.ResponseWriter, request *http.Request, operations ConnectorOperations) {
	if request.URL.Path == "/v1/connectors" || request.URL.Path == "/v1/connectors/" {
		if request.Method != http.MethodGet {
			failure(writer, http.StatusNotFound, "not_found")
			return
		}
		writeResult(writer, operations.Catalog())
		return
	}
	parts := strings.Split(strings.TrimPrefix(request.URL.Path, "/v1/connectors/"), "/")
	if _, exists := connections.DefinitionFor(parts[0]); !exists {
		failure(writer, http.StatusNotFound, "connector_not_found")
		return
	}
	switch {
	case len(parts) == 2 && parts[1] == "connect" && request.Method == http.MethodPost:
		dispatch(writer, request, func(input connections.ConnectRequest) operation.Result {
			return operations.Start(request.Context(), parts[0], input)
		})
	case len(parts) == 2 && parts[1] == "scope" && request.Method == http.MethodPatch:
		dispatch(writer, request, func(input connections.ScopeRequest) operation.Result { return operations.Update(parts[0], input) })
	case len(parts) == 1 && request.Method == http.MethodDelete:
		dispatch(writer, request, func(input connections.DisconnectRequest) operation.Result {
			return operations.Disconnect(request.Context(), parts[0], input)
		})
	case len(parts) == 3 && parts[1] == "connection-attempts" && request.Method == http.MethodGet:
		writeResult(writer, operations.Attempt(request.Context(), parts[0], parts[2]))
	case len(parts) == 4 && parts[1] == "connection-attempts" && parts[3] == "cancel" && request.Method == http.MethodPost:
		writeResult(writer, operations.Cancel(request.Context(), parts[0], parts[2]))
	default:
		failure(writer, http.StatusNotFound, "not_found")
	}
}
