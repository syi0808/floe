package httptransport

import (
	"errors"
	"floe/server/internal/inference"
	"floe/server/internal/trust"
	"io"
	"mime"
	"net/http"
	"strings"
)

type livePrepareRequest struct {
	SchemaVersion      int    `json:"schema_version"`
	CapabilityRevision string `json:"capability_revision"`
}
type liveStartRequest struct {
	SchemaVersion      int      `json:"schema_version"`
	TicketID           string   `json:"ticket_id"`
	RequestID          string   `json:"request_id"`
	CapabilityRevision string   `json:"capability_revision"`
	DurationMS         uint64   `json:"duration_ms"`
	DataClasses        []string `json:"data_classes"`
	Instructions       string   `json:"instructions"`
	InitialContext     string   `json:"initial_context"`
	Offer              string   `json:"offer"`
}
type liveReleaseRequest struct {
	SchemaVersion int    `json:"schema_version"`
	TicketID      string `json:"ticket_id"`
}
type liveEventAckRequest struct {
	SchemaVersion int    `json:"schema_version"`
	TicketID      string `json:"ticket_id"`
	Sequence      uint64 `json:"sequence"`
	Digest        string `json:"digest"`
}
type liveEventDTO struct {
	SchemaVersion int                 `json:"schema_version"`
	TicketID      string              `json:"ticket_id"`
	Sequence      uint64              `json:"sequence"`
	Digest        string              `json:"digest"`
	Event         inference.LiveEvent `json:"event"`
}
type liveTicketDTO struct {
	SchemaVersion      int    `json:"schema_version"`
	TicketID           string `json:"ticket_id"`
	CapabilityRevision string `json:"capability_revision"`
	ExpiresAtUnixMS    int64  `json:"expires_at_unix_ms"`
}
type liveStatusDTO struct {
	SchemaVersion   int                   `json:"schema_version"`
	TicketID        string                `json:"ticket_id"`
	Phase           string                `json:"phase"`
	RequestID       string                `json:"request_id,omitempty"`
	ExpiresAtUnixMS int64                 `json:"expires_at_unix_ms"`
	Attempted       bool                  `json:"attempted"`
	Failure         inference.FailureCode `json:"failure,omitempty"`
	Answer          string                `json:"answer,omitempty"`
}

