package httptransport

import (
	"net/http"
	"strings"

	"floe/server/internal/integrations"
	"floe/server/internal/operation"
	"floe/server/internal/trust"
)

func ServeConnectors(writer http.ResponseWriter, request *http.Request, principal trust.Principal, service *integrations.Service) {
	if request.URL.Path == "/v1/connectors" || request.URL.Path == "/v1/connectors/" {
		if request.Method != http.MethodGet {
			failure(writer, http.StatusNotFound, "not_found")
			return
		}
		writeResult(writer, service.Catalog(request.Context(), principal))
		return
	}
	parts := strings.Split(strings.TrimPrefix(request.URL.Path, "/v1/connectors/"), "/")
	if _, exists := integrations.DefinitionFor(parts[0]); !exists {
		failure(writer, http.StatusNotFound, "connector_not_found")
		return
	}
	switch {
	case len(parts) == 2 && parts[1] == "connect" && request.Method == http.MethodPost:
		dispatch(writer, request, func(input integrations.ConnectRequest) operation.Result {
			return service.Start(request.Context(), principal, parts[0], input)
		})
	case len(parts) == 2 && parts[1] == "scope" && request.Method == http.MethodPatch:
		dispatch(writer, request, func(input integrations.ScopeRequest) operation.Result {
			return service.UpdateScope(request.Context(), principal, parts[0], input)
		})
	case len(parts) == 2 && parts[1] == "disconnect" && request.Method == http.MethodPost:
		dispatch(writer, request, func(input integrations.DisconnectRequest) operation.Result {
			return service.Disconnect(request.Context(), principal, parts[0], input)
		})
	case len(parts) == 3 && parts[1] == "connection-attempts" && request.Method == http.MethodGet:
		writeResult(writer, service.Poll(request.Context(), principal, parts[0], parts[2]))
	case len(parts) == 4 && parts[1] == "connection-attempts" && parts[3] == "cancel" && request.Method == http.MethodPost:
		dispatch(writer, request, func(in integrations.CancelSetupRequest) operation.Result {
			return service.Cancel(request.Context(), principal, parts[0], parts[2], in)
		})
	default:
		failure(writer, http.StatusNotFound, "not_found")
	}
}
