package httptransport

import (
	"io"
	"mime"
	"net/http"
	"strings"

	"floe/server/internal/authority"
	"floe/server/internal/inference"
	"floe/server/internal/integrations"
	"floe/server/internal/operation"
	"floe/server/internal/pairing"
	"floe/server/internal/trust"
)

type RouteRequest struct {
	Purpose         string `json:"purpose"`
	Enabled         bool   `json:"enabled"`
	Target          string `json:"target"`
	ReasoningEffort string `json:"reasoning_effort"`
}
type TargetRequest struct {
	ID       string `json:"id"`
	Provider string `json:"provider"`
	BaseURL  string `json:"base_url"`
	Model    string `json:"model"`
	APIKey   string `json:"api_key"`
}
type ProviderRequest struct {
	Provider string                            `json:"provider"`
	BaseURL  string                            `json:"base_url"`
	APIKey   string                            `json:"api_key"`
	Purposes map[string]inference.PurposeModel `json:"purposes"`
}
type TestRequest struct {
	ID string `json:"id"`
}
type Handler struct {
	Address       string
	Trust         *trust.Service
	Inference     *InferenceHandler
	Setup         HostedSetup
	Pairing       *pairing.Operations
	Integrations  *integrations.Service
	Sources       *authority.SourceService
	Mirror        *authority.CalendarMirrorService
	Configuration *inference.Configuration
	Accounts      *inference.AccountManagement
	Clients       *trust.ClientAdministration
}

