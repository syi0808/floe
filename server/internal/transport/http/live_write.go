package httptransport

import (
	"floe/server/internal/inference"
	"floe/server/internal/trust"
	"net/http"
)

type liveAppendRequest struct {
	SchemaVersion int    `json:"schema_version"`
	TicketID      string `json:"ticket_id"`
	OperationID   string `json:"operation_id"`
	Kind          string `json:"kind"`
	DelegationID  string `json:"delegation_id,omitempty"`
	Text          string `json:"text"`
}
type liveCloseRequest struct {
	SchemaVersion int    `json:"schema_version"`
	TicketID      string `json:"ticket_id"`
	OperationID   string `json:"operation_id"`
}
type liveWriteDTO struct {
	SchemaVersion   int                   `json:"schema_version"`
	OperationID     string                `json:"operation_id"`
	Phase           string                `json:"phase"`
	Attempted       bool                  `json:"attempted"`
	CompletedFrames int                   `json:"completed_frames"`
	Failure         inference.FailureCode `json:"failure,omitempty"`
}

func liveWriteProjection(s inference.LiveWriteStatus) liveWriteDTO {
	return liveWriteDTO{1, s.OperationID, s.Phase, s.Attempted, s.CompletedFrames, s.Failure}
}
func (h *InferenceHandler) serveLiveWrite(w http.ResponseWriter, r *http.Request, p trust.Principal) bool {
	switch r.URL.Path {
	case "/v1/live/append":
		var in liveAppendRequest
		if err := h.decodeLiveBody(r, &in, 512*1024); err != nil {
			writeInferenceError(w, err)
			return true
		}
		if in.SchemaVersion != 1 {
			writeInferenceError(w, inference.Failure{Code: inference.UnsupportedSchema})
			return true
		}
		out, err := h.Service.AppendLiveCall(r.Context(), p, in.TicketID, inference.LiveAppend{OperationID: in.OperationID, Kind: in.Kind, DelegationID: in.DelegationID, Text: in.Text})
		if err != nil {
			writeInferenceError(w, err)
		} else {
			writeInferenceJSON(w, liveWriteProjection(out))
		}
		return true
	case "/v1/live/close":
		var in liveCloseRequest
		if err := h.decodeLiveBody(r, &in, 2048); err != nil {
			writeInferenceError(w, err)
			return true
		}
		if in.SchemaVersion != 1 {
			writeInferenceError(w, inference.Failure{Code: inference.UnsupportedSchema})
			return true
		}
		out, err := h.Service.CloseLiveCall(r.Context(), p, in.TicketID, in.OperationID)
		if err != nil {
			writeInferenceError(w, err)
		} else {
			writeInferenceJSON(w, liveWriteProjection(out))
		}
		return true
	}
	return false
}
