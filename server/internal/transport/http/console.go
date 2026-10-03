package httptransport

import (
	"context"
	"io"
	"mime"
	"net/http"
	"strings"

	"floe/server/internal/authority"
 "floe/server/internal/trust"
	"floe/server/internal/connections"
	"floe/server/internal/inference"
	"floe/server/internal/operation"
	"floe/server/internal/pairing"
)

type RouteRequest struct {
	Purpose         string `json:"purpose"`
 Enabled bool `json:"enabled"`
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
	ID            string `json:"id"`
}
type Management struct {
	State        func(trust.OperatorPrincipal) operation.Result
	Codex        func(context.Context, string) operation.Result
	Route        func(RouteRequest) operation.Result
	Target       func(TargetRequest) operation.Result
	Provider     func(ProviderRequest) operation.Result
	Test         func(context.Context, trust.OperatorPrincipal, TestRequest) operation.Result
	DeleteClient func(string) operation.Result
	DeleteTarget func(string) operation.Result
}
type ConnectorOperations struct {
	Catalog    func() operation.Result
	Start      func(context.Context, string, connections.ConnectRequest) operation.Result
	Attempt    func(context.Context, string, string) operation.Result
	Cancel     func(context.Context, string, string, connections.CancelSetupRequest) operation.Result
	Update     func(string, connections.ScopeRequest) operation.Result
	Disconnect func(context.Context, string, connections.DisconnectRequest) operation.Result
}
type Client struct {
	Principal trust.Principal
	List       func(context.Context) operation.Result
	Connectors ConnectorOperations
	Sources    *authority.SourceService
}
type Handler struct {
	Address      string
	Trust *trust.Service
 Inference *InferenceHandler
 Setup HostedSetup
	Pairing *pairing.Operations
	Authenticate func(context.Context,string) (Client, operation.Result)
	Management   Management
}

func (handler *Handler) ServeHTTP(writer http.ResponseWriter, request *http.Request) {
	if request.URL.Path=="/v1/inference-purposes"||request.URL.Path=="/v1/agent"||request.URL.Path=="/v1/generate"||strings.HasPrefix(request.URL.Path,"/v1/traces") { handler.Inference.ServeHTTP(writer,request);return }
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
 if strings.HasPrefix(request.URL.Path,"/manage/setup/"){handler.serveHostedSetup(writer,request);return}
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
	operator, authErr := handler.Trust.AuthenticateOperatorSession(request.Context(),cookie.Value,request.Header.Get("X-Floe-CSRF"),request.Method!=http.MethodGet)
 if authErr!=nil { failure(writer,http.StatusUnauthorized,"unauthorized");return }
 handler.manage(writer, request, cookie.Value, current, operator)
}

func (handler *Handler) serveClient(writer http.ResponseWriter, request *http.Request) {
	auth := request.Header.Get("Authorization")
	if !strings.HasPrefix(auth, "Bearer ") {
		failure(writer, http.StatusUnauthorized, "unauthorized")
		return
	}
	client, result := handler.Authenticate(request.Context(),strings.TrimPrefix(auth, "Bearer "))
	if result.Code != "" {
		writeResult(writer, result)
		return
	}
 principal := client.Principal
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
	failure(writer,http.StatusNotFound,"not_found")
}

func (handler *Handler) manage(writer http.ResponseWriter, request *http.Request, token string, current trust.OperatorSession, operator trust.OperatorPrincipal) {
	if strings.HasPrefix(request.URL.Path, "/manage/api/authority/") {
		AuthorityHandler{Trust:handler.Trust}.ServeAdmin(writer, request)
		return
	}
	if request.URL.Path == "/manage/api/state" && request.Method == http.MethodGet {
		result := handler.Management.State(operator)
		if result.Code == "" {
			result.Value.(map[string]any)["csrf"] = current.CSRF
		}
		writeResult(writer, result)
		return
	}
	if request.Method != http.MethodPost {
		if request.URL.Path == "/manage/api/pair/approve" || request.URL.Path == "/manage/api/pair/reject" {
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
		dispatch(writer,request,func(in pairing.ApprovalRequest)operation.Result{return handler.Pairing.Approve(request.Context(),operator,in)})
	case "/manage/api/pair/reject":
		dispatch(writer,request,func(in pairing.RejectionRequest)operation.Result{return handler.Pairing.Reject(request.Context(),operator,in)})
	case "/manage/api/route":
		dispatch(writer, request, handler.Management.Route)
	case "/manage/api/target":
		dispatch(writer, request, handler.Management.Target)
	case "/manage/api/provider":
		dispatch(writer, request, handler.Management.Provider)
	case "/manage/api/test":
		dispatch(writer, request, func(input TestRequest) operation.Result { return handler.Management.Test(request.Context(),operator,input) })
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

func decode(writer http.ResponseWriter, request *http.Request, output any) bool {
	mediaType, _, err := mime.ParseMediaType(request.Header.Get("Content-Type"))
	if err != nil || mediaType != "application/json" {
		return false
	}
 data,err:=io.ReadAll(http.MaxBytesReader(writer,request.Body,16384));if err!=nil||!StrictJSON(data){return false};return trust.DecodeStrict(data,output,16384,32)==nil
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
