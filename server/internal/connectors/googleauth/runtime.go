package googleauth

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"crypto/tls"
	"encoding/base64"
	"encoding/json"
	"errors"
	"html"
	"io"
	"net"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"time"
)

const (
	defaultAuthURL        = "https://accounts.google.com/o/oauth2/v2/auth"
	defaultTokenURL       = "https://oauth2.googleapis.com/token"
	defaultRevokeURL      = "https://oauth2.googleapis.com/revoke"
	credentialName        = "FLOE_GMAIL_OAUTH"
	readonlyScope         = "https://www.googleapis.com/auth/gmail.readonly"
	driveReadonlyScope    = "https://www.googleapis.com/auth/drive.readonly"
	calendarReadonlyScope = "https://www.googleapis.com/auth/calendar.readonly"
	openidScope           = "openid"
	defaultUserInfoURL    = "https://openidconnect.googleapis.com/v1/userinfo"
)

var ErrUnavailable = errors.New("Google authentication unavailable")
var ErrCredentialExpired = errors.New("Google credential expired")

type Store interface {
	Get(string) (string, error)
	Put(string, string) error
	Delete(string) error
}

type Config struct {
	ClientID       string
	ClientSecret   string
	CredentialName string
	Scopes         []string
}

type tokenBundle struct {
	ClientID               string    `json:"client_id"`
	AccessToken            string    `json:"access_token"`
	RefreshToken           string    `json:"refresh_token"`
	Scope                  string    `json:"scope"`
	ExpiresAt              time.Time `json:"expires_at"`
	ProviderIdentity       string    `json:"provider_identity,omitempty"`
	IdentityVerified       bool      `json:"identity_verified,omitempty"`
	IdentityReviewRequired bool      `json:"identity_review_required,omitempty"`
}

type loginFlow struct {
	state, verifier, authURL, redirectURI string
	expires                               time.Time
	server                                *http.Server
	listener                              net.Listener
}

type Runtime struct {
	operation                    sync.Mutex
	identityFence                sync.RWMutex
	mu                           sync.RWMutex
	store                        Store
	config                       Config
	client                       *http.Client
	tokens                       *tokenBundle
	flow                         *loginFlow
	authURL, tokenURL, revokeURL string
	callbackAddress              string
	credentialName               string
	credentialNamespace          string
	credentialGeneration         uint64
	scopes                       []string
	userinfoURL                  string
	allowTestEndpoints           bool
}

func New(store Store, config Config) (*Runtime, error) {
	if store == nil || !validCredential(config.ClientID, 512) || len(config.ClientSecret) > 2048 || strings.ContainsAny(config.ClientSecret, "\r\n") {
		return nil, ErrUnavailable
	}
	name, scopes := config.CredentialName, append([]string(nil), config.Scopes...)
	if name == "" && len(scopes) == 0 {
		name, scopes = credentialName, []string{readonlyScope}
	}
	if !validGoogleCredentialProfile(name, scopes) {
		return nil, ErrUnavailable
	}
	transport := &http.Transport{Proxy: nil, DialContext: (&net.Dialer{Timeout: 5 * time.Second, KeepAlive: 30 * time.Second}).DialContext, TLSClientConfig: &tls.Config{MinVersion: tls.VersionTLS12}, TLSHandshakeTimeout: 5 * time.Second, MaxIdleConns: 4, IdleConnTimeout: 30 * time.Second}
	return &Runtime{store: store, config: config, client: &http.Client{Transport: transport, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}, authURL: defaultAuthURL, tokenURL: defaultTokenURL, revokeURL: defaultRevokeURL, userinfoURL: defaultUserInfoURL, callbackAddress: "127.0.0.1:0", credentialName: name, credentialNamespace: name, scopes: scopes}, nil
}

func NewDrive(store Store, config Config) (*Runtime, error) {
	config.CredentialName = "FLOE_DRIVE_OAUTH"
	config.Scopes = []string{driveReadonlyScope}
	return New(store, config)
}

func NewCalendar(store Store, config Config) (*Runtime, error) {
	config.CredentialName = "FLOE_GOOGLE_CALENDAR_OAUTH"
	config.Scopes = []string{calendarReadonlyScope, openidScope}
	return New(store, config)
}

