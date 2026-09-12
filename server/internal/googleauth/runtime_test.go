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

func TestBindCredentialChangesVaultScopeAndRejectsArbitraryNames(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	runtime, err := New(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	name := credentialName + ":" + strings.Repeat("a", 64)
	if err := runtime.BindCredential(name); err != nil || runtime.credentialKey() != name {
		t.Fatalf("credential scope not bound: %q %v", runtime.credentialKey(), err)
	}
	if err := runtime.BindCredential("other:" + strings.Repeat("a", 64)); err == nil {
		t.Fatal("foreign credential namespace accepted")
	}
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

type countingGoogleStore struct {
	values map[string]string
	reads  int
}

type failingGoogleStore struct{}

func (failingGoogleStore) Get(string) (string, error) { return "", nil }
func (failingGoogleStore) Put(string, string) error   { return errors.New("persist failed") }
func (failingGoogleStore) Delete(string) error        { return errors.New("delete failed") }

type blockedGoogleStore struct {
	values  map[string]string
	blocked string
	started chan struct{}
	release chan struct{}
}

func (store *blockedGoogleStore) Get(name string) (string, error) {
	if name == store.blocked {
		close(store.started)
		<-store.release
	}
	return store.values[name], nil
}

func (store *blockedGoogleStore) Put(name, value string) error {
	store.values[name] = value
	return nil
}

func (store *blockedGoogleStore) Delete(name string) error {
	delete(store.values, name)
	return nil
}

func (store *countingGoogleStore) Get(name string) (string, error) {
	store.reads++
	return store.values[name], nil
}

func (store *countingGoogleStore) Put(name, value string) error {
	store.values[name] = value
	return nil
}

func (store *countingGoogleStore) Delete(name string) error {
	delete(store.values, name)
	return nil
}

func TestProviderIdentityStatusUsesOnlyHydratedCacheAndRejectsMalformedPersistedIdentity(t *testing.T) {
	store := &countingGoogleStore{values: map[string]string{}}
	runtime, err := NewCalendar(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	if identity, verified := runtime.ProviderIdentityStatus(); identity != "" || verified || store.reads != 0 {
		t.Fatalf("cold status performed I/O or trusted state: %q %v reads=%d", identity, verified, store.reads)
	}
	store.values[credentialName] = `{"client_id":"fixture-client","access_token":"access","refresh_token":"refresh","scope":"https://www.googleapis.com/auth/calendar.readonly openid","expires_at":"2027-09-12T00:00:00Z","provider_identity":"google:subject","identity_verified":true,"identity_review_required":false,"schema":1}`
	if _, err := runtime.ProviderIdentity(context.Background()); !errors.Is(err, ErrCredentialExpired) {
		t.Fatalf("unknown persisted schema was accepted: %v", err)
	}
	store.values[credentialName] = `{"client_id":"fixture-client","access_token":"access","refresh_token":"refresh","scope":"https://www.googleapis.com/auth/calendar.readonly openid","expires_at":"2027-09-12T00:00:00Z","provider_identity":"same@example.com","identity_verified":true,"identity_review_required":false}`
	if _, err := runtime.ProviderIdentity(context.Background()); !errors.Is(err, ErrCredentialExpired) {
		t.Fatal("email persisted as trusted provider identity")
	}
}

func TestVerifiedProviderIdentityFenceSerializesTokenPublication(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	runtime, err := NewCalendar(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	runtime.mu.Lock()
	runtime.tokens = &tokenBundle{ClientID: "fixture-client", AccessToken: "access", RefreshToken: "refresh", Scope: calendarReadonlyScope + " " + openidScope, ProviderIdentity: "google:subject", IdentityVerified: true, ExpiresAt: time.Now().Add(time.Hour)}
	runtime.mu.Unlock()
	if err := runtime.WithVerifiedProviderIdentity("FLOE_GOOGLE_CALENDAR_OAUTH:other-connection", "google:subject", func() error { return nil }); !errors.Is(err, ErrCredentialExpired) {
		t.Fatalf("identity borrowed across credential binding: %v", err)
	}
	entered := make(chan struct{})
	release := make(chan struct{})
	fenceDone := make(chan error, 1)
	go func() {
		fenceDone <- runtime.WithVerifiedProviderIdentity("FLOE_GOOGLE_CALENDAR_OAUTH", "google:subject", func() error {
			close(entered)
			<-release
			return nil
		})
	}()
	<-entered
	saveDone := make(chan error, 1)
	go func() {
		updated := *runtime.tokens
		updated.ProviderIdentity = "google:subject"
		saveDone <- runtime.save(&updated)
	}()
	select {
	case saveErr := <-saveDone:
		t.Fatalf("identity publication crossed active identity fence: %v", saveErr)
	case <-time.After(25 * time.Millisecond):
	}
	close(release)
	if err := <-fenceDone; err != nil {
		t.Fatal(err)
	}
	if err := <-saveDone; err != nil {
		t.Fatalf("identity publication after fence: %v", err)
	}
}

func TestVerifiedProviderIdentityFenceDoesNotWaitForTokenHTTP(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	runtime, err := NewCalendar(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	runtime.allowTestEndpoints = true
	requestEntered := make(chan struct{})
	releaseHTTP := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/token":
			close(requestEntered)
			<-releaseHTTP
			_, _ = io.WriteString(writer, `{"access_token":"new-access","refresh_token":"refresh","scope":"https://www.googleapis.com/auth/calendar.readonly openid","expires_in":3600,"token_type":"Bearer"}`)
		case "/userinfo":
			_, _ = io.WriteString(writer, `{"sub":"subject"}`)
		default:
			writer.WriteHeader(http.StatusNotFound)
		}
	}))
	defer server.Close()
	runtime.tokenURL, runtime.userinfoURL = server.URL+"/token", server.URL+"/userinfo"
	runtime.mu.Lock()
	runtime.tokens = &tokenBundle{ClientID: "fixture-client", AccessToken: "old-access", RefreshToken: "refresh", Scope: calendarReadonlyScope + " " + openidScope, ProviderIdentity: "google:subject", IdentityVerified: true, ExpiresAt: time.Now().Add(-time.Minute)}
	runtime.mu.Unlock()
	tokenDone := make(chan error, 1)
	go func() {
		_, tokenErr := runtime.Token(context.Background())
		tokenDone <- tokenErr
	}()
	<-requestEntered
	fenceDone := make(chan error, 1)
	go func() {
		fenceDone <- runtime.WithVerifiedProviderIdentity("FLOE_GOOGLE_CALENDAR_OAUTH", "google:subject", func() error { return nil })
	}()
	select {
	case fenceErr := <-fenceDone:
		if fenceErr != nil {
			t.Fatalf("cached source fence failed while token HTTP was pending: %v", fenceErr)
		}
	case <-time.After(250 * time.Millisecond):
		t.Fatal("cached source fence waited for token HTTP")
	}
	close(releaseHTTP)
	if tokenErr := <-tokenDone; tokenErr != nil {
		t.Fatalf("token refresh after HTTP release: %v", tokenErr)
	}
}

func TestIdentityPersistenceFailurePublishesCachedDeny(t *testing.T) {
	runtime, err := NewCalendar(failingGoogleStore{}, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	runtime.mu.Lock()
	runtime.tokens = &tokenBundle{ClientID: "fixture-client", AccessToken: "access", RefreshToken: "refresh", Scope: calendarReadonlyScope + " " + openidScope, ProviderIdentity: "google:subject", IdentityVerified: true, ExpiresAt: time.Now().Add(time.Hour)}
	runtime.mu.Unlock()
	updated := *runtime.tokens
	updated.IdentityVerified = false
	if err := runtime.save(&updated); err == nil {
		t.Fatal("persistence failure reported success")
	}
	if identity, verified := runtime.ProviderIdentityStatus(); identity != "google:subject" || verified {
		t.Fatalf("cached identity remained trusted after persistence failure: %q %v", identity, verified)
	}
}

func TestGoogleLogoutStoreFailureLeavesCachedIdentityDenied(t *testing.T) {
	runtime, err := NewCalendar(failingGoogleStore{}, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	revokeServer := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.WriteHeader(http.StatusOK)
	}))
	defer revokeServer.Close()
	runtime.revokeURL = revokeServer.URL
	runtime.mu.Lock()
	runtime.tokens = &tokenBundle{ClientID: "fixture-client", AccessToken: "access", RefreshToken: "refresh", Scope: calendarReadonlyScope + " " + openidScope, ProviderIdentity: "google:subject", IdentityVerified: true, ExpiresAt: time.Now().Add(time.Hour)}
	runtime.mu.Unlock()
	if err := runtime.logout(context.Background()); err == nil {
		t.Fatal("logout persistence failure reported success")
	}
	if identity, verified := runtime.ProviderIdentityStatus(); identity != "google:subject" || verified {
		t.Fatalf("logout persistence failure left identity trusted: %q %v", identity, verified)
	}
}

