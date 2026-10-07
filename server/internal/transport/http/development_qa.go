package httptransport

import (
	"html"
	"net/http"
	"strings"

	"floe/server/internal/trust"
)

func isDevelopmentQADashboardEntry(request *http.Request) bool {
	return request.Method == http.MethodGet && request.URL.RawQuery == "" && isDevelopmentQADashboardPath(request.URL.Path)
}

func isDevelopmentQADashboardPath(path string) bool {
	switch path {
	case "/", "/manage", "/manage/":
		return true
	default:
		return false
	}
}

func (handler *Handler) prepareDevelopmentQADashboard(writer http.ResponseWriter, request *http.Request) (string, bool) {
	if !developmentQADashboardNavigation(request) {
		failure(writer, http.StatusForbidden, "qa_dashboard_navigation_required")
		return "", false
	}
	if cookie, err := request.Cookie(handler.managementCookieName()); err == nil {
		if session, ok := handler.Trust.OperatorSession(cookie.Value); ok {
			return session.CSRF, true
		}
	}
	token, session, err := createDevelopmentQASession(request.Context(), handler.Trust)
	if err != nil {
		writeResult(writer, trust.Result(err))
		return "", false
	}
	http.SetCookie(writer, &http.Cookie{
		Name: handler.managementCookieName(), Value: token, Path: "/manage", HttpOnly: true,
		SameSite: http.SameSiteStrictMode, MaxAge: 12 * 60 * 60,
	})
	return session.CSRF, true
}

func (handler *Handler) allowDevelopmentQAManagementRequest(writer http.ResponseWriter, request *http.Request) bool {
	path := request.URL.Path
	if path == "/manage/api/login" {
		if request.Header.Get("Sec-Fetch-Site") != "same-origin" {
			failure(writer, http.StatusForbidden, "invalid_origin")
			return false
		}
		return true
	}
	if strings.HasPrefix(path, "/manage/api/") {
		if request.Header.Get("Sec-Fetch-Site") != "same-origin" {
			failure(writer, http.StatusForbidden, "invalid_origin")
			return false
		}
		cookie, err := request.Cookie(handler.managementCookieName())
		if err != nil {
			failure(writer, http.StatusUnauthorized, "unauthorized")
			return false
		}
		if _, err := handler.Trust.AuthenticateDashboardOperatorSession(request.Context(), cookie.Value, request.Header.Get("X-Floe-CSRF"), true); err != nil {
			writeResult(writer, trust.Result(err))
			return false
		}
		return true
	}
	if strings.HasPrefix(path, "/manage/setup/") && request.Method == http.MethodGet {
		if !developmentQADashboardNavigation(request) {
			failure(writer, http.StatusForbidden, "qa_dashboard_navigation_required")
			return false
		}
		return true
	}
	if request.Header.Get("Sec-Fetch-Site") != "same-origin" {
		failure(writer, http.StatusForbidden, "invalid_origin")
		return false
	}
	return true
}

func developmentQADashboardNavigation(request *http.Request) bool {
	site := request.Header.Get("Sec-Fetch-Site")
	return (site == "none" || site == "same-origin") &&
		request.Header.Get("Sec-Fetch-Mode") == "navigate" &&
		request.Header.Get("Sec-Fetch-Dest") == "document" &&
		strings.Contains(strings.ToLower(request.Header.Get("Accept")), "text/html")
}

func developmentQADashboardHTML(data []byte, csrf string) []byte {
	page := string(data)
	page = strings.Replace(page, `name="floe-qa-no-auth" content="false"`, `name="floe-qa-no-auth" content="true"`, 1)
	page = strings.Replace(page, `name="floe-qa-csrf" content=""`, `name="floe-qa-csrf" content="`+html.EscapeString(csrf)+`"`, 1)
	page = strings.Replace(page, `id="qa-mode-warning" class="warning qa-warning" role="alert" hidden`, `id="qa-mode-warning" class="warning qa-warning" role="alert"`, 1)
	return []byte(page)
}
