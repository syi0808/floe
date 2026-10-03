package httptransport

import (
	"floe/server/internal/authority"
	"floe/server/internal/trust"
	"net/http"
	"strings"
)

func serveCalendarMirror(w http.ResponseWriter, r *http.Request, p trust.Principal, s *authority.CalendarMirrorService) {
	if r.Method != http.MethodPost {
		failure(w, 405, "method_not_allowed")
		return
	}
	switch strings.TrimPrefix(r.URL.Path, "/v1/calendar/mirror/") {
	case "source-preview":
		var in authority.ProductCalendarPreviewRequest
		if !decodeSourceEnvelope(w, r, map[string]struct{}{"schema_version": {}, "connector_id": {}, "connection_id": {}, "local_revision": {}}, &in) {
			failure(w, 400, "validation")
			return
		}
		writeResult(w, s.Preview(r.Context(), p, in))
	case "admit":
		var in authority.ProductCalendarAdmissionRequest
		if !decodeSourceEnvelope(w, r, map[string]struct{}{"schema_version": {}, "claims": {}, "expires_at_unix_ms": {}}, &in) {
			failure(w, 400, "validation")
			return
		}
		writeResult(w, s.Admit(r.Context(), p, in))
	case "read", "release":
		proof, ok := decodeSourceProof(w, r)
		if !ok {
			failure(w, 400, "validation")
			return
		}
		if strings.HasSuffix(r.URL.Path, "/read") {
			writeResult(w, s.Read(r.Context(), p, proof))
		} else {
			writeResult(w, s.Release(r.Context(), p, proof))
		}
	default:
		failure(w, 404, "not_found")
	}
}