func (runtime *Runtime) Ready() bool { return runtime.load() != nil }

func (runtime *Runtime) BindCredential(name string) error {
	if !validBoundCredential(runtime.credentialNamespace, name) {
		return ErrUnavailable
	}
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	runtime.cancelLogin()
	runtime.identityFence.Lock()
	runtime.mu.Lock()
	runtime.credentialName = name
	runtime.credentialGeneration++
	runtime.tokens = nil
	runtime.mu.Unlock()
	runtime.identityFence.Unlock()
	return nil
}

func (runtime *Runtime) credentialKey() string {
	runtime.mu.RLock()
	defer runtime.mu.RUnlock()
	return runtime.credentialName
}

func (runtime *Runtime) Token(ctx context.Context) (string, error) {
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	current := runtime.load()
	if current == nil {
		return "", ErrCredentialExpired
	}
	if current.ExpiresAt.After(time.Now().Add(time.Minute)) {
		if runtime.requiresProviderIdentity() {
			if !hasScope(current.Scope, openidScope) {
				return "", ErrCredentialExpired
			}
			if current.IdentityVerified && current.ProviderIdentity != "" && !current.IdentityReviewRequired {
				return current.AccessToken, nil
			}
			identity, identityErr := runtime.identityForToken(ctx, current.AccessToken)
			identityChanged := current.ProviderIdentity != "" && current.ProviderIdentity != identity
			if identityErr != nil || identityChanged || current.IdentityReviewRequired {
				if identityChanged {
					current.ProviderIdentity = identity
					current.IdentityReviewRequired = true
				}
				current.IdentityVerified = false
				_ = runtime.save(current)
				return "", ErrCredentialExpired
			}
			if current.ProviderIdentity != identity || !current.IdentityVerified {
				current.ProviderIdentity, current.IdentityVerified = identity, true
				if runtime.save(current) != nil {
					return "", ErrCredentialExpired
				}
			}
		}
		return current.AccessToken, nil
	}
	refreshed, err := runtime.tokenRequest(ctx, url.Values{"grant_type": {"refresh_token"}, "client_id": {runtime.config.ClientID}, "refresh_token": {current.RefreshToken}, "client_secret": {runtime.config.ClientSecret}}, current)
	if errors.Is(err, ErrCredentialExpired) {
		_ = runtime.store.Delete(runtime.credentialKey())
		runtime.identityFence.Lock()
		runtime.mu.Lock()
		runtime.tokens = nil
		runtime.mu.Unlock()
		runtime.identityFence.Unlock()
	}
	if err != nil {
		return "", ErrCredentialExpired
	}
	if runtime.requiresProviderIdentity() {
		identity, identityErr := runtime.identityForToken(ctx, refreshed.AccessToken)
		refreshed.ProviderIdentity = identity
		if identityErr != nil || current.ProviderIdentity != "" && current.ProviderIdentity != identity || current.IdentityReviewRequired {
			if identityErr != nil {
				refreshed.ProviderIdentity = current.ProviderIdentity
			}
			refreshed.IdentityVerified = false
			refreshed.IdentityReviewRequired = current.IdentityReviewRequired || identityErr == nil && current.ProviderIdentity != "" && current.ProviderIdentity != identity
			_ = runtime.save(refreshed)
			return "", ErrCredentialExpired
		}
		refreshed.ProviderIdentity, refreshed.IdentityVerified = identity, true
	}
	if runtime.save(refreshed) != nil {
		return "", ErrCredentialExpired
	}
	return refreshed.AccessToken, nil
}

func (runtime *Runtime) ProviderIdentity(ctx context.Context) (string, error) {
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	current := runtime.load()
	if current == nil || !runtime.requiresProviderIdentity() || !hasScope(current.Scope, openidScope) {
		return "", ErrCredentialExpired
	}
	if current.IdentityVerified && current.ProviderIdentity != "" && !current.IdentityReviewRequired {
		return current.ProviderIdentity, nil
	}
	if current.IdentityReviewRequired {
		return "", ErrCredentialExpired
	}
	identity, err := runtime.identityForToken(ctx, current.AccessToken)
	if err != nil {
		current.IdentityVerified = false
		_ = runtime.save(current)
		return "", err
	}
	if current.ProviderIdentity != "" && current.ProviderIdentity != identity {
		current.IdentityVerified = false
		current.IdentityReviewRequired = true
		_ = runtime.save(current)
		return "", ErrCredentialExpired
	}
	current.ProviderIdentity, current.IdentityVerified, current.IdentityReviewRequired = identity, true, false
	if err := runtime.save(current); err != nil {
		return "", err
	}
	return identity, nil
}

