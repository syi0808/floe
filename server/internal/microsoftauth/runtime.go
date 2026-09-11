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
	ClientID     string    `json:"client_id"`
	AccessToken  string    `json:"access_token"`
	RefreshToken string    `json:"refresh_token"`
	Scope        string    `json:"scope"`
	ExpiresAt    time.Time `json:"expires_at"`
}

type loginFlow struct {
	state, verifier, authURL, redirectURI string
	expires                               time.Time
	server                                *http.Server
	listener                              net.Listener
}

type Runtime struct {
	operation           sync.Mutex
	mu                  sync.RWMutex
	store               Store
	config              Config
	client              *http.Client
	tokens              *tokenBundle
	flow                *loginFlow
	authURL             string
	tokenURL            string
	callbackAddress     string
	credentialName      string
	credentialNamespace string
	scope               string
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
	return &Runtime{store: store, config: config, client: &http.Client{Transport: transport, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}, authURL: defaultAuthURL, tokenURL: defaultTokenURL, callbackAddress: "127.0.0.1:0", credentialName: name, credentialNamespace: name, scope: scope}, nil
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
	runtime.mu.Lock()
	runtime.credentialName = name
	runtime.tokens = nil
	runtime.mu.Unlock()
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
		return current.AccessToken, nil
	}
	form := url.Values{"grant_type": {"refresh_token"}, "client_id": {runtime.config.ClientID}, "refresh_token": {current.RefreshToken}, "scope": {"offline_access " + runtime.scope}}
	if runtime.config.ClientSecret != "" {
		form.Set("client_secret", runtime.config.ClientSecret)
	}
	refreshed, err := runtime.tokenRequest(ctx, form, current)
	if errors.Is(err, ErrCredentialExpired) {
		runtime.clear()
	}
	if err != nil || runtime.save(refreshed) != nil {
		return "", ErrCredentialExpired
	}
	return refreshed.AccessToken, nil
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
		if runtime.store.Delete(runtime.credentialKey()) != nil {
			return nil, ErrUnavailable
		}
		runtime.mu.Lock()
		runtime.tokens = nil
		runtime.mu.Unlock()
	}
	if action == "cancel" {
		runtime.cancelLogin()
	}
	if action == "login" && runtime.load() == nil {
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
	redirectURI := "http://" + listener.Addr().String() + "/oauth/microsoft/callback"
	challenge := sha256.Sum256([]byte(verifier))
	parameters := url.Values{"client_id": {runtime.config.ClientID}, "response_type": {"code"}, "redirect_uri": {redirectURI}, "scope": {"offline_access " + runtime.scope}, "state": {state}, "code_challenge": {base64.RawURLEncoding.EncodeToString(challenge[:])}, "code_challenge_method": {"S256"}, "response_mode": {"query"}}
	flow := &loginFlow{state: state, verifier: verifier, authURL: runtime.authURL + "?" + parameters.Encode(), redirectURI: redirectURI, expires: time.Now().Add(5 * time.Minute), listener: listener}
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
	form := url.Values{"grant_type": {"authorization_code"}, "client_id": {runtime.config.ClientID}, "code": {request.URL.Query().Get("code")}, "redirect_uri": {flow.redirectURI}, "code_verifier": {flow.verifier}, "scope": {"offline_access " + runtime.scope}}
	if runtime.config.ClientSecret != "" {
		form.Set("client_secret", runtime.config.ClientSecret)
	}
	value, err := runtime.tokenRequest(ctx, form, nil)
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
	if !validCredential(refresh, 16384) || !hasScope(scope, runtime.scope) {
		return nil, ErrCredentialExpired
	}
	return &tokenBundle{ClientID: runtime.config.ClientID, AccessToken: output.AccessToken, RefreshToken: refresh, Scope: scope, ExpiresAt: time.Now().Add(time.Duration(output.ExpiresIn) * time.Second)}, nil
}

func (runtime *Runtime) load() *tokenBundle {
	runtime.mu.RLock()
	current := runtime.tokens
	runtime.mu.RUnlock()
	if current != nil {
		copy := *current
		return &copy
	}
	encoded, err := runtime.store.Get(runtime.credentialKey())
	if err != nil || encoded == "" || len(encoded) > 32768 {
		return nil
	}
	var value tokenBundle
	if json.Unmarshal([]byte(encoded), &value) != nil || value.ClientID != runtime.config.ClientID || !validCredential(value.AccessToken, 16384) || !validCredential(value.RefreshToken, 16384) || !hasScope(value.Scope, runtime.scope) || value.ExpiresAt.IsZero() {
		return nil
	}
	runtime.mu.Lock()
	runtime.tokens = &value
	runtime.mu.Unlock()
	return &value
}

func (runtime *Runtime) save(value *tokenBundle) error {
	if value == nil {
		return ErrUnavailable
	}
	encoded, err := json.Marshal(value)
	if err != nil || runtime.store.Put(runtime.credentialKey(), string(encoded)) != nil {
		return ErrUnavailable
	}
	copy := *value
	runtime.mu.Lock()
	runtime.tokens = &copy
	runtime.mu.Unlock()
	return nil
}

func (runtime *Runtime) clear() {
	_ = runtime.store.Delete(runtime.credentialKey())
	runtime.mu.Lock()
	runtime.tokens = nil
	runtime.mu.Unlock()
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
