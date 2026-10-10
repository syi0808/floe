package httptransport

import (
	"context"
	"floe/server/internal/integrations"
	"floe/server/internal/trust"
	"html/template"
	"net/http"
	"strconv"
	"strings"
)

type HostedSetup interface {
	HostedSetup(context.Context, trust.OperatorPrincipal, string) (integrations.SetupPresentation, error)
	BeginHostedSetup(context.Context, trust.OperatorPrincipal, string, string, uint64, map[string]any) (integrations.SetupPresentation, error)
}

var setupTemplate = template.Must(template.New("setup").Parse(`<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Floe connection setup</title>{{if eq .Setup.State "pending"}}<meta http-equiv="refresh" content="5">{{end}}</head><body><main><h1>{{.Setup.ConnectorName}}</h1><p>Connection setup: {{.Setup.State}}</p>{{if eq .Setup.State "awaiting_user"}}<form method="post"><input type="hidden" name="csrf" value="{{.CSRF}}"><input type="hidden" name="expected_revision" value="{{.Setup.Revision}}">{{range .Setup.ScopeFields}}<p><label>{{.Name}} <input name="scope.{{.Name}}" value="{{.Value}}" maxlength="12288"></label></p>{{end}}{{if .Setup.SecretRequired}}<label>Provider access token <input type="password" name="secret" required autocomplete="off" maxlength="4096"></label>{{end}}<button type="submit">Continue setup</button></form>{{end}}{{if .Setup.AuthorizationURL}}<p><a href="{{.Setup.AuthorizationURL}}" target="_blank" rel="noopener noreferrer">Open provider authorization</a></p>{{end}}{{if .Setup.UserCode}}<p>Enter this provider code: <strong>{{.Setup.UserCode}}</strong></p>{{end}}{{if .Setup.ErrorCode}}<p>Setup could not finish: {{.Setup.ErrorCode}}</p>{{end}}{{if eq .Setup.State "connected"}}<p>Connection setup is complete. Return to Floe to review the source.</p>{{end}}<p><a href="/manage/">Gateway dashboard</a></p></main></body></html>`))

func (h *Handler) serveHostedSetup(w http.ResponseWriter, r *http.Request) {
	id := strings.TrimPrefix(r.URL.Path, "/manage/setup/")
	if !trust.ValidID(id) || r.URL.RawQuery != "" {
		failure(w, 404, "not_found")
		return
	}
	if r.Method != http.MethodGet && r.Method != http.MethodPost {
		failure(w, 405, "method_not_allowed")
		return
	}
	cookie, err := r.Cookie("floe_management")
	if err != nil {
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		w.WriteHeader(401)
		_, _ = w.Write([]byte(`<p><a href="/manage/">Unlock the Gateway dashboard</a>, then return to this setup page.</p>`))
		return
	}
	csrf := ""
	secret := ""
	revision := uint64(0)
	scope := map[string]any{}
	if r.Method == http.MethodPost {
		if r.Header.Get("Origin") != "http://"+h.Address {
			failure(w, 403, "invalid_origin")
			return
		}
		r.Body = http.MaxBytesReader(w, r.Body, 32768)
		if r.ParseForm() != nil || len(r.PostForm) > 8 || len(r.PostForm["csrf"]) != 1 || len(r.PostForm["expected_revision"]) != 1 || len(r.PostForm["secret"]) > 1 {
			failure(w, 400, "validation")
			return
		}
		for key := range r.PostForm {
			if key != "csrf" && key != "expected_revision" && key != "secret" && !strings.HasPrefix(key, "scope.") {
				failure(w, 400, "validation")
				return
			}
		}
		for key, values := range r.PostForm {
			if strings.HasPrefix(key, "scope.") {
				if len(values) != 1 {
					failure(w, 400, "validation")
					return
				}
				name := strings.TrimPrefix(key, "scope.")
				value := strings.TrimSpace(values[0])
				if name == "calendar_ids" || name == "entities" {
					items := strings.FieldsFunc(value, func(r rune) bool { return r == ',' || r == '\n' })
					for i := range items {
						items[i] = strings.TrimSpace(items[i])
					}
					scope[name] = items
				} else {
					scope[name] = value
				}
			}
		}
		csrf = r.PostForm.Get("csrf")
		secret = r.PostForm.Get("secret")
		revision, err = strconv.ParseUint(r.PostForm.Get("expected_revision"), 10, 64)
		if err != nil || revision == 0 || revision > trust.MaxJSONInteger {
			failure(w, 400, "validation")
			return
		}
	}
	operator, err := h.Trust.AuthenticateOperatorSession(r.Context(), cookie.Value, csrf, r.Method == http.MethodPost)
	if err != nil {
		failure(w, 401, "unauthorized")
		return
	}
	var view integrations.SetupPresentation
	if r.Method == http.MethodPost {
		view, err = h.Setup.BeginHostedSetup(r.Context(), operator, id, secret, revision, scope)
	} else {
		view, err = h.Setup.HostedSetup(r.Context(), operator, id)
	}
	if err != nil {
		writeOperationError(w, err)
		return
	}
	session, ok := h.Trust.OperatorSession(cookie.Value)
	if !ok {
		failure(w, 401, "unauthorized")
		return
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	_ = setupTemplate.Execute(w, struct {
		Setup integrations.SetupPresentation
		CSRF  string
	}{view, session.CSRF})
}