func (runtime *Runtime) ProviderIdentityStatus() (string, bool) {
	runtime.identityFence.RLock()
	defer runtime.identityFence.RUnlock()
	runtime.mu.RLock()
	defer runtime.mu.RUnlock()
	if runtime.tokens == nil {
		return "", false
	}
	return runtime.tokens.ProviderIdentity, runtime.tokens.IdentityVerified && !runtime.tokens.IdentityReviewRequired
}

func (runtime *Runtime) WithVerifiedProviderIdentity(expectedCredential, expectedIdentity string, consume func() error) error {
	if consume == nil {
		return ErrCredentialExpired
	}
	runtime.identityFence.RLock()
	defer runtime.identityFence.RUnlock()
	runtime.mu.RLock()
	current := runtime.tokens
	verified := runtime.credentialName == expectedCredential && current != nil && current.ProviderIdentity == expectedIdentity && current.IdentityVerified && !current.IdentityReviewRequired
	runtime.mu.RUnlock()
	if !verified {
		return ErrCredentialExpired
	}
	return consume()
}

func (runtime *Runtime) Action(ctx context.Context, action string) (any, error) {
	if action != "status" && action != "login" && action != "cancel" && action != "logout" {
		return nil, ErrUnavailable
	}
	if !runtime.operation.TryLock() {
		return nil, ErrUnavailable
	}
	defer runtime.operation.Unlock()
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	if action == "logout" {
		if err := runtime.logout(ctx); err != nil {
			return nil, err
		}
	}
	if action == "cancel" {
		runtime.cancelLogin()
	}
	if action == "login" && (runtime.load() == nil || runtime.requiresProviderIdentity() && !runtime.identityReady()) {
		runtime.mu.RLock()
		pending := runtime.flow != nil && runtime.flow.expires.After(time.Now())
		runtime.mu.RUnlock()
		if !pending {
			if err := runtime.startLogin(); err != nil {
				return nil, err
			}
		}
	}
	runtime.mu.RLock()
	flow := runtime.flow
	runtime.mu.RUnlock()
	status, authURL := "disconnected", ""
	if runtime.load() != nil {
		status = "connected"
	} else if flow != nil && flow.expires.After(time.Now()) {
		status, authURL = "pending", flow.authURL
	}
	return map[string]any{"status": status, "auth_url": authURL, "scope": strings.Join(runtime.scopes, " ")}, nil
}

func (runtime *Runtime) startLogin() error {
	runtime.cancelLogin()
	state, err := randomValue()
	if err != nil {
		return err
	}
	verifier, err := randomValue()
	if err != nil {
		return err
	}
	listener, err := net.Listen("tcp", runtime.callbackAddress)
	if err != nil {
		return ErrUnavailable
	}
	redirectURI := "http://" + listener.Addr().String() + "/oauth/google/callback"
	challenge := sha256.Sum256([]byte(verifier))
	parameters := url.Values{"client_id": {runtime.config.ClientID}, "response_type": {"code"}, "redirect_uri": {redirectURI}, "scope": {strings.Join(runtime.scopes, " ")}, "state": {state}, "code_challenge": {base64.RawURLEncoding.EncodeToString(challenge[:])}, "code_challenge_method": {"S256"}, "access_type": {"offline"}, "prompt": {"consent"}}
	flow := &loginFlow{state: state, verifier: verifier, authURL: runtime.authURL + "?" + parameters.Encode(), redirectURI: redirectURI, expires: time.Now().Add(5 * time.Minute), listener: listener}
	mux := http.NewServeMux()
	mux.HandleFunc("/oauth/google/callback", func(writer http.ResponseWriter, request *http.Request) { runtime.callback(flow, writer, request) })
	flow.server = &http.Server{Handler: mux, ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 10 * time.Second, WriteTimeout: 15 * time.Second, IdleTimeout: 5 * time.Second, MaxHeaderBytes: 8192}
	runtime.mu.Lock()
	runtime.flow = flow
	runtime.mu.Unlock()
	go func() { _ = flow.server.Serve(listener) }()
	go func() {
		timer := time.NewTimer(time.Until(flow.expires))
		defer timer.Stop()
		<-timer.C
		runtime.operation.Lock()
		defer runtime.operation.Unlock()
		runtime.mu.RLock()
		active := runtime.flow == flow
		runtime.mu.RUnlock()
		if active {
			runtime.cancelLogin()
		}
	}()
	return nil
}

