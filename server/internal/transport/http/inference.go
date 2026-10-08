package httptransport

import (
	"encoding/json"
	"errors"
	"floe/server/internal/inference"
	"floe/server/internal/modelcatalog"
	"floe/server/internal/operation"
	"floe/server/internal/trust"
	"io"
	"mime"
	"net/http"
	"strings"
)

type InferenceHandler struct {
	Service      *inference.Service
	Trust        *trust.Service
	ModelCatalog *modelcatalog.Store
	Address      string
}

func (h *InferenceHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	if r.Host != h.Address {
		writeInferenceError(w, inference.Failure{Code: inference.PermissionDenied})
		return
	}
	operator := r.URL.Path == "/v1/generate" || r.URL.Path == "/v1/traces" || strings.HasPrefix(r.URL.Path, "/v1/traces/")
	var paired trust.Principal
	var admin trust.OperatorPrincipal
	var err error
	if operator {
		origin := r.Header.Get("Origin")
		if origin != "" && origin != "http://"+h.Address || r.Method != http.MethodGet && origin != "http://"+h.Address {
			writeInferenceError(w, inference.Failure{Code: inference.PermissionDenied})
			return
		}
		cookie, e := r.Cookie("floe_management")
		if e != nil {
			writeInferenceError(w, inference.Failure{Code: inference.Unauthorized})
			return
		}
		admin, err = h.Trust.AuthenticateOperatorSession(r.Context(), cookie.Value, r.Header.Get("X-Floe-CSRF"), r.Method != http.MethodGet)
	} else {
		if r.Header.Get("Origin") != "" {
			writeInferenceError(w, inference.Failure{Code: inference.PermissionDenied})
			return
		}
		auth := r.Header.Get("Authorization")
		if !strings.HasPrefix(auth, "Bearer ") {
			writeInferenceError(w, inference.Failure{Code: inference.Unauthorized})
			return
		}
		paired, err = h.Trust.AuthenticateBearer(r.Context(), strings.TrimPrefix(auth, "Bearer "))
	}
	if err != nil {
		writeInferenceError(w, inferenceTrustFailure(err))
		return
	}
	if r.URL.RawQuery != "" {
		writeInferenceError(w, inference.Failure{Code: inference.Validation})
		return
	}
	switch r.URL.Path {
	case "/v1/inference-purposes":
		if r.Method != http.MethodGet {
			writeInferenceError(w, inference.Failure{Code: inference.MethodNotAllowed})
			return
		}
		if r.ContentLength != 0 {
			writeInferenceError(w, inference.Failure{Code: inference.Validation})
			return
		}
		inventory, err := h.Service.ObservePurposes(r.Context(), paired)
		if err != nil {
			writeInferenceError(w, err)
			return
		}
		writeInferenceJSON(w, inventoryDTO(inventory, h.ModelCatalog))
	case "/v1/agent":
		if r.Method != http.MethodPost {
			writeInferenceError(w, inference.Failure{Code: inference.MethodNotAllowed})
			return
		}
		data, err := inferenceBody(r)
		if err != nil {
			writeInferenceError(w, err)
			return
		}
		in, err := decodeAgentRequest(data)
		if err != nil {
			writeInferenceError(w, err)
			return
		}
		out, err := h.Service.InvokeAgent(r.Context(), paired, in)
		if err != nil {
			writeInferenceError(w, err)
			return
		}
		writeInferenceJSON(w, AgentResponseDTO{3, out.Purpose, out.CapabilityRevision, out.AttemptID, out.TraceID, out.Output, out.CallIDs, out.Usage})
	case "/v1/generate":
		if r.Method != http.MethodPost {
			writeInferenceError(w, inference.Failure{Code: inference.MethodNotAllowed})
			return
		}
		data, err := inferenceBody(r)
		if err != nil {
			writeInferenceError(w, err)
			return
		}
		in, err := decodeStructuredRequest(data)
		if err != nil {
			writeInferenceError(w, err)
			return
		}
		out, err := h.Service.InvokeStructured(r.Context(), admin, in)
		if err != nil {
			writeInferenceError(w, err)
			return
		}
		writeInferenceJSON(w, StructuredResponseDTO{3, out.Purpose, out.CapabilityRevision, out.AttemptID, out.TraceID, out.Output, out.Usage})
	case "/v1/traces":
		if r.Method != http.MethodGet {
			writeInferenceError(w, inference.Failure{Code: inference.MethodNotAllowed})
			return
		}
		out, err := h.Service.Traces(admin, 20)
		if err != nil {
			writeInferenceError(w, inferenceTrustFailure(err))
			return
		}
		writeInferenceJSON(w, map[string]any{"schema_version": 3, "traces": out})
	default:
		if strings.HasPrefix(r.URL.Path, "/v1/traces/") {
			if r.Method != http.MethodGet {
				writeInferenceError(w, inference.Failure{Code: inference.MethodNotAllowed})
				return
			}
			out, err := h.Service.Trace(admin, strings.TrimPrefix(r.URL.Path, "/v1/traces/"))
			if err != nil {
				writeInferenceError(w, err)
				return
			}
			writeInferenceJSON(w, map[string]any{"schema_version": 3, "trace": out})
			return
		}
		writeInferenceError(w, inference.Failure{Code: inference.NotFound})
	}
}
func inferenceBody(r *http.Request) ([]byte, error) {
	content, _, err := mime.ParseMediaType(r.Header.Get("Content-Type"))
	if err != nil || content != "application/json" {
		return nil, inference.Failure{Code: inference.ContentTypeUnsupported}
	}
	data, err := io.ReadAll(io.LimitReader(r.Body, 98305))
	if err != nil {
		return nil, inference.Failure{Code: inference.Validation}
	}
	if len(data) > 98304 {
		return nil, inference.Failure{Code: inference.BodyTooLarge}
	}
	return data, nil
}
func writeInferenceJSON(w http.ResponseWriter, value any) {
	data, err := json.Marshal(value)
	if err != nil || len(data) > 65536 {
		writeInferenceError(w, inference.Failure{Code: inference.InvalidOutput})
		return
	}
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write(data)
}
func writeInferenceError(w http.ResponseWriter, err error) {
	var f inference.Failure
	if !errors.As(err, &f) {
		f = inference.Failure{Code: inference.ModelUnavailable}
	}
	status := inferenceStatus(f.Code)
	out := InferenceErrorDTO{SchemaVersion: 3}
	out.Error.Code = f.Code
	// Only the execution service may attach the admitted correlation. Request
	// decoding and other pre-admission failures expose no caller-supplied echo.
	if f.Dispatched && f.TraceID != "" && trust.ValidID(f.AttemptID) && inference.ValidPurpose(string(f.Purpose)) && inference.ValidHex(f.CapabilityRevision, 32) {
		out.TraceID = &f.TraceID
		out.AttemptID = &f.AttemptID
		out.Purpose = &f.Purpose
		out.CapabilityRevision = &f.CapabilityRevision
		if inference.ValidateUsage(f.Usage) == nil {
			out.Usage = f.Usage
		}
	}
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(out)
}
func inferenceStatus(code inference.FailureCode) int {
	switch code {
	case inference.Validation, inference.UnsupportedSchema:
		return 400
	case inference.Unauthorized:
		return 401
	case inference.PermissionDenied, inference.IdentityMismatch:
		return 403
	case inference.NotFound:
		return 404
	case inference.MethodNotAllowed:
		return 405
	case inference.CapabilityChanged, inference.PurposeNotConfigured, inference.PurposeDisabled:
		return 409
	case inference.BodyTooLarge:
		return 413
	case inference.ContentTypeUnsupported:
		return 415
	case inference.ModelBusy, inference.QuotaExceeded:
		return 429
	case inference.InvalidOutput, inference.RequestRejected:
		return 502
	case inference.ModelTimeout:
		return 504
	default:
		return 503
	}
}

func inferenceTrustFailure(err error) error {
	var e operation.Error
	if errors.As(err, &e) {
		switch e.Category {
		case operation.Denied:
			return inference.Failure{Code: inference.PermissionDenied}
		case operation.Unavailable:
			return inference.Failure{Code: inference.ModelUnavailable}
		}
	}
	return inference.Failure{Code: inference.Unauthorized}
}