func liveStatusProjection(s inference.LiveCallStatus) liveStatusDTO {
	return liveStatusDTO{1, s.TicketID, s.Phase, s.RequestID, s.ExpiresAtUnixMS, s.Attempted, s.Failure, s.Answer}
}
func (h *InferenceHandler) decodeLiveBody(r *http.Request, out any, limit int64) error {
	if r.Method != http.MethodPost {
		return inference.Failure{Code: inference.MethodNotAllowed}
	}
	media, parameters, err := mime.ParseMediaType(r.Header.Get("Content-Type"))
	if err != nil || media != "application/json" || len(parameters) != 0 {
		return inference.Failure{Code: inference.ContentTypeUnsupported}
	}
	if r.ContentLength > limit {
		return inference.Failure{Code: inference.BodyTooLarge}
	}
	if limit > 2048 {
		h.liveBodiesOnce.Do(func() { h.liveBodies = make(chan struct{}, 2) })
		select {
		case h.liveBodies <- struct{}{}:
			defer func() { <-h.liveBodies }()
		default:
			return inference.Failure{Code: inference.ModelBusy}
		}
	}
	data, err := io.ReadAll(io.LimitReader(r.Body, limit+1))
	if err != nil {
		return inference.Failure{Code: inference.Validation}
	}
	if int64(len(data)) > limit {
		return inference.Failure{Code: inference.BodyTooLarge}
	}
	if !inference.ValidJSONTextEncoding(data) || trust.DecodeStrict(data, out, int(limit), 32) != nil {
		return inference.Failure{Code: inference.Validation}
	}
	return nil
}
func (h *InferenceHandler) serveLive(w http.ResponseWriter, r *http.Request, p trust.Principal) {
	fail := func(err error) { writeInferenceError(w, err) }
	if r.URL.RawPath != "" {
		fail(inference.Failure{Code: inference.Validation})
		return
	}
	switch r.URL.Path {
	case "/v1/live/prepare":
		var request livePrepareRequest
		if err := h.decodeLiveBody(r, &request, 2048); err != nil {
			fail(err)
			return
		}
		if request.SchemaVersion != 1 {
			fail(inference.Failure{Code: inference.UnsupportedSchema})
			return
		}
		ticket, err := h.Service.PrepareLiveCall(r.Context(), p, request.CapabilityRevision)
		if err != nil {
			fail(err)
			return
		}
		writeInferenceJSON(w, liveTicketDTO{1, ticket.ID, ticket.CapabilityRevision, ticket.ExpiresAtUnixMS})
	case "/v1/live/start":
		var request liveStartRequest
		if err := h.decodeLiveBody(r, &request, 16*1024*1024); err != nil {
			fail(err)
			return
		}
		if request.SchemaVersion != 1 {
			fail(inference.Failure{Code: inference.UnsupportedSchema})
			return
		}
		status, err := h.Service.StartLiveCall(r.Context(), p, inference.LiveStartCommand{TicketID: request.TicketID, RequestID: request.RequestID, CapabilityRevision: request.CapabilityRevision, DurationMS: request.DurationMS, DataClasses: request.DataClasses, Input: inference.LiveStartInput{Instructions: request.Instructions, InitialContext: request.InitialContext, Offer: request.Offer}})
		if err != nil {
			fail(err)
			return
		}
		writeInferenceJSON(w, liveStatusProjection(status))
	case "/v1/live/event/ack":
		var request liveEventAckRequest
		if err := h.decodeLiveBody(r, &request, 2048); err != nil {
			fail(err)
			return
		}
		if request.SchemaVersion != 1 {
			fail(inference.Failure{Code: inference.UnsupportedSchema})
			return
		}
		if err := h.Service.AcknowledgeLiveEvent(r.Context(), p, request.TicketID, request.Sequence, request.Digest); err != nil {
			fail(err)
			return
		}
		writeInferenceJSON(w, map[string]any{"schema_version": 1, "acknowledged_sequence": request.Sequence, "digest": request.Digest})
	case "/v1/live/release":
		var request liveReleaseRequest
		if err := h.decodeLiveBody(r, &request, 2048); err != nil {
			fail(err)
			return
		}
		if request.SchemaVersion != 1 {
			fail(inference.Failure{Code: inference.UnsupportedSchema})
			return
		}
		if err := h.Service.ReleaseLiveCall(p, request.TicketID); err != nil {
			fail(err)
			return
		}
		writeInferenceJSON(w, map[string]any{"schema_version": 1, "local_control_released": true})
	case "/v1/live/calls":
		if r.Method != http.MethodGet {
			fail(inference.Failure{Code: inference.MethodNotAllowed})
			return
		}
		if r.ContentLength != 0 {
			fail(inference.Failure{Code: inference.Validation})
			return
		}
		calls, err := h.Service.ListLiveCalls(p)
		if err != nil {
			fail(err)
			return
		}
		out := make([]liveStatusDTO, 0, len(calls))
		for _, call := range calls {
			out = append(out, liveStatusProjection(call))
		}
		writeInferenceJSON(w, map[string]any{"schema_version": 1, "calls": out})
	default:
		if !strings.HasPrefix(r.URL.Path, "/v1/live/calls/") {
			fail(inference.Failure{Code: inference.NotFound})
			return
		}
		if r.Method != http.MethodGet {
			fail(inference.Failure{Code: inference.MethodNotAllowed})
			return
		}
		if r.ContentLength != 0 {
			fail(inference.Failure{Code: inference.Validation})
			return
		}
		id := strings.TrimPrefix(r.URL.Path, "/v1/live/calls/")
		eventRead := strings.HasSuffix(id, "/event")
		if eventRead {
			id = strings.TrimSuffix(id, "/event")
		}
		if !trust.ValidID(id) {
			fail(inference.Failure{Code: inference.Validation})
			return
		}
		if eventRead {
			observed, err := h.Service.ObserveLiveEvent(r.Context(), p, id)
			if errors.Is(err, inference.ErrLiveObservationPending) {
				// A bounded empty observation is not a terminal call failure.
				w.WriteHeader(http.StatusNoContent)
				return
			}
			if err != nil {
				fail(err)
				return
			}
			writeInferenceJSON(w, liveEventDTO{1, id, observed.Delivery.Sequence, observed.Digest, observed.Delivery.Event})
			return
		}
		status, err := h.Service.LiveCallStatus(r.Context(), p, id)
		if err != nil {
			fail(err)
			return
		}
		writeInferenceJSON(w, liveStatusProjection(status))
	}
}
