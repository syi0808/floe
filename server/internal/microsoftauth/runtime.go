package microsoftauth

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
	defaultAuthURL         = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
	defaultTokenURL        = "https://login.microsoftonline.com/common/oauth2/v2.0/token"
	credentialName         = "FLOE_MICROSOFT_MAIL_OAUTH"
	mailReadScope          = "Mail.Read"
	calendarCredentialName = "FLOE_MICROSOFT_CALENDAR_OAUTH"
	calendarReadScope      = "Calendars.Read"
	teamsCredentialName    = "FLOE_MICROSOFT_TEAMS_OAUTH"
	teamsReadScope         = "ChannelMessage.Read.All"
	openidScope            = "openid"
	profileScope           = "profile"
	defaultMetadataURL     = "https://login.microsoftonline.com/common/v2.0/.well-known/openid-configuration"
)

var ErrUnavailable = errors.New("Microsoft authentication unavailable")
var ErrCredentialExpired = errors.New("Microsoft credential expired")

type Store interface {
	Get(string) (string, error)
	Put(string, string) error
	Delete(string) error
}

type Config struct {
	ClientID       string
	ClientSecret   string
	CredentialName string
	Scope          string
}

type tokenBundle struct {
	ClientID               string    `json:"client_id"`
	AccessToken            string    `json:"access_token"`
	RefreshToken           string    `json:"refresh_token"`
	Scope                  string    `json:"scope"`
	ExpiresAt              time.Time `json:"expires_at"`
	IDToken                string    `json:"id_token,omitempty"`
	ProviderIdentity       string    `json:"provider_identity,omitempty"`
	IdentityVerified       bool      `json:"identity_verified,omitempty"`
	IdentityReviewRequired bool      `json:"identity_review_required,omitempty"`
}

type loginFlow struct {
	state, verifier, nonce, authURL, redirectURI string
	expires                                      time.Time
	server                                       *http.Server
	listener                                     net.Listener
}

type Runtime struct {
	operation            sync.Mutex
	identityFence        sync.RWMutex
	mu                   sync.RWMutex
	store                Store
	config               Config
	client               *http.Client
	tokens               *tokenBundle
	flow                 *loginFlow
	authURL              string
	tokenURL             string
	callbackAddress      string
	credentialName       string
	credentialNamespace  string
	credentialGeneration uint64
	scope                string
	metadataURL          string
	allowTestEndpoints   bool
}

func New(store Store, config Config) (*Runtime, error) {
	if store == nil || !validCredential(config.ClientID, 512) || len(config.ClientSecret) > 2048 || strings.ContainsAny(config.ClientSecret, "\r\n") {
		return nil, ErrUnavailable
	}
	name, scope := config.CredentialName, config.Scope
	if name == "" && scope == "" {
		name, scope = credentialName, mailReadScope
	}
	validProfile := name == credentialName && scope == mailReadScope ||
		name == calendarCredentialName && scope == calendarReadScope ||
		name == teamsCredentialName && scope == teamsReadScope
	if !validProfile {
		return nil, ErrUnavailable
	}
	transport := &http.Transport{Proxy: nil, DialContext: (&net.Dialer{Timeout: 5 * time.Second, KeepAlive: 30 * time.Second}).DialContext, TLSClientConfig: &tls.Config{MinVersion: tls.VersionTLS12}, TLSHandshakeTimeout: 5 * time.Second, MaxIdleConns: 4, IdleConnTimeout: 30 * time.Second}
	return &Runtime{store: store, config: config, client: &http.Client{Transport: transport, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}, authURL: defaultAuthURL, tokenURL: defaultTokenURL, metadataURL: defaultMetadataURL, callbackAddress: "127.0.0.1:0", credentialName: name, credentialNamespace: name, scope: scope}, nil
}

func NewCalendar(store Store, config Config) (*Runtime, error) {
	config.CredentialName = calendarCredentialName
	config.Scope = calendarReadScope
	return New(store, config)
}