func (handler *Handler) ServeHTTP(writer http.ResponseWriter, request *http.Request) {
	if request.URL.Path == "/v1/inference-purposes" || request.URL.Path == "/v1/agent" || request.URL.Path == "/v1/generate" || strings.HasPrefix(request.URL.Path, "/v1/traces") {
		handler.Inference.ServeHTTP(writer, request)
		return
	}
	writer.Header().Set("Cache-Control", "no-store")
	writer.Header().Set("X-Content-Type-Options", "nosniff")
	writer.Header().Set("Referrer-Policy", "no-referrer")
	writer.Header().Set("Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'")
	if request.Host != handler.Address {
		failure(writer, http.StatusForbidden, "invalid_host")
		return
	}
	origin := request.Header.Get("Origin")
	if origin != "" && origin != "http://"+handler.Address {
		failure(writer, http.StatusForbidden, "invalid_origin")
		return
	}
	if strings.HasPrefix(request.URL.Path, "/v1/") || strings.HasPrefix(request.URL.Path, "/pair/") {
		if origin != "" {
			failure(writer, http.StatusForbidden, "unauthorized")
			return
		}
		if strings.HasPrefix(request.URL.Path, "/pair/") {
			if request.Method != http.MethodPost {
				failure(writer, http.StatusNotFound, "not_found")
				return
			}
			dispatch(writer, request, func(input pairing.Request) operation.Result {
				return handler.Pairing.Execute(request.Context(), strings.TrimPrefix(request.URL.Path, "/pair/"), input)
			})
		} else {
			handler.serveClient(writer, request)
		}
		return
	}
	if strings.HasPrefix(request.URL.Path, "/manage/setup/") {
		handler.serveHostedSetup(writer, request)
		return
	}
	if request.Method == http.MethodGet && (request.URL.Path == "/" || request.URL.Path == "/manage" || request.URL.Path == "/manage/" || request.URL.Path == "/manage/app.js" || request.URL.Path == "/manage/style.css") {
		name, contentType := "index.html", "text/html; charset=utf-8"
		if strings.HasSuffix(request.URL.Path, "app.js") {
			name, contentType = "app.js", "text/javascript; charset=utf-8"
		}
		if strings.HasSuffix(request.URL.Path, "style.css") {
			name, contentType = "style.css", "text/css; charset=utf-8"
		}
		data, _ := assets.ReadFile("web/" + name)
		writer.Header().Set("Content-Type", contentType)
		_, _ = writer.Write(data)
		return
	}
	if request.Method == http.MethodPost && origin != "http://"+handler.Address {
		failure(writer, http.StatusForbidden, "invalid_origin")
		return
	}
	if request.URL.Path == "/manage/api/login" && request.Method == http.MethodPost {
		var input struct {
			Token string `json:"token"`
		}
		if !decode(writer, request, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		token, result := handler.Trust.LoginOperator(input.Token)
		if result.Code == "" {
			http.SetCookie(writer, &http.Cookie{Name: "floe_management", Value: token, Path: "/", HttpOnly: true, SameSite: http.SameSiteStrictMode, MaxAge: 43200})
		}
		writeResult(writer, result)
		return
	}
	cookie, err := request.Cookie("floe_management")
	if err != nil {
		failure(writer, http.StatusUnauthorized, "unauthorized")
		return
	}
	current, ok := handler.Trust.OperatorSession(cookie.Value)
	if !ok {
		failure(writer, http.StatusUnauthorized, "unauthorized")
		return
	}
	operator, authErr := handler.Trust.AuthenticateOperatorSession(request.Context(), cookie.Value, request.Header.Get("X-Floe-CSRF"), request.Method != http.MethodGet)
	if authErr != nil {
		failure(writer, http.StatusUnauthorized, "unauthorized")
		return
	}
	handler.manage(writer, request, cookie.Value, current, operator)
}

func (handler *Handler) serveClient(writer http.ResponseWriter, request *http.Request) {
	auth := request.Header.Get("Authorization")
	if !strings.HasPrefix(auth, "Bearer ") {
		failure(writer, http.StatusUnauthorized, "unauthorized")
		return
	}
	principal, err := handler.Trust.AuthenticateBearer(request.Context(), strings.TrimPrefix(auth, "Bearer "))
	if err != nil {
		writeResult(writer, trust.Result(err))
		return
	}
	if strings.HasPrefix(request.URL.Path, "/v1/calendar/mirror/") {
		serveCalendarMirror(writer, request, principal, handler.Mirror)
		return
	}
	if strings.HasPrefix(request.URL.Path, "/v1/connectors") {
		ServeConnectors(writer, request, principal, handler.Integrations)
		return
	}
	if request.URL.Path == "/v1/connections" {
		if request.Method != http.MethodGet {
			failure(writer, http.StatusNotFound, "not_found")
			return
		}
		writeResult(writer, handler.Integrations.List(request.Context(), principal))
		return
	}
	if strings.HasPrefix(request.URL.Path, "/v1/views/") {
		serveSource(writer, request, principal, handler.Sources)
		return
	}
	failure(writer, http.StatusNotFound, "not_found")
}

func (handler *Handler) manage(writer http.ResponseWriter, request *http.Request, token string, current trust.OperatorSession, operator trust.OperatorPrincipal) {
	if strings.HasPrefix(request.URL.Path, "/manage/api/authority/") {
		AuthorityHandler{Trust: handler.Trust}.ServeAdmin(writer, request)
		return
	}
	if request.URL.Path == "/manage/api/state" && request.Method == http.MethodGet {
		result := handler.managementState(request, operator)
		if result.Code == "" {
			result.Value.(map[string]any)["csrf"] = current.CSRF
		}
		writeResult(writer, result)
		return
	}
	if request.Method != http.MethodPost {
		if request.URL.Path == "/manage/api/pair/approve" || request.URL.Path == "/manage/api/pair/reject" || request.URL.Path == "/manage/api/pair/recover" {
			failure(writer, http.StatusMethodNotAllowed, "method_not_allowed")
		} else {
			failure(writer, http.StatusNotFound, "not_found")
		}
		return
	}
	switch request.URL.Path {
	case "/manage/api/logout":
		handler.Trust.LogoutOperator(token)
		http.SetCookie(writer, &http.Cookie{Name: "floe_management", Path: "/", MaxAge: -1, HttpOnly: true, SameSite: http.SameSiteStrictMode})
		reply(writer, http.StatusOK, map[string]bool{"ok": true})
	case "/manage/api/pair/approve":
		dispatch(writer, request, func(in pairing.ApprovalRequest) operation.Result {
			return handler.Pairing.Approve(request.Context(), operator, in)
		})
	case "/manage/api/pair/recover":
		dispatch(writer, request, func(in pairing.RecoveryRequest) operation.Result {
			return handler.Pairing.Recover(request.Context(), operator, in)
		})
	case "/manage/api/pair/reject":
		dispatch(writer, request, func(in pairing.RejectionRequest) operation.Result {
			return handler.Pairing.Reject(request.Context(), operator, in)
		})
	case "/manage/api/route":
		dispatch(writer, request, func(in RouteRequest) operation.Result {
			return handler.Configuration.UpdateRoute(request.Context(), operator, inference.RouteUpdate{Purpose: in.Purpose, Enabled: in.Enabled, Target: in.Target, ReasoningEffort: in.ReasoningEffort})
		})
	case "/manage/api/target":
		dispatch(writer, request, func(in TargetRequest) operation.Result {
			return handler.Configuration.UpdateTarget(request.Context(), operator, inference.TargetUpdate{ID: in.ID, Provider: in.Provider, BaseURL: in.BaseURL, Model: in.Model, APIKey: in.APIKey})
		})
	case "/manage/api/provider":
		dispatch(writer, request, func(in ProviderRequest) operation.Result {
			return handler.Configuration.UpdateProvider(request.Context(), operator, inference.ProviderUpdate{Provider: in.Provider, BaseURL: in.BaseURL, APIKey: in.APIKey, Purposes: in.Purposes})
		})
	case "/manage/api/test":
		dispatch(writer, request, func(input TestRequest) operation.Result {
			result, err := handler.Inference.Service.ProbeTarget(request.Context(), operator, input.ID)
			if err != nil {
				return operation.Reject(operation.Upstream, "model_unavailable")
			}
			return operation.Accept(map[string]any{"ok": true, "elapsed_ms": result.ElapsedMS, "trace_id": result.TraceID})
		})
	case "/manage/api/client/delete", "/manage/api/target/delete":
		var input struct {
			ID string `json:"id"`
		}
		if !decode(writer, request, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		if request.URL.Path == "/manage/api/client/delete" {
			writeResult(writer, handler.Clients.Revoke(request.Context(), operator, input.ID))
		} else {
			writeResult(writer, handler.Configuration.DeleteTarget(request.Context(), operator, input.ID))
		}
	default:
		if strings.HasPrefix(request.URL.Path, "/manage/api/codex/") {
			writeResult(writer, handler.Accounts.Execute(request.Context(), operator, inference.AccountCommand(strings.TrimPrefix(request.URL.Path, "/manage/api/codex/"))))
			return
		}
		failure(writer, http.StatusNotFound, "not_found")
	}
}

func dispatch[Input any](writer http.ResponseWriter, request *http.Request, execute func(Input) operation.Result) {
	var input Input
	if !decode(writer, request, &input) {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	writeResult(writer, execute(input))
}

func decode(writer http.ResponseWriter, request *http.Request, output any) bool {
	mediaType, _, err := mime.ParseMediaType(request.Header.Get("Content-Type"))
	if err != nil || mediaType != "application/json" {
		return false
	}
	data, err := io.ReadAll(http.MaxBytesReader(writer, request.Body, 16384))
	if err != nil || !StrictJSON(data) {
		return false
	}
	return trust.DecodeStrict(data, output, 16384, 32) == nil
}

func writeResult(writer http.ResponseWriter, result operation.Result) {
	statuses := map[operation.Category]int{
		operation.Ready: http.StatusOK, operation.Created: http.StatusCreated,
		operation.Invalid: http.StatusBadRequest, operation.Unauthenticated: http.StatusUnauthorized,
		operation.Denied: http.StatusForbidden, operation.Missing: http.StatusNotFound,
		operation.Conflict: http.StatusConflict, operation.Limited: http.StatusTooManyRequests,
		operation.Unavailable: http.StatusServiceUnavailable, operation.Upstream: http.StatusBadGateway,
		operation.Internal: http.StatusInternalServerError,
	}
	status, ok := statuses[result.Category]
	if !ok {
		failure(writer, http.StatusInternalServerError, "operation_unavailable")
		return
	}
	if result.Code != "" {
		failure(writer, status, result.Code)
		return
	}
	reply(writer, status, result.Value)
}

// managementState is a redacted transport projection of owner snapshots.
func (handler *Handler) managementState(request *http.Request, operator trust.OperatorPrincipal) operation.Result {
	config, err := handler.Configuration.Snapshot(request.Context(), operator)
	if err != nil {
		return operation.Reject(operation.Unavailable, "configuration_unavailable")
	}
	clients, err := handler.Trust.Clients()
	if err != nil {
		return trust.Result(err)
	}
	ids := []string{}
	scopes := map[string]any{}
	for _, client := range clients {
		ids = append(ids, client.ClientID)
		scopes[client.ClientID] = map[string]string{"person_id": client.PersonID, "device_id": client.DeviceID}
	}
	traces, err := handler.Inference.Service.Traces(operator, 20)
	if err != nil {
		return operation.Reject(operation.Unauthenticated, "unauthorized")
	}
	var inventory any = config.Inventory
	if !config.InventoryAvailable {
		inventory = nil
	}
	pending, err := handler.Pairing.Pending(request.Context())
	if err != nil {
		return trust.Result(err)
	}
	return operation.Accept(map[string]any{"providers": config.Profiles, "clients": ids, "client_scopes": scopes, "pairing": pending, "address": "http://" + handler.Address, "traces": traces, "inventory": inventory})
}

func (handler *Handler) ServeUnavailable(writer http.ResponseWriter) {
	writer.Header().Set("Cache-Control", "no-store")
	failure(writer, http.StatusServiceUnavailable, "node_closed")
}