func (runtime *Runtime) callback(flow *loginFlow, writer http.ResponseWriter, request *http.Request) {
	writer.Header().Set("Content-Type", "text/html; charset=utf-8")
	writer.Header().Set("Cache-Control", "no-store")
	writer.Header().Set("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'")
	if request.Method != http.MethodGet || request.URL.Query().Get("state") != flow.state || request.URL.Query().Get("code") == "" || request.URL.Query().Get("error") != "" || time.Now().After(flow.expires) {
		writer.WriteHeader(http.StatusBadRequest)
		_, _ = io.WriteString(writer, callbackPage("Google connection failed", "Return to Floe and start the connection again."))
		return
	}
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	runtime.mu.RLock()
	active := runtime.flow == flow
	runtime.mu.RUnlock()
	if !active {
		writer.WriteHeader(http.StatusConflict)
		_, _ = io.WriteString(writer, callbackPage("Google connection expired", "Return to Floe and start the connection again."))
		return
	}
	ctx, cancel := context.WithTimeout(request.Context(), 15*time.Second)
	defer cancel()
	value, err := runtime.tokenRequest(ctx, url.Values{"grant_type": {"authorization_code"}, "client_id": {runtime.config.ClientID}, "client_secret": {runtime.config.ClientSecret}, "code": {request.URL.Query().Get("code")}, "redirect_uri": {flow.redirectURI}, "code_verifier": {flow.verifier}}, nil)
	if err == nil {
		err = runtime.save(value)
	}
	if err != nil {
		writer.WriteHeader(http.StatusBadGateway)
		_, _ = io.WriteString(writer, callbackPage("Google connection failed", "Return to Floe and try again."))
		runtime.finishLogin(flow)
		return
	}
	_, _ = io.WriteString(writer, callbackPage("Google connected", "You can close this window and return to Floe."))
	runtime.finishLogin(flow)
}

func (runtime *Runtime) tokenRequest(ctx context.Context, form url.Values, previous *tokenBundle) (*tokenBundle, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, runtime.tokenURL, strings.NewReader(form.Encode()))
	if err != nil {
		return nil, ErrUnavailable
	}
	request.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	request.Header.Set("Accept", "application/json")
	response, err := runtime.client.Do(request)
	if err != nil {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		return nil, ErrUnavailable
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		if response.StatusCode == http.StatusBadRequest || response.StatusCode == http.StatusUnauthorized {
			return nil, ErrCredentialExpired
		}
		return nil, ErrUnavailable
	}
	body, err := io.ReadAll(io.LimitReader(response.Body, 32769))
	if err != nil || len(body) > 32768 {
		return nil, ErrUnavailable
	}
	var output struct {
		AccessToken  string `json:"access_token"`
		RefreshToken string `json:"refresh_token"`
		ExpiresIn    int64  `json:"expires_in"`
		Scope        string `json:"scope"`
		TokenType    string `json:"token_type"`
	}
	if json.Unmarshal(body, &output) != nil || !validCredential(output.AccessToken, 16384) || output.ExpiresIn < 60 || output.ExpiresIn > 86400 || !strings.EqualFold(output.TokenType, "Bearer") {
		return nil, ErrCredentialExpired
	}
	refresh, scope := output.RefreshToken, output.Scope
	if previous != nil {
		if refresh == "" {
			refresh = previous.RefreshToken
		}
		if scope == "" {
			scope = previous.Scope
		}
	}
	if !validCredential(refresh, 16384) || !hasAllScopes(scope, runtime.scopes) {
		return nil, ErrCredentialExpired
	}
	return &tokenBundle{ClientID: runtime.config.ClientID, AccessToken: output.AccessToken, RefreshToken: refresh, Scope: scope, ExpiresAt: time.Now().Add(time.Duration(output.ExpiresIn) * time.Second)}, nil
}

