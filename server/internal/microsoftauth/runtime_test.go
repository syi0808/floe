package microsoftauth

import (
	"context"
	"crypto"
	"crypto/rand"
	"crypto/rsa"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strconv"
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
	name := credentialName + ":" + strings.Repeat("b", 64)
	if err := runtime.BindCredential(name); err != nil || runtime.credentialKey() != name {
		t.Fatalf("credential scope not bound: %q %v", runtime.credentialKey(), err)
	}
	if err := runtime.BindCredential("other:" + strings.Repeat("b", 64)); err == nil {
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

type countingMicrosoftStore struct {
	values map[string]string
	reads  int
}

type failingMicrosoftStore struct{}

func (failingMicrosoftStore) Get(string) (string, error) { return "", nil }
func (failingMicrosoftStore) Put(string, string) error   { return errors.New("persist failed") }
func (failingMicrosoftStore) Delete(string) error        { return errors.New("delete failed") }

type blockedMicrosoftStore struct {
	values  map[string]string
	blocked string
	started chan struct{}
	release chan struct{}
}

func (store *blockedMicrosoftStore) Get(name string) (string, error) {
	if name == store.blocked {
		close(store.started)
		<-store.release
	}
	return store.values[name], nil
}

func (store *blockedMicrosoftStore) Put(name, value string) error {
	store.values[name] = value
	return nil
}

func (store *blockedMicrosoftStore) Delete(name string) error {
	delete(store.values, name)
	return nil
}

func (store *countingMicrosoftStore) Get(name string) (string, error) {
	store.reads++
	return store.values[name], nil
}

func (store *countingMicrosoftStore) Put(name, value string) error {
	store.values[name] = value
	return nil
}

func (store *countingMicrosoftStore) Delete(name string) error {
	delete(store.values, name)
	return nil
}

func TestProviderIdentityStatusUsesOnlyHydratedCacheAndRejectsMalformedPersistedIdentity(t *testing.T) {
	store := &countingMicrosoftStore{values: map[string]string{}}
	runtime, err := NewCalendar(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	if identity, verified := runtime.ProviderIdentityStatus(); identity != "" || verified || store.reads != 0 {
		t.Fatalf("cold status performed I/O or trusted state: %q %v reads=%d", identity, verified, store.reads)
	}
	store.values[calendarCredentialName] = `{"client_id":"fixture-client","access_token":"access","refresh_token":"refresh","scope":"openid profile offline_access Calendars.Read","expires_at":"2027-09-12T00:00:00Z","provider_identity":"microsoft:00000000-0000-4000-8000-000000000001:subject","identity_verified":true,"identity_review_required":false,"schema":1}`
	if _, err := runtime.ProviderIdentity(context.Background()); !errors.Is(err, ErrCredentialExpired) {
		t.Fatalf("unknown persisted schema was accepted: %v", err)
	}
	store.values[calendarCredentialName] = `{"client_id":"fixture-client","access_token":"access","refresh_token":"refresh","scope":"openid profile offline_access Calendars.Read","expires_at":"2027-09-12T00:00:00Z","provider_identity":"microsoft:tenant:email@example.com","identity_verified":true,"identity_review_required":false}`
	if _, err := runtime.ProviderIdentity(context.Background()); !errors.Is(err, ErrCredentialExpired) {
		t.Fatal("malformed Microsoft provider identity was trusted")
	}
}

func TestMicrosoftVerifiedIdentityFenceSerializesPublicationAndCredentialBinding(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	runtime, err := NewCalendar(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	runtime.mu.Lock()
	runtime.tokens = &tokenBundle{ClientID: "fixture-client", AccessToken: "access", RefreshToken: "refresh", Scope: "openid profile offline_access Calendars.Read", ProviderIdentity: "microsoft:00000000-0000-4000-8000-000000000001:subject", IdentityVerified: true, ExpiresAt: time.Now().Add(time.Hour)}
	runtime.mu.Unlock()
	if err := runtime.WithVerifiedProviderIdentity(calendarCredentialName+":other-connection", "microsoft:00000000-0000-4000-8000-000000000001:subject", func() error { return nil }); !errors.Is(err, ErrCredentialExpired) {
		t.Fatalf("identity borrowed across credential binding: %v", err)
	}
	entered := make(chan struct{})
	release := make(chan struct{})
	fenceDone := make(chan error, 1)
	go func() {
		fenceDone <- runtime.WithVerifiedProviderIdentity(calendarCredentialName, "microsoft:00000000-0000-4000-8000-000000000001:subject", func() error {
			close(entered)
			<-release
			return nil
		})
	}()
	<-entered
	saveDone := make(chan error, 1)
	go func() {
		updated := *runtime.tokens
		updated.IdentityVerified = false
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

func TestMicrosoftIdentityPersistenceFailurePublishesCachedDeny(t *testing.T) {
	runtime, err := NewCalendar(failingMicrosoftStore{}, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	runtime.mu.Lock()
	runtime.tokens = &tokenBundle{ClientID: "fixture-client", AccessToken: "access", RefreshToken: "refresh", Scope: "openid profile offline_access Calendars.Read", ProviderIdentity: "microsoft:00000000-0000-4000-8000-000000000001:subject", IdentityVerified: true, ExpiresAt: time.Now().Add(time.Hour)}
	runtime.mu.Unlock()
	updated := *runtime.tokens
	updated.IdentityVerified = false
	if err := runtime.save(&updated); err == nil {
		t.Fatal("persistence failure reported success")
	}
	if identity, verified := runtime.ProviderIdentityStatus(); identity == "" || verified {
		t.Fatalf("cached identity remained trusted after persistence failure: %q %v", identity, verified)
	}
}

func TestMicrosoftLogoutStoreFailureLeavesCachedIdentityDenied(t *testing.T) {
	runtime, err := NewCalendar(failingMicrosoftStore{}, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	runtime.mu.Lock()
	runtime.tokens = &tokenBundle{ClientID: "fixture-client", AccessToken: "access", RefreshToken: "refresh", Scope: "openid profile offline_access Calendars.Read", ProviderIdentity: "microsoft:00000000-0000-4000-8000-000000000001:subject", IdentityVerified: true, ExpiresAt: time.Now().Add(time.Hour)}
	runtime.mu.Unlock()
	if _, err := runtime.Action(context.Background(), "logout"); err == nil {
		t.Fatal("logout persistence failure reported success")
	}
	if identity, verified := runtime.ProviderIdentityStatus(); identity == "" || verified {
		t.Fatalf("logout persistence failure left identity trusted: %q %v", identity, verified)
	}
}

func TestMicrosoftLateCredentialLoadCannotPublishAfterRebind(t *testing.T) {
	firstCredential := calendarCredentialName + ":" + strings.Repeat("a", 64)
	secondCredential := calendarCredentialName + ":" + strings.Repeat("b", 64)
	encoded, _ := json.Marshal(tokenBundle{ClientID: "fixture-client", AccessToken: "access", RefreshToken: "refresh", Scope: "openid profile offline_access Calendars.Read", ProviderIdentity: "microsoft:00000000-0000-4000-8000-000000000001:subject", IdentityVerified: true, ExpiresAt: time.Now().Add(time.Hour)})
	store := &blockedMicrosoftStore{values: map[string]string{firstCredential: string(encoded)}, blocked: firstCredential, started: make(chan struct{}), release: make(chan struct{})}
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
	value := tokenBundle{ClientID: "fixture-client", AccessToken: "access", RefreshToken: "refresh", Scope: "openid profile offline_access Calendars.Read", ProviderIdentity: "microsoft:00000000-0000-4000-8000-000000000001:subject", IdentityVerified: true, ExpiresAt: time.Now().Add(time.Hour)}
	encoded, _ := json.Marshal(value)
	store := &memoryStore{values: map[string]string{calendarCredentialName: string(encoded)}}
	runtime, err := NewCalendar(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	hydrated := runtime.load()
	if hydrated == nil || hydrated.ProviderIdentity == "" || hydrated.IdentityVerified {
		t.Fatalf("persisted identity was trusted before preflight: %+v", hydrated)
	}
	if identity, verified := runtime.ProviderIdentityStatus(); identity == "" || verified {
		t.Fatalf("hydrated identity status was trusted: %q %v", identity, verified)
	}
}

func TestTokenRefreshRetainsRotatingCredentialAndExactScope(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	expired := tokenBundle{ClientID: "fixture-client", AccessToken: "old-access", RefreshToken: "old-refresh", Scope: mailReadScope, ExpiresAt: time.Now().Add(-time.Minute)}
	encoded, _ := json.Marshal(expired)
	store.values[credentialName] = string(encoded)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		body, _ := io.ReadAll(request.Body)
		form, _ := url.ParseQuery(string(body))
		if request.Method != http.MethodPost || form.Get("grant_type") != "refresh_token" || form.Get("refresh_token") != "old-refresh" || form.Get("scope") != "offline_access Mail.Read" || form.Get("client_secret") != "fixture-secret" {
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

func TestPKCELoginPersistsCredentialAndLogoutDeletesLocally(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	var redirectURI, verifier string
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		body, _ := io.ReadAll(request.Body)
		form, _ := url.ParseQuery(string(body))
		redirectURI, verifier = form.Get("redirect_uri"), form.Get("code_verifier")
		if request.URL.Path != "/token" || form.Get("code") != "fixture-code" || form.Get("scope") != "offline_access Mail.Read" || len(verifier) < 43 {
			t.Fatalf("bad exchange: %s %s", request.URL.Path, body)
		}
		io.WriteString(writer, `{"access_token":"access-token","refresh_token":"refresh-token","scope":"openid offline_access Mail.Read","expires_in":3600,"token_type":"Bearer"}`)
	}))
	defer server.Close()
	runtime, _ := New(store, Config{ClientID: "fixture-client"})
	runtime.authURL, runtime.tokenURL = server.URL+"/auth", server.URL+"/token"
	defer runtime.Close()
	result, err := runtime.Action(context.Background(), "login")
	if err != nil {
		t.Fatal(err)
	}
	status := result.(map[string]any)
	auth, _ := url.Parse(status["auth_url"].(string))
	if auth.Query().Get("code_challenge_method") != "S256" || auth.Query().Get("scope") != "offline_access Mail.Read" || auth.Query().Get("response_mode") != "query" {
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
	if runtime.Ready() || store.values[credentialName] != "" {
		t.Fatal("logout retained local credential")
	}
}

func TestRejectsMissingConfigWrongStateAndMissingScope(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	if _, err := New(store, Config{}); !errors.Is(err, ErrUnavailable) {
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
		io.WriteString(writer, `{"access_token":"access-token","refresh_token":"refresh-token","scope":"openid offline_access","expires_in":3600,"token_type":"Bearer"}`)
	}))
	defer tokenServer.Close()
	runtime.tokenURL = tokenServer.URL
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
	value := tokenBundle{ClientID: "old-client", AccessToken: "access-token", RefreshToken: "refresh-token", Scope: mailReadScope, ExpiresAt: time.Now().Add(time.Hour)}
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
	if auth.Query().Get("scope") != "offline_access Calendars.Read openid profile" || runtime.credentialName != calendarCredentialName {
		t.Fatalf("Calendar OAuth profile: %s %s", auth.Query().Get("scope"), runtime.credentialName)
	}
	bundle := tokenBundle{ClientID: "fixture-client", AccessToken: "calendar-access", RefreshToken: "calendar-refresh", Scope: calendarReadScope, ExpiresAt: time.Now().Add(time.Hour)}
	encoded, _ := json.Marshal(bundle)
	store.values[calendarCredentialName] = string(encoded)
	runtime.cancelLogin()
	if !runtime.Ready() {
		t.Fatal("Calendar credential was not loaded")
	}
	mail, _ := New(store, Config{ClientID: "fixture-client"})
	if mail.Ready() {
		t.Fatal("Calendar credential crossed into Mail runtime")
	}
}

func TestTeamsOAuthUsesSeparateCredentialAndExactReadonlyScope(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	runtime, err := NewTeams(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	defer runtime.Close()
	result, err := runtime.Action(context.Background(), "login")
	if err != nil {
		t.Fatal(err)
	}
	auth, _ := url.Parse(result.(map[string]any)["auth_url"].(string))
	if auth.Query().Get("scope") != "offline_access ChannelMessage.Read.All" || runtime.credentialName != teamsCredentialName {
		t.Fatalf("Teams OAuth profile: %s %s", auth.Query().Get("scope"), runtime.credentialName)
	}
	bundle := tokenBundle{ClientID: "fixture-client", AccessToken: "teams-access", RefreshToken: "teams-refresh", Scope: teamsReadScope, ExpiresAt: time.Now().Add(time.Hour)}
	encoded, _ := json.Marshal(bundle)
	store.values[teamsCredentialName] = string(encoded)
	runtime.cancelLogin()
	if !runtime.Ready() {
		t.Fatal("Teams credential was not loaded")
	}
	mail, _ := New(store, Config{ClientID: "fixture-client"})
	calendar, _ := NewCalendar(store, Config{ClientID: "fixture-client"})
	if mail.Ready() || calendar.Ready() {
		t.Fatal("Teams credential crossed into another Microsoft runtime")
	}
}

func TestCalendarProviderIdentityValidatesInitialAndRefreshIDTokens(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	privateKey, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	identitySubject := "subject-a"
	refreshes := 0
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/metadata":
			_, _ = io.WriteString(writer, `{"issuer":"https://login.microsoftonline.com/common/v2.0","jwks_uri":"`+serverURLPlaceholder+`/keys"}`)
		case "/keys":
			modulus, exponent := rsaJWK(privateKey)
			_, _ = io.WriteString(writer, `{"keys":[{"kty":"RSA","use":"sig","kid":"fixture-key","n":"`+modulus+`","e":"`+exponent+`","issuer":"https://login.microsoftonline.com/{tenantid}/v2.0","cloud_instance_name":"microsoftonline.com","x5t":"fixture-thumbprint","x5c":["fixture-certificate"]}]}`)
		case "/token":
			refreshes++
			claims := identitySubject
			nonce := ""
			if refreshes == 1 {
				claims = identitySubject
				nonce = "initial-nonce"
			}
			token := signedMicrosoftIDToken(t, privateKey, "00000000-0000-4000-8000-000000000001", claims, nonce)
			writer.Header().Set("Content-Type", "application/json")
			_, _ = io.WriteString(writer, `{"access_token":"access-`+strconv.Itoa(refreshes)+`","refresh_token":"refresh","scope":"openid profile offline_access Calendars.Read","id_token":"`+token+`","expires_in":3600,"token_type":"Bearer"}`)
		}
	}))
	defer server.Close()
	serverURLPlaceholder = server.URL
	runtime, err := NewCalendar(store, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	runtime.allowTestEndpoints = true
	runtime.metadataURL = server.URL + "/metadata"
	runtime.tokenURL = server.URL + "/token"
	initial, err := runtime.tokenRequest(context.Background(), url.Values{}, nil, "initial-nonce")
	if err != nil {
		t.Fatalf("initial token: %v", err)
	}
	if initial.ProviderIdentity != "microsoft:00000000-0000-4000-8000-000000000001:subject-a" || !initial.IdentityVerified {
		t.Fatalf("initial identity=%q verified=%v", initial.ProviderIdentity, initial.IdentityVerified)
	}
	initial.ExpiresAt = time.Now().Add(-time.Minute)
	encoded, _ := json.Marshal(initial)
	store.values[calendarCredentialName] = string(encoded)
	if _, err := runtime.Token(context.Background()); err != nil {
		t.Fatalf("refresh identity: %v", err)
	}
}

func TestCalendarProviderIdentityQuarantinesChangedRefreshSubject(t *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	privateKey, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	serverURLPlaceholder = ""
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/metadata":
			_, _ = io.WriteString(writer, `{"issuer":"https://login.microsoftonline.com/common/v2.0","jwks_uri":"`+serverURLPlaceholder+`/keys"}`)
		case "/keys":
			modulus, exponent := rsaJWK(privateKey)
			_, _ = io.WriteString(writer, `{"keys":[{"kty":"RSA","use":"sig","kid":"fixture-key","n":"`+modulus+`","e":"`+exponent+`","issuer":"https://login.microsoftonline.com/{tenantid}/v2.0","cloud_instance_name":"microsoftonline.com","x5t":"fixture-thumbprint","x5c":["fixture-certificate"]}]}`)
		case "/token":
			token := signedMicrosoftIDToken(t, privateKey, "00000000-0000-4000-8000-000000000001", "subject-b", "")
			_, _ = io.WriteString(writer, `{"access_token":"new-access","refresh_token":"refresh","scope":"openid profile offline_access Calendars.Read","id_token":"`+token+`","expires_in":3600,"token_type":"Bearer"}`)
		}
	}))
	defer server.Close()
	serverURLPlaceholder = server.URL
	runtime, _ := NewCalendar(store, Config{ClientID: "fixture-client"})
	runtime.allowTestEndpoints = true
	runtime.metadataURL, runtime.tokenURL = server.URL+"/metadata", server.URL+"/token"
	initial := &tokenBundle{ClientID: "fixture-client", AccessToken: "old-access", RefreshToken: "refresh", Scope: "openid profile offline_access Calendars.Read", ProviderIdentity: "microsoft:00000000-0000-4000-8000-000000000001:subject-a", IdentityVerified: true, ExpiresAt: time.Now().Add(-time.Minute)}
	if err := runtime.save(initial); err != nil {
		t.Fatal(err)
	}
	if _, err := runtime.Token(context.Background()); !errors.Is(err, ErrCredentialExpired) {
		t.Fatalf("subject replacement error=%v", err)
	}
	current := runtime.load()
	if current == nil || current.IdentityVerified || current.ProviderIdentity != "microsoft:00000000-0000-4000-8000-000000000001:subject-b" {
		t.Fatalf("replacement was not quarantined: %+v", current)
	}
}

func TestIDTokenSecurityFieldsRejectWrongCaseAndMultipleAudience(t *testing.T) {
	if _, err := decodeJWTHeader([]byte(`{"ALG":"RS256","kid":"fixture-key"}`)); err == nil {
		t.Fatal("wrong-case JWT algorithm accepted")
	}
	claims := `{"iss":"https://login.microsoftonline.com/00000000-0000-4000-8000-000000000001/v2.0","aud":["fixture-client","other"],"tid":"00000000-0000-4000-8000-000000000001","sub":"subject","exp":9999999999,"iat":1}`
	if _, err := parseClaims([]byte(claims)); err == nil {
		t.Fatal("multiple audience accepted without azp validation")
	}
}

func TestMicrosoftJWKSAllowsProviderKeyWithoutOptionalAlgorithm(t *testing.T) {
	privateKey, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	modulus, exponent := rsaJWK(privateKey)
	keys, err := parseJWKS([]byte(`{"keys":[{"kty":"RSA","use":"sig","kid":"fixture-key","n":"` + modulus + `","e":"` + exponent + `","issuer":"https://login.microsoftonline.com/{tenantid}/v2.0","cloud_instance_name":"microsoftonline.com","x5t":"fixture-thumbprint","x5c":["fixture-certificate"]}]}`))
	if err != nil || keys["fixture-key"].PublicKey == nil {
		t.Fatalf("provider key without alg rejected: %v", err)
	}
	if _, err := parseJWKS([]byte(`{"keys":[{"kty":"RSA","use":"sig","alg":"HS256","kid":"fixture-key","n":"` + modulus + `","e":"` + exponent + `","issuer":"https://login.microsoftonline.com/{tenantid}/v2.0"}]}`)); err == nil {
		t.Fatal("conflicting JWK algorithm accepted")
	}
	if _, err := parseJWKS([]byte(`{"keys":[{"kty":"RSA","use":"sig","kid":"fixture-key","n":"` + modulus + `","e":"` + exponent + `"}]}`)); err == nil {
		t.Fatal("missing signing-key issuer accepted")
	}
	if _, err := parseJWKS([]byte(`{"keys":[{"kty":"RSA","use":"sig","kid":"fixture-key","n":"` + modulus + `","e":"` + exponent + `","issuer":"https://issuer.invalid/{tenantid}/v2.0"}]}`)); err == nil {
		t.Fatal("wrong signing-key issuer accepted")
	}
}

func TestMicrosoftConcreteSigningKeyIssuerMatchesTokenIssuer(t *testing.T) {
	privateKey, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	tenantID := "00000000-0000-4000-8000-000000000001"
	serverURLPlaceholder = ""
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/metadata":
			_, _ = io.WriteString(writer, `{"issuer":"https://login.microsoftonline.com/common/v2.0","jwks_uri":"`+serverURLPlaceholder+`/keys"}`)
		case "/keys":
			modulus, exponent := rsaJWK(privateKey)
			_, _ = io.WriteString(writer, `{"keys":[{"kty":"RSA","use":"sig","kid":"fixture-key","n":"`+modulus+`","e":"`+exponent+`","issuer":"https://login.microsoftonline.com/`+tenantID+`/v2.0","cloud_instance_name":"microsoftonline.com","x5c":["fixture-certificate"]}]}`)
		default:
			writer.WriteHeader(http.StatusNotFound)
		}
	}))
	defer server.Close()
	serverURLPlaceholder = server.URL
	runtime, err := NewCalendar(&memoryStore{values: map[string]string{}}, Config{ClientID: "fixture-client"})
	if err != nil {
		t.Fatal(err)
	}
	runtime.allowTestEndpoints = true
	runtime.metadataURL = server.URL + "/metadata"
	token := signedMicrosoftIDToken(t, privateKey, tenantID, "subject-a", "")
	identity, err := runtime.verifyIDToken(context.Background(), token, false, "")
	if err != nil || identity != "microsoft:"+tenantID+":subject-a" {
		t.Fatalf("concrete signing-key issuer rejected: %q %v", identity, err)
	}
}

