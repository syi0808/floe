package httptransport

import (
	"net/http"
	"strings"

	"floe/server/internal/authority"
	"floe/server/internal/trust"
)

func serveSource(writer http.ResponseWriter, request *http.Request, principal trust.Principal, service *authority.SourceService) {
	parts := strings.Split(strings.TrimPrefix(request.URL.Path, "/v1/views/"), "/")
	viewID := parts[0]
	if viewID != "calendar.timeline" && viewID != "mail.communication" && viewID != "work.context" && viewID != "life.logistics" {
		failure(writer, http.StatusNotFound, "not_found")
		return
	}
	if request.Method != http.MethodPost {
		failure(writer, http.StatusMethodNotAllowed, "method_not_allowed")
		return
	}
	if len(parts) == 1 {
		failure(writer, http.StatusBadRequest, "admission_required")
		return
	}
	if len(parts) != 2 {
		failure(writer, http.StatusNotFound, "not_found")
		return
	}
	switch parts[1] {
	case "source-preview":
		var input authority.SourcePreview
		if !decodeSourceEnvelope(writer, request, map[string]struct{}{"connector_id": {}, "connection_id": {}, "resource": {}}, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		writeResult(writer, service.PreviewView(request.Context(), principal, viewID, input))
	case "admit":
		allowed := map[string]struct{}{"schema_version": {}, "connector_id": {}, "connection_id": {}, "connection_revision": {}, "resources": {}, "grant": {}, "purpose": {}, "consumer": {}, "max_items": {}, "max_bytes": {}, "query": {}}
		var input authority.ViewAdmission
		if !decodeSourceEnvelope(writer, request, allowed, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		writeResult(writer, service.AdmitView(request.Context(), principal, viewID, input))
	case "read", "release":
		proof, ok := decodeSourceProof(writer, request)
		if !ok {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		if parts[1] == "release" {
			writeResult(writer, service.Release(request.Context(), principal, viewID, proof))
		} else {
			writeResult(writer, service.ReadView(request.Context(), principal, viewID, proof))
		}
	default:
		failure(writer, http.StatusNotFound, "not_found")
	}
}
