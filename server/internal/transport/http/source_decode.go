package httptransport

import (
	"encoding/json"
	"io"
	"net/http"
	"strings"

	"floe/server/internal/authority"
 "floe/server/internal/trust"
)

type sourceProofWire struct {
	SchemaVersion int             `json:"schema_version"`
	Proof         json.RawMessage `json:"proof"`
}

func decodeSourceEnvelope(writer http.ResponseWriter, request *http.Request, allowed map[string]struct{}, output any) bool {
	data, err := io.ReadAll(http.MaxBytesReader(writer, request.Body, authority.MaxChallengeBytes))
	if err != nil || len(data) == 0 || !authority.StrictJSON(data) || !authority.ValidateCalendarCaseExact(data) || !authority.ValidateCalendarObjectKeys(data, allowed) {
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
func decodeSourceProof(writer http.ResponseWriter, request *http.Request) (trust.Proof, bool) {
	var envelope sourceProofWire
	if !decodeSourceEnvelope(writer, request, map[string]struct{}{"schema_version": {}, "proof": {}}, &envelope) || envelope.SchemaVersion != authority.SchemaVersion || len(envelope.Proof) == 0 {
		return trust.Proof{}, false
	}
	proof, err := trust.ParseProofJSON(envelope.Proof)
	return proof, err == nil
}