func TestGoogleLateCredentialLoadCannotPublishAfterRebind(t *testing.T) {
	firstCredential := "FLOE_GOOGLE_CALENDAR_OAUTH:" + strings.Repeat("a", 64)
	secondCredential := "FLOE_GOOGLE_CALENDAR_OAUTH:" + strings.Repeat("b", 64)
	encoded, _ := json.Marshal(tokenBundle{ClientID: "fixture-client", AccessToken: "access", RefreshToken: "refresh", Scope: calendarReadonlyScope + " " + openidScope, ProviderIdentity: "google:subject", IdentityVerified: true, ExpiresAt: time.Now().Add(time.Hour)})
	store := &blockedGoogleStore{values: map[string]string{firstCredential: string(encoded)}, blocked: firstCredential, started: make(chan struct{}), release: make(chan struct{})}
	runtime, err := NewCalendar(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	if err := runtime.BindCredential(firstCredential); err != nil {
		t.Fatal(err)
	}
	readyDone := make(chan bool, 1)
	go func() { readyDone <- runtime.Ready() }()
	<-store.started
	boundDone := make(chan error, 1)
	go func() { boundDone <- runtime.BindCredential(secondCredential) }()
	if err := <-boundDone; err != nil {
		t.Fatal(err)
	}
	close(store.release)
	if <-readyDone {
		t.Fatal("late credential load published under replacement binding")
	}
	if runtime.credentialKey() != secondCredential {
		t.Fatalf("credential binding reverted: %q", runtime.credentialKey())
	}
}

func TestPersistedVerifiedIdentityHydratesDeniedUntilProviderPreflight(t *testing.T) {
	value := tokenBundle{ClientID: "fixture-client", AccessToken: "access", RefreshToken: "refresh", Scope: calendarReadonlyScope + " " + openidScope, ProviderIdentity: "google:subject", IdentityVerified: true, ExpiresAt: time.Now().Add(time.Hour)}
	encoded, _ := json.Marshal(value)
	store := &memoryStore{values: map[string]string{"FLOE_GOOGLE_CALENDAR_OAUTH": string(encoded)}}
	runtime, err := NewCalendar(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	hydrated := runtime.load()
	if hydrated == nil || hydrated.ProviderIdentity != "google:subject" || hydrated.IdentityVerified {
		t.Fatalf("persisted identity was trusted before preflight: %+v", hydrated)
	}
	if identity, verified := runtime.ProviderIdentityStatus(); identity != "google:subject" || verified {
		t.Fatalf("hydrated identity status was trusted: %q %v", identity, verified)
	}
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
	if auth.Query().Get("scope") != calendarReadonlyScope+" "+openidScope || runtime.credentialName != "FLOE_GOOGLE_CALENDAR_OAUTH" {
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

func TestCalendarProviderIdentityUsesUserInfoAndRefreshesSameSubject(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	subject := "subject-a"
	refreshes := 0
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/userinfo":
			writer.Header().Set("Content-Type", "application/json")
			_, _ = io.WriteString(writer, `{"sub":"`+subject+`","email":"same@example.com"}`)
		case "/token":
			refreshes++
			writer.Header().Set("Content-Type", "application/json")
			_, _ = io.WriteString(writer, `{"access_token":"access-`+string(rune('a'+refreshes))+`","expires_in":3600,"scope":"https://www.googleapis.com/auth/calendar.readonly openid","token_type":"Bearer"}`)
		default:
			writer.WriteHeader(http.StatusNotFound)
		}
	}))
	defer server.Close()
	runtime, err := NewCalendar(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	runtime.userinfoURL, runtime.tokenURL = server.URL+"/userinfo", server.URL+"/token"
	runtime.allowTestEndpoints = true
	value := tokenBundle{ClientID: "fixture-client", AccessToken: "old-access", RefreshToken: "refresh", Scope: calendarReadonlyScope + " " + openidScope, ExpiresAt: time.Now().Add(-time.Minute)}
	encoded, _ := json.Marshal(value)
	store.values["FLOE_GOOGLE_CALENDAR_OAUTH"] = string(encoded)
	identity, err := runtime.ProviderIdentity(context.Background())
	if err != nil || identity != "google:subject-a" {
		t.Fatalf("identity=%q err=%v", identity, err)
	}
	if _, err := runtime.Token(context.Background()); err != nil {
		t.Fatalf("same-subject refresh: %v", err)
	}
	if refreshes != 1 {
		t.Fatalf("refreshes=%d", refreshes)
	}
}

func TestCalendarProviderIdentityRejectsSubjectReplacement(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	subject := "subject-a"
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/userinfo":
			writer.Header().Set("Content-Type", "application/json")
			_, _ = io.WriteString(writer, `{"sub":"`+subject+`","email":"same@example.com"}`)
		case "/token":
			writer.Header().Set("Content-Type", "application/json")
			_, _ = io.WriteString(writer, `{"access_token":"new-access","expires_in":3600,"scope":"https://www.googleapis.com/auth/calendar.readonly openid","token_type":"Bearer"}`)
		}
	}))
	defer server.Close()
	runtime, _ := NewCalendar(store, Config{ClientID: "fixture-client"})
	runtime.userinfoURL, runtime.tokenURL = server.URL+"/userinfo", server.URL+"/token"
	runtime.allowTestEndpoints = true
	value := tokenBundle{ClientID: "fixture-client", AccessToken: "old-access", RefreshToken: "refresh", Scope: calendarReadonlyScope + " " + openidScope, ProviderIdentity: "google:subject-a", IdentityVerified: true, ExpiresAt: time.Now().Add(-time.Minute)}
	encoded, _ := json.Marshal(value)
	store.values["FLOE_GOOGLE_CALENDAR_OAUTH"] = string(encoded)
	if _, err := runtime.ProviderIdentity(context.Background()); err != nil {
		t.Fatal(err)
	}
	subject = "subject-b"
	if _, err := runtime.Token(context.Background()); !errors.Is(err, ErrCredentialExpired) {
		t.Fatalf("subject replacement error=%v", err)
	}
	current := runtime.load()
	if current == nil || current.IdentityVerified || current.ProviderIdentity != "google:subject-b" {
		t.Fatalf("replacement was not quarantined: %+v", current)
	}
}