func (runtime *Runtime) logout(ctx context.Context) error {
	runtime.cancelLogin()
	current := runtime.load()
	if current != nil {
		runtime.publishIdentityDeny()
		request, err := http.NewRequestWithContext(ctx, http.MethodPost, runtime.revokeURL, strings.NewReader(url.Values{"token": {current.RefreshToken}}.Encode()))
		if err != nil {
			return ErrUnavailable
		}
		request.Header.Set("Content-Type", "application/x-www-form-urlencoded")
		response, err := runtime.client.Do(request)
		if err != nil {
			return ErrUnavailable
		}
		defer response.Body.Close()
		if response.StatusCode != http.StatusOK {
			return ErrUnavailable
		}
		runtime.publishIdentityDeny()
	}
	if runtime.store.Delete(runtime.credentialKey()) != nil {
		runtime.publishIdentityDeny()
		return ErrUnavailable
	}
	runtime.identityFence.Lock()
	runtime.mu.Lock()
	runtime.tokens = nil
	runtime.mu.Unlock()
	runtime.identityFence.Unlock()
	return nil
}

func (runtime *Runtime) load() *tokenBundle {
	runtime.mu.RLock()
	current := runtime.tokens
	runtime.mu.RUnlock()
	if current != nil {
		copy := *current
		return &copy
	}
	runtime.mu.RLock()
	credentialName := runtime.credentialName
	credentialGeneration := runtime.credentialGeneration
	runtime.mu.RUnlock()
	encoded, err := runtime.store.Get(credentialName)
	if err != nil || encoded == "" || len(encoded) > 32768 {
		return nil
	}
	var value tokenBundle
	if !decodePersistedTokenBundle(encoded, &value) || value.ClientID != runtime.config.ClientID || !validCredential(value.AccessToken, 16384) || !validCredential(value.RefreshToken, 16384) || !hasAllScopes(value.Scope, runtime.resourceScopes()) || value.ExpiresAt.IsZero() || !runtime.validPersistedIdentity(value) {
		return nil
	}
	if runtime.requiresProviderIdentity() && !hasScope(value.Scope, openidScope) {
		value.ProviderIdentity, value.IdentityVerified = "", false
	} else if runtime.requiresProviderIdentity() && value.IdentityVerified && !value.IdentityReviewRequired {
		value.IdentityVerified = false
	}
	runtime.identityFence.Lock()
	runtime.mu.RLock()
	if runtime.credentialName != credentialName || runtime.credentialGeneration != credentialGeneration {
		current := runtime.tokens
		if current == nil {
			runtime.mu.RUnlock()
			runtime.identityFence.Unlock()
			return nil
		}
		copy := *current
		runtime.mu.RUnlock()
		runtime.identityFence.Unlock()
		return &copy
	}
	if current := runtime.tokens; current != nil {
		copy := *current
		runtime.mu.RUnlock()
		runtime.identityFence.Unlock()
		return &copy
	}
	runtime.mu.RUnlock()
	runtime.mu.Lock()
	runtime.tokens = &value
	runtime.mu.Unlock()
	runtime.identityFence.Unlock()
	return &value
}

func decodePersistedTokenBundle(encoded string, value *tokenBundle) bool {
	if value == nil || len(encoded) == 0 || len(encoded) > 32768 || rejectDuplicateJSON([]byte(encoded)) != nil {
		return false
	}
	var fields map[string]json.RawMessage
	if json.Unmarshal([]byte(encoded), &fields) != nil {
		return false
	}
	allowed := map[string]bool{"client_id": true, "access_token": true, "refresh_token": true, "scope": true, "expires_at": true, "provider_identity": true, "identity_verified": true, "identity_review_required": true}
	for key := range fields {
		if !allowed[key] {
			return false
		}
		for known := range allowed {
			if key != known && strings.EqualFold(key, known) {
				return false
			}
		}
	}
	return json.Unmarshal([]byte(encoded), value) == nil
}

