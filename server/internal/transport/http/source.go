package httptransport

import (
	"net/http"
	"strings"

	"floe/server/internal/authorization"
)

func serveSource(writer http.ResponseWriter, request *http.Request, principal authorization.Principal, service *authorization.SourceService) {
	parts := strings.Split(strings.TrimPrefix(request.URL.Path, "/v1/views/"), "/")
	viewID := parts[0]
	if viewID != "calendar.timeline" && viewID != "mail.communication" && viewID != "work.context" && viewID != "life.logistics" {
		failure(writer, http.StatusNotFound, "not_found")
		return
	}
	if len(parts) == 1 && viewID != "calendar.timeline" {
		failure(writer, http.StatusBadRequest, "admission_required")
		return
	}
	if viewID == "calendar.timeline" && len(service.Calendars) == 0 && (len(parts) != 2 || parts[1] != "source-preview") {
		failure(writer, http.StatusNotFound, "calendar_connector_not_found")
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
		var input authorization.SourcePreview
		if !decodeCalendarEnvelope(writer, request, map[string]struct{}{"connector_id": {}, "connection_id": {}, "resource": {}}, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		writeResult(writer, service.PreviewView(principal, viewID, input))
	case "admit":
		allowed := map[string]struct{}{"schema_version": {}, "connector_id": {}, "connection_id": {}, "connection_revision": {}, "resources": {}, "policy": {}, "grant": {}, "purpose": {}, "consumer": {}, "max_items": {}, "max_bytes": {}, "query": {}}
		if viewID == "calendar.timeline" {
			var input authorization.CalendarAdmission
			if !decodeCalendarEnvelope(writer, request, allowed, &input) {
				failure(writer, http.StatusBadRequest, "validation")
				return
			}
			writeResult(writer, service.AdmitCalendar(request.Context(), principal, input))
		} else {
			var input authorization.ViewAdmission
			if !decodeCalendarEnvelope(writer, request, allowed, &input) {
				failure(writer, http.StatusBadRequest, "validation")
				return
			}
			writeResult(writer, service.AdmitView(principal, viewID, input))
		}
	case "read", "release":
		proof, ok := decodeCalendarProof(writer, request)
		if !ok {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		if parts[1] == "release" {
			writeResult(writer, service.Release(principal, proof))
		} else if viewID == "calendar.timeline" {
			writeResult(writer, service.ReadCalendar(request.Context(), principal, proof))
		} else {
			writeResult(writer, service.ReadView(request.Context(), principal, viewID, proof))
		}
	default:
		failure(writer, http.StatusNotFound, "not_found")
	}
}