func NewTeams(store Store, config Config) (*Runtime, error) {
	config.CredentialName = teamsCredentialName
	config.Scope = teamsReadScope
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
		if runtime.requiresProviderIdentity() && (!current.IdentityVerified || current.IdentityReviewRequired || current.ProviderIdentity == "") {
			return "", ErrCredentialExpired
		}
		return current.AccessToken, nil
	}
	if runtime.requiresProviderIdentity() && current.IdentityReviewRequired {
		return "", ErrCredentialExpired
	}
	form := url.Values{"grant_type": {"refresh_token"}, "client_id": {runtime.config.ClientID}, "refresh_token": {current.RefreshToken}, "scope": {runtime.requestedScope()}}
	if runtime.config.ClientSecret != "" {
		form.Set("client_secret", runtime.config.ClientSecret)
	}
	refreshed, err := runtime.tokenRequest(ctx, form, current, "")
	if errors.Is(err, ErrCredentialExpired) {
		runtime.clear()
	}
	if err != nil {
		return "", ErrCredentialExpired
	}
	if runtime.requiresProviderIdentity() {
		if refreshed.ProviderIdentity == "" || !refreshed.IdentityVerified {
			if refreshed.ProviderIdentity == "" {
				refreshed.ProviderIdentity = current.ProviderIdentity
			}
			_ = runtime.save(refreshed)
			return "", ErrCredentialExpired
		}
		if current.ProviderIdentity != "" && current.ProviderIdentity != refreshed.ProviderIdentity {
			refreshed.IdentityVerified = false
			_ = runtime.save(refreshed)
			return "", ErrCredentialExpired
		}
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
	if current == nil || !runtime.requiresProviderIdentity() || !hasScope(current.Scope, openidScope) || !hasScope(current.Scope, profileScope) {
		return "", ErrCredentialExpired
	}
	if current.IdentityReviewRequired {
		return "", ErrCredentialExpired
	}
	if current.IdentityVerified && current.ProviderIdentity != "" {
		return current.ProviderIdentity, nil
	}
	form := url.Values{"grant_type": {"refresh_token"}, "client_id": {runtime.config.ClientID}, "refresh_token": {current.RefreshToken}, "scope": {runtime.requestedScope()}}
	if runtime.config.ClientSecret != "" {
		form.Set("client_secret", runtime.config.ClientSecret)
	}
	refreshed, err := runtime.tokenRequest(ctx, form, current, "")
	if err != nil || refreshed == nil || !refreshed.IdentityVerified || refreshed.ProviderIdentity == "" {
		return "", ErrCredentialExpired
	}
	if current.ProviderIdentity != "" && current.ProviderIdentity != refreshed.ProviderIdentity {
		refreshed.IdentityVerified = false
		refreshed.IdentityReviewRequired = true
		_ = runtime.save(refreshed)
		return "", ErrCredentialExpired
	}
	if err := runtime.save(refreshed); err != nil {
		return "", err
	}
	return refreshed.ProviderIdentity, nil
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
		runtime.cancelLogin()
		runtime.publishIdentityDeny()
		if runtime.store.Delete(runtime.credentialKey()) != nil {
			return nil, ErrUnavailable
		}
		runtime.identityFence.Lock()
		runtime.mu.Lock()
		runtime.tokens = nil
		runtime.mu.Unlock()
		runtime.identityFence.Unlock()
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
	return map[string]any{"status": status, "auth_url": authURL, "scope": runtime.scope}, nil
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
	nonce, err := randomValue()
	if err != nil {
		return ErrUnavailable
	}
	redirectURI := "http://" + listener.Addr().String() + "/oauth/microsoft/callback"
	challenge := sha256.Sum256([]byte(verifier))
	parameters := url.Values{"client_id": {runtime.config.ClientID}, "response_type": {"code"}, "redirect_uri": {redirectURI}, "scope": {runtime.requestedScope()}, "state": {state}, "nonce": {nonce}, "code_challenge": {base64.RawURLEncoding.EncodeToString(challenge[:])}, "code_challenge_method": {"S256"}, "response_mode": {"query"}}
	flow := &loginFlow{state: state, verifier: verifier, nonce: nonce, authURL: runtime.authURL + "?" + parameters.Encode(), redirectURI: redirectURI, expires: time.Now().Add(5 * time.Minute), listener: listener}
	mux := http.NewServeMux()
	mux.HandleFunc("/oauth/microsoft/callback", func(writer http.ResponseWriter, request *http.Request) { runtime.callback(flow, writer, request) })
	flow.server = &http.Server{Handler: mux, ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 10 * time.Second, WriteTimeout: 15 * time.Second, IdleTimeout: 5 * time.Second, MaxHeaderBytes: 8192}
	runtime.mu.Lock()
	runtime.flow = flow
	runtime.mu.Unlock()
	go func() { _ = flow.server.Serve(listener) }()
	go runtime.expireLogin(flow)
	return nil
}

func (runtime *Runtime) expireLogin(flow *loginFlow) {
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
}