func (runtime *Runtime) validPersistedIdentity(value tokenBundle) bool {
	if !runtime.requiresProviderIdentity() {
		return value.ProviderIdentity == "" && !value.IdentityVerified && !value.IdentityReviewRequired
	}
	if value.ProviderIdentity != "" && (!strings.HasPrefix(value.ProviderIdentity, "google:") || !validSubject(strings.TrimPrefix(value.ProviderIdentity, "google:"))) {
		return false
	}
	if value.IdentityVerified && (value.ProviderIdentity == "" || value.IdentityReviewRequired) {
		return false
	}
	return true
}

func (runtime *Runtime) resourceScopes() []string {
	if !runtime.requiresProviderIdentity() {
		return runtime.scopes
	}
	return []string{calendarReadonlyScope}
}

func (runtime *Runtime) identityReady() bool {
	current := runtime.load()
	return current != nil && current.IdentityVerified && !current.IdentityReviewRequired && current.ProviderIdentity != "" && hasScope(current.Scope, openidScope)
}

func (runtime *Runtime) save(value *tokenBundle) error {
	if value == nil || !runtime.validPersistedIdentity(*value) {
		return ErrUnavailable
	}
	encoded, err := json.Marshal(value)
	if err != nil || runtime.store.Put(runtime.credentialKey(), string(encoded)) != nil {
		runtime.publishIdentityDeny()
		return ErrUnavailable
	}
	copy := *value
	runtime.identityFence.Lock()
	runtime.mu.Lock()
	runtime.tokens = &copy
	runtime.mu.Unlock()
	runtime.identityFence.Unlock()
	return nil
}

func (runtime *Runtime) publishIdentityDeny() {
	runtime.identityFence.Lock()
	defer runtime.identityFence.Unlock()
	runtime.mu.Lock()
	defer runtime.mu.Unlock()
	if runtime.tokens == nil {
		return
	}
	denied := *runtime.tokens
	denied.IdentityVerified = false
	if denied.ProviderIdentity != "" {
		denied.IdentityReviewRequired = true
	}
	runtime.tokens = &denied
}

func (runtime *Runtime) finishLogin(flow *loginFlow) {
	runtime.mu.Lock()
	if runtime.flow == flow {
		runtime.flow = nil
	}
	runtime.mu.Unlock()
	go func() { _ = flow.server.Shutdown(context.Background()) }()
}
func (runtime *Runtime) cancelLogin() {
	runtime.mu.Lock()
	flow := runtime.flow
	runtime.flow = nil
	runtime.mu.Unlock()
	if flow != nil {
		_ = flow.server.Close()
	}
}
func (runtime *Runtime) Close() {
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	runtime.cancelLogin()
}

func randomValue() (string, error) {
	value := make([]byte, 32)
	if _, err := rand.Read(value); err != nil {
		return "", ErrUnavailable
	}
	return base64.RawURLEncoding.EncodeToString(value), nil
}
func validCredential(value string, limit int) bool {
	return strings.TrimSpace(value) != "" && len(value) <= limit && !strings.ContainsAny(value, "\r\n")
}
func hasScope(value, required string) bool {
	for _, scope := range strings.Fields(value) {
		if scope == required {
			return true
		}
	}
	return false
}
func hasAllScopes(value string, required []string) bool {
	for _, scope := range required {
		if !hasScope(value, scope) {
			return false
		}
	}
	return true
}
func validGoogleCredentialProfile(name string, scopes []string) bool {
	return name == credentialName && len(scopes) == 1 && scopes[0] == readonlyScope ||
		name == "FLOE_DRIVE_OAUTH" && len(scopes) == 1 && scopes[0] == driveReadonlyScope ||
		name == "FLOE_GOOGLE_CALENDAR_OAUTH" && len(scopes) == 2 && scopes[0] == calendarReadonlyScope && scopes[1] == openidScope
}

func validBoundCredential(namespace, name string) bool {
	if name == namespace {
		return true
	}
	prefix := namespace + ":"
	if !strings.HasPrefix(name, prefix) || len(name) != len(prefix)+64 {
		return false
	}
	for _, character := range strings.TrimPrefix(name, prefix) {
		if character < '0' || character > '9' && character < 'a' || character > 'f' {
			return false
		}
	}
	return true
}
func callbackPage(title, message string) string {
	return "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'><title>" + html.EscapeString(title) + "</title><body style='font:16px system-ui;padding:48px'><h1>" + html.EscapeString(title) + "</h1><p>" + html.EscapeString(message) + "</p></body>"
}
