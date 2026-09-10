package googleauth

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"sync"
	"testing"
	"time"
)

type memoryStore struct {
	mu     sync.Mutex
	values map[string]string
}

func (store *memoryStore) Get(name string) (string, error) {
	store.mu.Lock()
	defer store.mu.Unlock()
	return store.values[name], nil
}
func (store *memoryStore) Put(name, value string) error {
	store.mu.Lock()
	defer store.mu.Unlock()
	store.values[name] = value
	return nil
}
func (store *memoryStore) Delete(name string) error {
	store.mu.Lock()
	defer store.mu.Unlock()
	delete(store.values, name)
	return nil
}

func TestTokenRefreshRetainsRotatingCredentialAndRequiredScope(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	expired := tokenBundle{ClientID: "fixture-client", AccessToken: "old-access", RefreshToken: "old-refresh", Scope: readonlyScope, ExpiresAt: time.Now().Add(-time.Minute)}
	encoded, _ := json.Marshal(expired)
	store.values[credentialName] = string(encoded)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		body, _ := io.ReadAll(request.Body)
		form, _ := url.ParseQuery(string(body))
		if request.Method != http.MethodPost || form.Get("grant_type") != "refresh_token" || form.Get("refresh_token") != "old-refresh" || form.Get("client_secret") != "fixture-secret" {
			t.Fatalf("invalid refresh: %s %s", request.Method, body)
		}
		writer.Header().Set("Content-Type", "application/json")
		io.WriteString(writer, `{"access_token":"new-access","expires_in":3600,"token_type":"Bearer"}`)
	}))
	defer server.Close()
	runtime, _ := New(store, Config{ClientID: "fixture-client", ClientSecret: "fixture-secret"})
	runtime.tokenURL = server.URL
	token, err := runtime.Token(context.Background())
	if err != nil || token != "new-access" {
		t.Fatalf("token=%q err=%v", token, err)
	}
	if !strings.Contains(store.values[credentialName], "old-refresh") || strings.Contains(store.values[credentialName], "fixture-secret") {
		t.Fatal("credential rotation or secret persistence failed")
	}
}

func TestPKCELoginCallbackPersistsTokensAndLogoutRevokes(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	var redirectURI, verifier, revoked string
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		body, _ := io.ReadAll(request.Body)
		form, _ := url.ParseQuery(string(body))
		switch request.URL.Path {
		case "/token":
			redirectURI, verifier = form.Get("redirect_uri"), form.Get("code_verifier")
			if form.Get("code") != "fixture-code" || len(verifier) < 43 {
				t.Fatalf("bad exchange: %s", body)
			}
			io.WriteString(writer, `{"access_token":"access","refresh_token":"refresh","scope":"https://www.googleapis.com/auth/gmail.readonly","expires_in":3600,"token_type":"Bearer"}`)
		case "/revoke":
			revoked = form.Get("token")
		default:
			t.Fatalf("unexpected endpoint: %s", request.URL.Path)
		}
	}))
	defer server.Close()
	runtime, _ := New(store, Config{ClientID: "fixture-client"})
	runtime.authURL, runtime.tokenURL, runtime.revokeURL = server.URL+"/auth", server.URL+"/token", server.URL+"/revoke"
	defer runtime.Close()
	result, err := runtime.Action(context.Background(), "login")
	if err != nil {
		t.Fatal(err)
	}
	status := result.(map[string]any)
	auth, _ := url.Parse(status["auth_url"].(string))
	if auth.Query().Get("code_challenge_method") != "S256" || auth.Query().Get("scope") != readonlyScope || auth.Query().Get("access_type") != "offline" {
		t.Fatalf("bad auth URL: %s", auth)
	}
	callback := auth.Query().Get("redirect_uri") + "?code=fixture-code&state=" + url.QueryEscape(auth.Query().Get("state"))
	response, err := http.Get(callback)
	if err != nil {
		t.Fatal(err)
	}
	response.Body.Close()
	if response.StatusCode != http.StatusOK || redirectURI != auth.Query().Get("redirect_uri") || verifier == "" || !runtime.Ready() {
		t.Fatalf("callback status=%d redirect=%q verifier=%q", response.StatusCode, redirectURI, verifier)
	}
	if _, err := runtime.Action(context.Background(), "logout"); err != nil {
		t.Fatal(err)
	}
	if revoked != "refresh" || runtime.Ready() || store.values[credentialName] != "" {
		t.Fatalf("revoked=%q ready=%v", revoked, runtime.Ready())
	}
}

