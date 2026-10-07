//go:build floe_dev

package httptransport

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"floe/server/internal/trust"
)

func TestDevelopmentQADashboardIssuesScopedSessionAndRetainsGuards(t *testing.T) {
	fixture := newConsoleFixture(t)
	fixture.handler.DevelopmentQANoAuth = true
	navigation := map[string]string{
		"Accept":         "text/html,application/xhtml+xml",
		"Sec-Fetch-Mode": "navigate",
		"Sec-Fetch-Dest": "document",
		"Sec-Fetch-Site": "none",
	}
	page := fixture.requestWithHeaders(http.MethodGet, "/manage/", nil, nil, "", "", "", navigation)
	if page.Code != http.StatusOK {
		t.Fatalf("QA dashboard page returned %d: %s", page.Code, page.Body.String())
	}
	if !strings.Contains(page.Body.String(), `name="floe-qa-no-auth" content="true"`) ||
		!strings.Contains(page.Body.String(), "DEVELOPMENT QA MODE · DASHBOARD TOKEN AUTHENTICATION IS DISABLED") ||
		strings.Contains(page.Body.String(), `id="qa-mode-warning" class="warning qa-warning" role="alert" hidden`) {
		t.Fatal("QA dashboard page omitted its mode marker or visible warning")
	}
	cookies := page.Result().Cookies()
	if len(cookies) != 1 {
		t.Fatalf("QA dashboard issued %d cookies, want one", len(cookies))
	}
	qaCookie := cookies[0]
	if qaCookie.Name != "floe_management_qa" || qaCookie.Value == "" || qaCookie.Path != "/manage" || !qaCookie.HttpOnly || qaCookie.SameSite != http.SameSiteStrictMode {
		t.Fatalf("unexpected scoped QA session cookie: %#v", qaCookie)
	}
	session, ok := fixture.trust.OperatorSession(qaCookie.Value)
	if !ok || session.CSRF == "" || !strings.Contains(page.Body.String(), `name="floe-qa-csrf" content="`+session.CSRF+`"`) {
		t.Fatal("QA page did not bind its initial state request to the minted session CSRF capability")
	}

	reloadNavigation := map[string]string{
		"Accept":         "text/html,application/xhtml+xml",
		"Sec-Fetch-Mode": "navigate",
		"Sec-Fetch-Dest": "document",
		"Sec-Fetch-Site": "same-origin",
	}
	reload := fixture.requestWithHeaders(http.MethodGet, "/manage/", nil, qaCookie, "", "", "", reloadNavigation)
	if reload.Code != http.StatusOK || len(reload.Result().Cookies()) != 0 || !strings.Contains(reload.Body.String(), `name="floe-qa-csrf" content="`+session.CSRF+`"`) {
		t.Fatalf("same-origin dashboard reload did not reuse its session: status=%d cookies=%#v", reload.Code, reload.Result().Cookies())
	}

	sameOrigin := map[string]string{"Sec-Fetch-Site": "same-origin"}
	stateResponse := fixture.requestWithHeaders(http.MethodGet, "/manage/api/state", nil, qaCookie, "http://"+fixture.address, session.CSRF, "", sameOrigin)
	if stateResponse.Code != http.StatusOK {
		t.Fatalf("tokenless QA state request returned %d: %s", stateResponse.Code, stateResponse.Body.String())
	}
	var state consoleState
	if err := json.Unmarshal(stateResponse.Body.Bytes(), &state); err != nil || !state.QAMode || state.CSRF != session.CSRF {
		t.Fatalf("QA state = %#v, decode error %v", state, err)
	}

	noCSRF := fixture.requestWithHeaders(http.MethodGet, "/manage/api/state", nil, qaCookie, "http://"+fixture.address, "", "", sameOrigin)
	if noCSRF.Code != http.StatusUnauthorized || responseCode(t, noCSRF) != "unauthorized" {
		t.Fatalf("QA state without CSRF returned %d with %q", noCSRF.Code, responseCode(t, noCSRF))
	}
	wrongOrigin := fixture.requestWithHeaders(http.MethodGet, "/manage/api/state", nil, qaCookie, "http://127.0.0.1:18432", session.CSRF, "", sameOrigin)
	if wrongOrigin.Code != http.StatusForbidden || responseCode(t, wrongOrigin) != "invalid_origin" {
		t.Fatalf("QA state from another origin returned %d with %q", wrongOrigin.Code, responseCode(t, wrongOrigin))
	}
	foreignMutation := fixture.requestWithHeaders(http.MethodPost, "/manage/api/logout", map[string]bool{}, qaCookie, "https://evil.example", session.CSRF, "", map[string]string{"Sec-Fetch-Site": "cross-site"})
	if foreignMutation.Code != http.StatusForbidden || responseCode(t, foreignMutation) != "invalid_origin" {
		t.Fatalf("cross-origin QA mutation returned %d with %q", foreignMutation.Code, responseCode(t, foreignMutation))
	}
	stateAfterForeignMutation := fixture.requestWithHeaders(http.MethodGet, "/manage/api/state", nil, qaCookie, "http://"+fixture.address, session.CSRF, "", sameOrigin)
	if stateAfterForeignMutation.Code != http.StatusOK {
		t.Fatalf("cross-origin request invalidated the QA session: %d %s", stateAfterForeignMutation.Code, stateAfterForeignMutation.Body.String())
	}

	crossSite := map[string]string{
		"Accept":         "text/html,application/xhtml+xml",
		"Sec-Fetch-Mode": "navigate",
		"Sec-Fetch-Dest": "document",
		"Sec-Fetch-Site": "cross-site",
	}
	for _, path := range []string{"/", "/manage/"} {
		response := fixture.requestWithHeaders(http.MethodGet, path, nil, nil, "", "", "", crossSite)
		if response.Code != http.StatusForbidden || len(response.Result().Cookies()) != 0 {
			t.Fatalf("cross-site navigation to %s returned %d and cookies %#v", path, response.Code, response.Result().Cookies())
		}
	}
	embedded := map[string]string{
		"Accept":         "text/html,application/xhtml+xml",
		"Sec-Fetch-Mode": "navigate",
		"Sec-Fetch-Dest": "iframe",
		"Sec-Fetch-Site": "cross-site",
	}
	iframe := fixture.requestWithHeaders(http.MethodGet, "/manage/", nil, nil, "", "", "", embedded)
	if iframe.Code != http.StatusForbidden || len(iframe.Result().Cookies()) != 0 {
		t.Fatalf("cross-site iframe received a QA session: status=%d cookies=%#v", iframe.Code, iframe.Result().Cookies())
	}
	otherLocalOrigin := fixture.requestWithHeaders(http.MethodGet, "/manage/", nil, nil, "http://127.0.0.1:18432", "", "", navigation)
	if otherLocalOrigin.Code != http.StatusForbidden || len(otherLocalOrigin.Result().Cookies()) != 0 {
		t.Fatalf("different local origin received a QA session: status=%d cookies=%#v", otherLocalOrigin.Code, otherLocalOrigin.Result().Cookies())
	}

	loginAttempt := fixture.requestWithHeaders(http.MethodPost, "/manage/api/login", map[string]string{"token": fixture.adminToken}, nil, "http://"+fixture.address, "", "", sameOrigin)
	if loginAttempt.Code != http.StatusForbidden || responseCode(t, loginAttempt) != "qa_login_disabled" {
		t.Fatalf("QA login endpoint returned %d with %q", loginAttempt.Code, responseCode(t, loginAttempt))
	}
	clientAPI := fixture.request(http.MethodGet, "/v1/connections", nil, qaCookie, "", "", "")
	if clientAPI.Code != http.StatusUnauthorized {
		t.Fatalf("QA session bypassed paired client authentication: %d %s", clientAPI.Code, clientAPI.Body.String())
	}
	operatorCookie := *qaCookie
	operatorCookie.Name = "floe_management"
	operatorAPI := fixture.request(http.MethodGet, "/v1/traces", nil, &operatorCookie, "", "", "")
	if operatorAPI.Code != http.StatusUnauthorized {
		t.Fatalf("dashboard-only QA session bypassed operator API authentication: %d %s", operatorAPI.Code, operatorAPI.Body.String())
	}

	restartedTrust, err := trust.Open(fixture.trustFiles, false)
	if err != nil {
		t.Fatalf("reopen Trust without restoring process-local sessions: %v", err)
	}
	normalHandler := *fixture.handler
	normalHandler.Trust = restartedTrust
	normalHandler.DevelopmentQANoAuth = false
	normalMode := httptest.NewRequest(http.MethodGet, "/manage/api/state", nil)
	normalMode.Host = fixture.address
	normalMode.AddCookie(&operatorCookie)
	normalResponse := httptest.NewRecorder()
	normalHandler.ServeHTTP(normalResponse, normalMode)
	if normalResponse.Code != http.StatusUnauthorized || responseCode(t, normalResponse) != "unauthorized" {
		t.Fatalf("restarted normal-mode server accepted the old QA session: %d with %q", normalResponse.Code, responseCode(t, normalResponse))
	}
	loginBody, err := json.Marshal(map[string]string{"token": fixture.adminToken})
	if err != nil {
		t.Fatalf("encode synthetic administrator token: %v", err)
	}
	loginRequest := httptest.NewRequest(http.MethodPost, "/manage/api/login", bytes.NewReader(loginBody))
	loginRequest.Host = fixture.address
	loginRequest.Header.Set("Content-Type", "application/json")
	loginRequest.Header.Set("Origin", "http://"+fixture.address)
	loginResponse := httptest.NewRecorder()
	normalHandler.ServeHTTP(loginResponse, loginRequest)
	loginCookies := loginResponse.Result().Cookies()
	if loginResponse.Code != http.StatusOK || len(loginCookies) != 1 || loginCookies[0].Name != "floe_management" {
		t.Fatalf("normal mode did not restore token login: status=%d cookies=%#v body=%s", loginResponse.Code, loginCookies, loginResponse.Body.String())
	}

	logout := fixture.requestWithHeaders(http.MethodPost, "/manage/api/logout", map[string]bool{}, qaCookie, "http://"+fixture.address, session.CSRF, "", sameOrigin)
	if logout.Code != http.StatusOK {
		t.Fatalf("QA session logout returned %d: %s", logout.Code, logout.Body.String())
	}
	afterLogout := fixture.requestWithHeaders(http.MethodGet, "/manage/api/state", nil, qaCookie, "http://"+fixture.address, session.CSRF, "", sameOrigin)
	if afterLogout.Code != http.StatusUnauthorized {
		t.Fatalf("logged-out QA session remained usable: %d %s", afterLogout.Code, afterLogout.Body.String())
	}
}
