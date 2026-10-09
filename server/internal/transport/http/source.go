package httptransport

import (
	"encoding/json"
	"errors"
	"net/http"
	"strings"
	"time"

	"floe/server/internal/trust"
	"floe/server/internal/views"
)

type sourcePreviewRequest struct {
	ConnectorID  string `json:"connector_id"`
	ConnectionID string `json:"connection_id"`
	Resource     string `json:"resource"`
}

type sourceAdmissionRequest struct {
	SchemaVersion      int             `json:"schema_version"`
	ConnectorID        string          `json:"connector_id"`
	ConnectionID       string          `json:"connection_id"`
	ConnectionRevision uint64          `json:"connection_revision"`
	Resources          []string        `json:"resources"`
	Grant              sourceGrantDTO  `json:"grant"`
	Purpose            string          `json:"purpose"`
	Consumer           string          `json:"consumer"`
	MaxItems           uint32          `json:"max_items"`
	MaxBytes           uint32          `json:"max_bytes"`
	Query              json.RawMessage `json:"query"`
}

type sourceGrantDTO struct {
	ID          string `json:"id"`
	Incarnation string `json:"incarnation"`
	Epoch       uint64 `json:"epoch"`
}

type sourcePreviewResponse struct {
	trust.ProducerMetadata
	Descriptor         string   `json:"descriptor_b64url"`
	Signature          string   `json:"producer_signature"`
	ExpiresAtUnixMS    int64    `json:"expires_at_unix_ms"`
	ConnectionRevision uint64   `json:"connection_revision"`
	SourceResources    []string `json:"source_resources"`
}

type sourceChallengeResponse struct {
	SchemaVersion int                    `json:"schema_version"`
	Operation     string                 `json:"operation"`
	ChallengeID   string                 `json:"challenge_id"`
	Challenge     string                 `json:"challenge_b64url"`
	Signature     string                 `json:"producer_signature"`
	Producer      trust.ProducerMetadata `json:"producer"`
	Expires       time.Time              `json:"expires"`
}

type sourceReleaseResponse struct {
	SchemaVersion int             `json:"schema_version"`
	View          json.RawMessage `json:"view"`
}

func serveSource(writer http.ResponseWriter, request *http.Request, principal trust.Principal, service *views.Service) {
	parts := strings.Split(strings.TrimPrefix(request.URL.Path, "/v1/views/"), "/")
	viewID := parts[0]
	if viewID != string(views.Calendar) && viewID != string(views.Communication) && viewID != string(views.WorkContext) && viewID != string(views.Logistics) {
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
		var input sourcePreviewRequest
		if !decodeSourceEnvelope(writer, request, map[string]struct{}{"connector_id": {}, "connection_id": {}, "resource": {}}, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		result, err := service.Preview(request.Context(), principal, viewID, views.PreviewRequest{ConnectorID: input.ConnectorID, ConnectionID: input.ConnectionID, Resource: input.Resource})
		if err != nil {
			writeViewError(writer, err)
			return
		}
		reply(writer, http.StatusOK, sourcePreviewResponse{result.Producer, result.Descriptor, result.Signature, result.ExpiresAtUnixMS, result.ConnectionRevision, result.SourceResources})
	case "admit":
		allowed := map[string]struct{}{"schema_version": {}, "connector_id": {}, "connection_id": {}, "connection_revision": {}, "resources": {}, "grant": {}, "purpose": {}, "consumer": {}, "max_items": {}, "max_bytes": {}, "query": {}}
		var input sourceAdmissionRequest
		if !decodeSourceEnvelope(writer, request, allowed, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		result, err := service.Admit(request.Context(), principal, viewID, views.AdmissionRequest{
			SchemaVersion: input.SchemaVersion, ConnectorID: input.ConnectorID, ConnectionID: input.ConnectionID,
			ConnectionRevision: input.ConnectionRevision, Resources: append([]string(nil), input.Resources...),
			Grant:   views.GrantReference{ID: input.Grant.ID, Incarnation: input.Grant.Incarnation, Epoch: input.Grant.Epoch},
			Purpose: input.Purpose, Consumer: input.Consumer, MaxItems: input.MaxItems, MaxBytes: input.MaxBytes,
			Query: append([]byte(nil), input.Query...),
		})
		if err != nil {
			writeViewError(writer, err)
			return
		}
		writeSourceChallenge(writer, result)
	case "read", "release":
		proof, ok := decodeSourceProof(writer, request)
		if !ok {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		if parts[1] == "release" {
			result, err := service.Release(request.Context(), principal, viewID, proof)
			if err != nil {
				writeViewError(writer, err)
				return
			}
			view, err := marshalReleasedView(result.View)
			if err != nil {
				failure(writer, http.StatusInternalServerError, "view_unavailable")
				return
			}
			reply(writer, http.StatusOK, sourceReleaseResponse{result.SchemaVersion, view})
		} else {
			result, err := service.Read(request.Context(), principal, viewID, proof)
			if err != nil {
				writeViewError(writer, err)
				return
			}
			writeSourceChallenge(writer, result)
		}
	default:
		failure(writer, http.StatusNotFound, "not_found")
	}
}

func writeSourceChallenge(writer http.ResponseWriter, result views.ChallengeResult) {
	reply(writer, http.StatusOK, sourceChallengeResponse{
		SchemaVersion: result.SchemaVersion, Operation: result.Operation, ChallengeID: result.ChallengeID,
		Challenge: result.Challenge, Signature: result.Signature, Producer: result.Producer, Expires: result.Expires,
	})
}

func writeViewError(writer http.ResponseWriter, err error) {
	var failureValue views.Error
	if !errors.As(err, &failureValue) {
		failure(writer, http.StatusInternalServerError, "operation_unavailable")
		return
	}
	status := map[views.ErrorCategory]int{
		views.ErrorInvalid: http.StatusBadRequest, views.ErrorDenied: http.StatusForbidden,
		views.ErrorConflict: http.StatusConflict, views.ErrorLimited: http.StatusTooManyRequests,
		views.ErrorUnavailable: http.StatusServiceUnavailable, views.ErrorUpstream: http.StatusBadGateway,
		views.ErrorInternal: http.StatusInternalServerError,
	}[failureValue.Category]
	if status == 0 {
		failure(writer, http.StatusInternalServerError, "operation_unavailable")
		return
	}
	failure(writer, status, failureValue.Code)
}

func marshalReleasedView(result views.Result) (json.RawMessage, error) {
	var raw []byte
	var err error
	switch result.ViewID {
	case views.Calendar:
		if result.Calendar == nil {
			return nil, views.ErrInvalid
		}
		raw, err = json.Marshal(result.Calendar)
	case views.Communication:
		if result.Communication == nil {
			return nil, views.ErrInvalid
		}
		raw, err = json.Marshal(result.Communication)
	case views.WorkContext:
		if result.Work == nil {
			return nil, views.ErrInvalid
		}
		raw, err = json.Marshal(result.Work)
	case views.Logistics:
		if result.Logistics == nil {
			return nil, views.ErrInvalid
		}
		raw, err = json.Marshal(result.Logistics)
	default:
		return nil, views.ErrInvalid
	}
	if err != nil {
		return nil, err
	}
	return json.RawMessage(raw), nil
}