func TestCalendarUserInfoRejectsDuplicateSubject(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(writer, `{"sub":"subject-a","sub":"subject-b"}`)
	}))
	defer server.Close()
	runtime, _ := NewCalendar(store, Config{ClientID: "fixture-client"})
	runtime.userinfoURL = server.URL
	runtime.allowTestEndpoints = true
	value := tokenBundle{ClientID: "fixture-client", AccessToken: "access", RefreshToken: "refresh", Scope: calendarReadonlyScope + " " + openidScope, ExpiresAt: time.Now().Add(time.Hour)}
	encoded, _ := json.Marshal(value)
	store.values["FLOE_GOOGLE_CALENDAR_OAUTH"] = string(encoded)
	if _, err := runtime.ProviderIdentity(context.Background()); !errors.Is(err, ErrCredentialExpired) {
		t.Fatalf("duplicate identity accepted: %v", err)
	}
}

func TestCalendarUserInfoHonorsCallerDeadline(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		<-request.Context().Done()
	}))
	defer server.Close()
	runtime, _ := NewCalendar(store, Config{ClientID: "fixture-client"})
	runtime.userinfoURL = server.URL
	runtime.allowTestEndpoints = true
	value := tokenBundle{ClientID: "fixture-client", AccessToken: "access", RefreshToken: "refresh", Scope: calendarReadonlyScope + " " + openidScope, ExpiresAt: time.Now().Add(time.Hour)}
	encoded, _ := json.Marshal(value)
	store.values["FLOE_GOOGLE_CALENDAR_OAUTH"] = string(encoded)
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Millisecond)
	defer cancel()
	if _, err := runtime.ProviderIdentity(ctx); err == nil {
		t.Fatal("stalled UserInfo request ignored caller deadline")
	}
}