var serverURLPlaceholder string

func rsaJWK(privateKey *rsa.PrivateKey) (string, string) {
	return base64.RawURLEncoding.EncodeToString(privateKey.PublicKey.N.Bytes()), base64.RawURLEncoding.EncodeToString(bigEndianExponent(privateKey.PublicKey.E))
}

func bigEndianExponent(value int) []byte {
	if value == 0 {
		return []byte{0}
	}
	bytes := []byte{}
	for value > 0 {
		bytes = append([]byte{byte(value)}, bytes...)
		value >>= 8
	}
	return bytes
}

func signedMicrosoftIDToken(t *testing.T, privateKey *rsa.PrivateKey, tenantID, subject, nonce string) string {
	t.Helper()
	header := base64.RawURLEncoding.EncodeToString([]byte(`{"alg":"RS256","kid":"fixture-key","typ":"JWT"}`))
	claims := `{"iss":"https://login.microsoftonline.com/` + tenantID + `/v2.0","aud":"fixture-client","tid":"` + tenantID + `","sub":"` + subject + `","exp":` + strconv.FormatInt(time.Now().Add(time.Hour).Unix(), 10) + `,"iat":` + strconv.FormatInt(time.Now().Add(-time.Minute).Unix(), 10)
	if nonce != "" {
		claims += `,"nonce":"` + nonce + `"`
	}
	claims += `}`
	payload := base64.RawURLEncoding.EncodeToString([]byte(claims))
	message := []byte(header + "." + payload)
	digest := sha256.Sum256(message)
	signature, err := rsa.SignPKCS1v15(rand.Reader, privateKey, crypto.SHA256, digest[:])
	if err != nil {
		t.Fatal(err)
	}
	return header + "." + payload + "." + base64.RawURLEncoding.EncodeToString(signature)
}