func TestRejectsMissingConfigWrongStateAndMissingScope(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	if _, err := New(store, Config{}); err == nil {
		t.Fatal("accepted missing client")
	}
	runtime, _ := New(store, Config{ClientID: "fixture-client"})
	defer runtime.Close()
	result, _ := runtime.Action(context.Background(), "login")
	auth, _ := url.Parse(result.(map[string]any)["auth_url"].(string))
	response, err := http.Get(auth.Query().Get("redirect_uri") + "?code=x&state=wrong")
	if err != nil {
		t.Fatal(err)
	}
	response.Body.Close()
	if response.StatusCode != http.StatusBadRequest || runtime.Ready() {
		t.Fatal("wrong state accepted")
	}

	tokenServer := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		io.WriteString(writer, `{"access_token":"access","refresh_token":"refresh","scope":"openid","expires_in":3600,"token_type":"Bearer"}`)
	}))
	defer tokenServer.Close()
	runtime.tokenURL = tokenServer.URL
	runtime.mu.Lock()
	runtime.tokens = nil
	runtime.mu.Unlock()
	store.values = map[string]string{}
	runtime.cancelLogin()
	result, _ = runtime.Action(context.Background(), "login")
	auth, _ = url.Parse(result.(map[string]any)["auth_url"].(string))
	response, err = http.Get(auth.Query().Get("redirect_uri") + "?code=x&state=" + auth.Query().Get("state"))
	if err != nil {
		t.Fatal(err)
	}
	response.Body.Close()
	if response.StatusCode != http.StatusBadGateway || runtime.Ready() {
		t.Fatal("missing scope accepted")
	}
}

func TestChangedClientAndRejectedRefreshNeverReuseStoredCredential(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	value := tokenBundle{ClientID: "old-client", AccessToken: "access", RefreshToken: "refresh", Scope: readonlyScope, ExpiresAt: time.Now().Add(time.Hour)}
	encoded, _ := json.Marshal(value)
	store.values[credentialName] = string(encoded)
	runtime, _ := New(store, Config{ClientID: "new-client"})
	if runtime.Ready() {
		t.Fatal("changed client inherited old tokens")
	}

	value.ClientID = "new-client"
	value.ExpiresAt = time.Now().Add(-time.Minute)
	encoded, _ = json.Marshal(value)
	store.values[credentialName] = string(encoded)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) { writer.WriteHeader(http.StatusBadRequest) }))
	defer server.Close()
	runtime.tokenURL = server.URL
	if _, err := runtime.Token(context.Background()); !errors.Is(err, ErrCredentialExpired) {
		t.Fatalf("refresh: %v", err)
	}
	if runtime.Ready() || store.values[credentialName] != "" {
		t.Fatal("rejected refresh retained unusable credential")
	}
}

func TestDriveOAuthUsesSeparateCredentialAndExactReadonlyScope(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	runtime, err := NewDrive(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	defer runtime.Close()
	result, err := runtime.Action(context.Background(), "login")
	if err != nil {
		t.Fatal(err)
	}
	auth, _ := url.Parse(result.(map[string]any)["auth_url"].(string))
	if auth.Query().Get("scope") != driveReadonlyScope || runtime.credentialName != "FLOE_DRIVE_OAUTH" {
		t.Fatalf("Drive OAuth profile: %s %s", auth.Query().Get("scope"), runtime.credentialName)
	}
	bundle := tokenBundle{ClientID: "fixture-client", AccessToken: "drive-access", RefreshToken: "drive-refresh", Scope: driveReadonlyScope, ExpiresAt: time.Now().Add(time.Hour)}
	encoded, _ := json.Marshal(bundle)
	store.values["FLOE_DRIVE_OAUTH"] = string(encoded)
	runtime.cancelLogin()
	if !runtime.Ready() {
		t.Fatal("Drive credential was not loaded")
	}
	gmail, _ := New(store, Config{ClientID: "fixture-client"})
	if gmail.Ready() {
		t.Fatal("Drive credential crossed into Gmail runtime")
	}
}

func TestCalendarOAuthUsesSeparateCredentialAndExactReadonlyScope(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	runtime, err := NewCalendar(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	defer runtime.Close()
	result, err := runtime.Action(context.Background(), "login")
	if err != nil {
		t.Fatal(err)
	}
	auth, _ := url.Parse(result.(map[string]any)["auth_url"].(string))
	if auth.Query().Get("scope") != calendarReadonlyScope || runtime.credentialName != "FLOE_GOOGLE_CALENDAR_OAUTH" {
		t.Fatalf("Calendar OAuth profile: %s %s", auth.Query().Get("scope"), runtime.credentialName)
	}
	bundle := tokenBundle{ClientID: "fixture-client", AccessToken: "calendar-access", RefreshToken: "calendar-refresh", Scope: calendarReadonlyScope, ExpiresAt: time.Now().Add(time.Hour)}
	encoded, _ := json.Marshal(bundle)
	store.values["FLOE_GOOGLE_CALENDAR_OAUTH"] = string(encoded)
	runtime.cancelLogin()
	if !runtime.Ready() {
		t.Fatal("Calendar credential was not loaded")
	}
	drive, _ := NewDrive(store, Config{ClientID: "fixture-client"})
	if drive.Ready() {
		t.Fatal("Calendar credential crossed into Drive runtime")
	}
}
