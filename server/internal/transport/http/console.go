package httptransport

import (
	"context"
	"encoding/json"
	"io"
	"mime"
	"net/http"
	"strings"

	"floe/server/internal/authorization"
	"floe/server/internal/connections"
	"floe/server/internal/inference"
	"floe/server/internal/operation"
	"floe/server/internal/pairing"
)

type RouteRequest struct {
	Class           string `json:"inference_class"`
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
	Classes  map[string]inference.ProfileClass `json:"classes"`
}
type TestRequest struct {
	ID            string `json:"id"`
	AllowExternal bool   `json:"allow_external"`
}
type Management struct {
	State        func() operation.Result
	Codex        func(context.Context, string) operation.Result
	Route        func(RouteRequest) operation.Result
	Target       func(TargetRequest) operation.Result
	Provider     func(ProviderRequest) operation.Result
	Test         func(context.Context, TestRequest) operation.Result
	DeleteClient func(string) operation.Result
	DeleteTarget func(string) operation.Result
	Authority    func() AuthorityHandler
}
type ConnectorOperations struct {
	Catalog    func() operation.Result
	Start      func(context.Context, string, connections.ConnectRequest) operation.Result
	Attempt    func(context.Context, string, string) operation.Result
	Cancel     func(context.Context, string, string) operation.Result
	Update     func(string, connections.ScopeRequest) operation.Result
	Disconnect func(context.Context, string, connections.DisconnectRequest) operation.Result
}
type Client struct {
	Scope      connections.Scope
	List       func(context.Context) operation.Result
	Connectors ConnectorOperations
	Authority  AuthorityHandler
	Sources    *authorization.SourceService
	Inference  http.Handler
}
type Handler struct {
	Address      string
	Sessions     *Sessions
	Pairing      func() *pairing.Operations
	Authenticate func(string) (Client, operation.Result)
	Management   Management
}

func (handler *Handler) ServeHTTP(writer http.ResponseWriter, request *http.Request) {
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
				return handler.Pairing().Execute(strings.TrimPrefix(request.URL.Path, "/pair/"), input)
			})
		} else {
			handler.serveClient(writer, request)
		}
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
		token, result := handler.Sessions.Login(input.Token)
		if result.Code == "" {
			http.SetCookie(writer, &http.Cookie{Name: "floe_management", Value: token, Path: "/manage", HttpOnly: true, SameSite: http.SameSiteStrictMode, MaxAge: 43200})
		}
		writeResult(writer, result)
		return
	}
	cookie, err := request.Cookie("floe_management")
	if err != nil {
		failure(writer, http.StatusUnauthorized, "unauthorized")
		return
	}
	current, ok := handler.Sessions.Lookup(cookie.Value)
	if !ok || request.Method != http.MethodGet && request.Header.Get("X-Floe-CSRF") != current.CSRF {
		failure(writer, http.StatusUnauthorized, "unauthorized")
		return
	}
	handler.manage(writer, request, cookie.Value, current)
}

func (handler *Handler) serveClient(writer http.ResponseWriter, request *http.Request) {
	auth := request.Header.Get("Authorization")
	if !strings.HasPrefix(auth, "Bearer ") {
		failure(writer, http.StatusUnauthorized, "unauthorized")
		return
	}
	client, result := handler.Authenticate(strings.TrimPrefix(auth, "Bearer "))
	if result.Code != "" {
		writeResult(writer, result)
		return
	}
	principal := authorization.Principal{ClientID: client.Scope.ClientID, PersonID: client.Scope.PersonID, DeviceID: client.Scope.DeviceID, Authenticated: true}
	if client.Authority.ServeClient(writer, request, principal) {
		return
	}
	if request.URL.Path == "/v1/authority/calendar/source" && request.Method == http.MethodPost {
		var input authorization.SourcePreview
		if !strictDecode(writer, request, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		writeResult(writer, client.Sources.PreviewCalendar(principal, input))
		return
	}
	if strings.HasPrefix(request.URL.Path, "/v1/connectors") {
		ServeConnectors(writer, request, client.Connectors)
		return
	}
	if request.URL.Path == "/v1/connections" {
		if request.Method != http.MethodGet {
			failure(writer, http.StatusNotFound, "not_found")
			return
		}
		writeResult(writer, client.List(request.Context()))
		return
	}
	if strings.HasPrefix(request.URL.Path, "/v1/views/") {
		serveSource(writer, request, principal, client.Sources)
		return
	}
	client.Inference.ServeHTTP(writer, request)
}

func (handler *Handler) manage(writer http.ResponseWriter, request *http.Request, token string, current sessionRecord) {
	if strings.HasPrefix(request.URL.Path, "/manage/api/authority/") {
		handler.Management.Authority().ServeAdmin(writer, request)
		return
	}
	if request.URL.Path == "/manage/api/state" && request.Method == http.MethodGet {
		result := handler.Management.State()
		if result.Code == "" {
			result.Value.(map[string]any)["csrf"] = current.CSRF
		}
		writeResult(writer, result)
		return
	}
	if request.Method != http.MethodPost {
		failure(writer, http.StatusMethodNotAllowed, "method_not_allowed")
		return
	}
	switch request.URL.Path {
	case "/manage/api/logout":
		handler.Sessions.Delete(token)
		http.SetCookie(writer, &http.Cookie{Name: "floe_management", Path: "/manage", MaxAge: -1, HttpOnly: true, SameSite: http.SameSiteStrictMode})
		reply(writer, http.StatusOK, map[string]bool{"ok": true})
	case "/manage/api/pair/approve":
		dispatch(writer, request, handler.Pairing().Approve)
	case "/manage/api/pair/reject":
		dispatch(writer, request, handler.Pairing().Reject)
	case "/manage/api/route":
		dispatch(writer, request, handler.Management.Route)
	case "/manage/api/target":
		dispatch(writer, request, handler.Management.Target)
	case "/manage/api/provider":
		dispatch(writer, request, handler.Management.Provider)
	case "/manage/api/test":
		dispatch(writer, request, func(input TestRequest) operation.Result { return handler.Management.Test(request.Context(), input) })
	case "/manage/api/client/delete", "/manage/api/target/delete":
		var input struct {
			ID string `json:"id"`
		}
		if !decode(writer, request, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		if request.URL.Path == "/manage/api/client/delete" {
			writeResult(writer, handler.Management.DeleteClient(input.ID))
		} else {
			writeResult(writer, handler.Management.DeleteTarget(input.ID))
		}
	default:
		if strings.HasPrefix(request.URL.Path, "/manage/api/codex/") {
			writeResult(writer, handler.Management.Codex(request.Context(), strings.TrimPrefix(request.URL.Path, "/manage/api/codex/")))
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

func AuthenticatedInference(gateway http.Handler, token string) http.Handler {
	return http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		forward := request.Clone(request.Context())
		forward.Header.Set("Authorization", "Bearer "+token)
		gateway.ServeHTTP(writer, forward)
	})
}

func decode(writer http.ResponseWriter, request *http.Request, output any) bool {
	mediaType, _, err := mime.ParseMediaType(request.Header.Get("Content-Type"))
	if err != nil || mediaType != "application/json" {
		return false
	}
	decoder := json.NewDecoder(http.MaxBytesReader(writer, request.Body, 16384))
	decoder.DisallowUnknownFields()
	return decoder.Decode(output) == nil && decoder.Decode(new(any)) == io.EOF
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
