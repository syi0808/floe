package httptransport

import (
	"encoding/json"
	"io"
	"net/http"

	"floe/server/internal/authority"
	"floe/server/internal/trust"
)

type sourceProofWire struct {
	SchemaVersion int             `json:"schema_version"`
	Proof         json.RawMessage `json:"proof"`
}

func decodeSourceEnvelope(writer http.ResponseWriter, request *http.Request, allowed map[string]struct{}, output any) bool {
	data, err := io.ReadAll(http.MaxBytesReader(writer, request.Body, authority.MaxChallengeBytes))
	if err != nil || trust.DecodeStrict(data, output, authority.MaxChallengeBytes, authority.MaxJSONDepth) != nil {
		return false
	}
	var fields map[string]json.RawMessage
	if json.Unmarshal(data, &fields) != nil || len(fields) != len(allowed) {
		return false
	}
	for key, value := range fields {
		if _, ok := allowed[key]; !ok || string(value) == "null" {
			return false
		}
	}
	return true
}
func decodeSourceProof(writer http.ResponseWriter, request *http.Request) (trust.Proof, bool) {
	var envelope sourceProofWire
	if !decodeSourceEnvelope(writer, request, map[string]struct{}{"schema_version": {}, "proof": {}}, &envelope) || envelope.SchemaVersion != authority.SchemaVersion || len(envelope.Proof) == 0 {
		return trust.Proof{}, false
	}
	proof, err := trust.ParseProofJSON(envelope.Proof)
	return proof, err == nil
}
