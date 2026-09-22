package httptransport

import (
	"encoding/json"
	"io"
	"net/http"
	"strings"

	"floe/server/internal/authorization"
)

type calendarProofWire struct {
	SchemaVersion int             `json:"schema_version"`
	Proof         json.RawMessage `json:"proof"`
}

func decodeCalendarEnvelope(writer http.ResponseWriter, request *http.Request, allowed map[string]struct{}, output any) bool {
	data, err := io.ReadAll(http.MaxBytesReader(writer, request.Body, authorization.MaxChallengeBytes))
	if err != nil || len(data) == 0 || !authorization.StrictJSON(data) || !authorization.ValidateCalendarCaseExact(data) || !authorization.ValidateCalendarObjectKeys(data, allowed) {
		return false
	}
	decoder := json.NewDecoder(strings.NewReader(string(data)))
	decoder.DisallowUnknownFields()
	if decoder.Decode(output) != nil {
		return false
	}
	var extra any
	return decoder.Decode(&extra) == io.EOF
}
func decodeCalendarProof(writer http.ResponseWriter, request *http.Request) (authorization.Proof, bool) {
	var envelope calendarProofWire
	if !decodeCalendarEnvelope(writer, request, map[string]struct{}{"schema_version": {}, "proof": {}}, &envelope) || envelope.SchemaVersion != authorization.SchemaVersion || len(envelope.Proof) == 0 {
		return authorization.Proof{}, false
	}
	proof, err := authorization.ParseProofJSON(envelope.Proof)
	return proof, err == nil
}
