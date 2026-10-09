package httptransport

import (
	"encoding/json"
	"floe/server/internal/trust"
	"floe/server/internal/views"
	viewcontracts "floe/server/internal/views/contracts"
	"net/http"
	"strings"
)

func serveCalendarMirror(w http.ResponseWriter, r *http.Request, p trust.Principal, s *views.CalendarMirrorService) {
	if r.Method != http.MethodPost {
		failure(w, 405, "method_not_allowed")
		return
	}
	switch strings.TrimPrefix(r.URL.Path, "/v1/calendar/mirror/") {
	case "source-preview":
		var in viewcontracts.ProductCalendarPreviewRequest
		if !decodeSourceEnvelope(w, r, map[string]struct{}{"schema_version": {}, "connector_id": {}, "connection_id": {}, "local_revision": {}}, &in) {
			failure(w, 400, "validation")
			return
		}
		result, err := s.Preview(r.Context(), p, in)
		if err != nil {
			writeViewError(w, err)
			return
		}
		writeCalendarMirrorPreview(w, result)
	case "admit":
		var in viewcontracts.ProductCalendarAdmissionRequest
		if !decodeSourceEnvelope(w, r, map[string]struct{}{"schema_version": {}, "claims": {}, "expires_at_unix_ms": {}}, &in) {
			failure(w, 400, "validation")
			return
		}
		result, err := s.Admit(r.Context(), p, in)
		if err != nil {
			writeViewError(w, err)
			return
		}
		writeCalendarMirrorChallenge(w, result)
	case "read", "release":
		proof, ok := decodeSourceProof(w, r)
		if !ok {
			failure(w, 400, "validation")
			return
		}
		if strings.HasSuffix(r.URL.Path, "/read") {
			result, err := s.Read(r.Context(), p, proof)
			if err != nil {
				writeViewError(w, err)
				return
			}
			writeCalendarMirrorChallenge(w, result)
		} else {
			result, err := s.Release(r.Context(), p, proof)
			if err != nil {
				writeViewError(w, err)
				return
			}
			writeCalendarMirrorRelease(w, result)
		}
	default:
		failure(w, 404, "not_found")
	}
}

func writeCalendarMirrorPreview(w http.ResponseWriter, result views.CalendarMirrorPreviewResult) {
	reply(w, http.StatusOK, struct {
		SchemaVersion int                    `json:"schema_version"`
		Descriptor    string                 `json:"descriptor_b64url"`
		Signature     string                 `json:"producer_signature"`
		Producer      trust.ProducerMetadata `json:"producer"`
	}{result.SchemaVersion, result.Descriptor, result.Signature, result.Producer})
}

func writeCalendarMirrorChallenge(w http.ResponseWriter, result views.CalendarMirrorChallengeResult) {
	reply(w, http.StatusOK, struct {
		SchemaVersion int                    `json:"schema_version"`
		Challenge     string                 `json:"challenge_b64url"`
		Signature     string                 `json:"producer_signature"`
		Producer      trust.ProducerMetadata `json:"producer"`
	}{result.SchemaVersion, result.Challenge, result.Signature, result.Producer})
}

func writeCalendarMirrorRelease(w http.ResponseWriter, result views.CalendarMirrorReleaseResult) {
	reply(w, http.StatusOK, struct {
		SchemaVersion int             `json:"schema_version"`
		Page          json.RawMessage `json:"page"`
	}{result.SchemaVersion, result.Page})
}