func (runtime *Runtime) callback(flow *loginFlow, writer http.ResponseWriter, request *http.Request) {
	writer.Header().Set("Content-Type", "text/html; charset=utf-8")
	writer.Header().Set("Cache-Control", "no-store")
	writer.Header().Set("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'")
	if request.Method != http.MethodGet || request.URL.Query().Get("state") != flow.state || request.URL.Query().Get("code") == "" || request.URL.Query().Get("error") != "" || time.Now().After(flow.expires) {
		writer.WriteHeader(http.StatusBadRequest)
		_, _ = io.WriteString(writer, callbackPage("Microsoft connection failed", "Return to Floe and start the connection again."))
		return
	}
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	runtime.mu.RLock()
	active := runtime.flow == flow
	runtime.mu.RUnlock()
	if !active {
		writer.WriteHeader(http.StatusConflict)
		_, _ = io.WriteString(writer, callbackPage("Microsoft connection expired", "Return to Floe and start the connection again."))
		return
	}
	ctx, cancel := context.WithTimeout(request.Context(), 15*time.Second)
	defer cancel()
	form := url.Values{"grant_type": {"authorization_code"}, "client_id": {runtime.config.ClientID}, "code": {request.URL.Query().Get("code")}, "redirect_uri": {flow.redirectURI}, "code_verifier": {flow.verifier}, "scope": {runtime.requestedScope()}}
	if runtime.config.ClientSecret != "" {
		form.Set("client_secret", runtime.config.ClientSecret)
	}
	value, err := runtime.tokenRequest(ctx, form, nil, flow.nonce)
	if err == nil {
		err = runtime.save(value)
	}
	if err != nil {
		writer.WriteHeader(http.StatusBadGateway)
		_, _ = io.WriteString(writer, callbackPage("Microsoft connection failed", "Return to Floe and try again."))
		runtime.finishLogin(flow)
		return
	}
	_, _ = io.WriteString(writer, callbackPage("Microsoft connected", "You can close this window and return to Floe."))
	runtime.finishLogin(flow)
}

func (runtime *Runtime) tokenRequest(ctx context.Context, form url.Values, previous *tokenBundle, expectedNonce string) (*tokenBundle, error) {
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
		IDToken      string `json:"id_token"`
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
	if !validCredential(refresh, 16384) || !hasScope(scope, runtime.scope) || runtime.requiresProviderIdentity() && (!hasScope(scope, openidScope) || !hasScope(scope, profileScope)) {
		return nil, ErrCredentialExpired
	}
	bundle := &tokenBundle{ClientID: runtime.config.ClientID, AccessToken: output.AccessToken, RefreshToken: refresh, Scope: scope, IDToken: output.IDToken, ExpiresAt: time.Now().Add(time.Duration(output.ExpiresIn) * time.Second)}
	if runtime.requiresProviderIdentity() && bundle.IDToken != "" {
		identity, verifyErr := runtime.verifyIDToken(ctx, bundle.IDToken, expectedNonce != "", expectedNonce)
		if verifyErr != nil {
			if errors.Is(verifyErr, ErrUnavailable) && previous != nil {
				bundle.ProviderIdentity = previous.ProviderIdentity
				bundle.IdentityVerified = false
				return bundle, nil
			}
			return nil, ErrCredentialExpired
		}
		bundle.ProviderIdentity, bundle.IdentityVerified = identity, true
	}
	return bundle, nil
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
	if !decodePersistedTokenBundle(encoded, &value) || value.ClientID != runtime.config.ClientID || !validCredential(value.AccessToken, 16384) || !validCredential(value.RefreshToken, 16384) || !hasScope(value.Scope, runtime.scope) || value.ExpiresAt.IsZero() || !runtime.validPersistedIdentity(value) {
		return nil
	}
	if runtime.requiresProviderIdentity() && (!hasScope(value.Scope, openidScope) || !hasScope(value.Scope, profileScope)) {
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
	allowed := map[string]bool{"client_id": true, "access_token": true, "refresh_token": true, "scope": true, "expires_at": true, "id_token": true, "provider_identity": true, "identity_verified": true, "identity_review_required": true}
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
	if value.ProviderIdentity != "" {
		parts := strings.Split(value.ProviderIdentity, ":")
		if len(parts) != 3 || parts[0] != "microsoft" || !validGUID(parts[1]) || !validIdentityPart(parts[2]) {
			return false
		}
	}
	if value.IdentityVerified && (value.ProviderIdentity == "" || value.IdentityReviewRequired) {
		return false
	}
	return true
}

func (runtime *Runtime) identityReady() bool {
	current := runtime.load()
	return current != nil && current.IdentityVerified && current.ProviderIdentity != "" && hasScope(current.Scope, openidScope) && hasScope(current.Scope, profileScope)
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

func (runtime *Runtime) clear() {
	runtime.publishIdentityDeny()
	_ = runtime.store.Delete(runtime.credentialKey())
	runtime.identityFence.Lock()
	runtime.mu.Lock()
	runtime.tokens = nil
	runtime.mu.Unlock()
	runtime.identityFence.Unlock()
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
		if strings.EqualFold(scope, required) {
			return true
		}
	}
	return false
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
