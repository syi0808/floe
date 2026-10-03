package workoauth

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
	"strconv"
	"strings"
	"sync"
	"time"
)

const (
	GitHubCredential = "FLOE_GITHUB_OAUTH"
	SlackCredential  = "FLOE_SLACK_OAUTH"
)

var ErrUnavailable = errors.New("work provider authentication unavailable")
var ErrCredentialExpired = errors.New("work provider credential expired")

type Store interface {
	Get(string) (string, error)
	Put(string, string) error
	Delete(string) error
}

type Config struct {
	ClientID     string
	ClientSecret string
}

type providerProfile struct {
	name, credential, authURL, tokenURL, callbackPath, callbackHost, callbackAddress string
	requiredScopes                                                                   []string
	deviceFlow                                                                       bool
}

var githubProfile = providerProfile{
	name: "GitHub", credential: GitHubCredential,
	authURL: "https://github.com/login/device/code", tokenURL: "https://github.com/login/oauth/access_token",
	requiredScopes: []string{"github.issues.read"},
	deviceFlow:     true,
}

var slackProfile = providerProfile{
	name: "Slack", credential: SlackCredential,
	authURL: "https://slack.com/oauth/v2/authorize", tokenURL: "https://slack.com/api/oauth.v2.access",
	callbackPath: "/oauth/slack/callback", callbackHost: "localhost", callbackAddress: "127.0.0.1:1456",
	requiredScopes: []string{"channels:history", "groups:history"},
}

type tokenBundle struct {
	ClientID     string    `json:"client_id"`
	AccessToken  string    `json:"access_token"`
	RefreshToken string    `json:"refresh_token,omitempty"`
	Scope        string    `json:"scope"`
	ExpiresAt    time.Time `json:"expires_at,omitempty"`
}

type loginFlow struct {
	state, verifier, authURL, redirectURI string
	deviceCode, userCode                  string
	expires                               time.Time
	nextPoll                              time.Time
	pollInterval                          time.Duration
	server                                *http.Server
	listener                              net.Listener
}

type Runtime struct {
	operation       sync.Mutex
	mu              sync.RWMutex
	store           Store
	config          Config
	profile         providerProfile
	client          *http.Client
	tokens          *tokenBundle
	flow            *loginFlow
	credentialName  string
	callbackAddress string
}

func NewGitHub(store Store, config Config) (*Runtime, error) {
	config.ClientSecret = ""
	return newRuntime(store, config, githubProfile)
}

func NewSlack(store Store, config Config) (*Runtime, error) {
	if len(config.ClientSecret) > 2048 || strings.ContainsAny(config.ClientSecret, "\r\n") {
		return nil, ErrUnavailable
	}
	return newRuntime(store, config, slackProfile)
}

func newRuntime(store Store, config Config, profile providerProfile) (*Runtime, error) {
	if store == nil || !validCredential(config.ClientID, 512) {
		return nil, ErrUnavailable
	}
	transport := &http.Transport{
		Proxy:               nil,
		DialContext:         (&net.Dialer{Timeout: 5 * time.Second, KeepAlive: 30 * time.Second}).DialContext,
		TLSClientConfig:     &tls.Config{MinVersion: tls.VersionTLS12},
		TLSHandshakeTimeout: 5 * time.Second, MaxIdleConns: 4, IdleConnTimeout: 30 * time.Second,
	}
	return &Runtime{
		store: store, config: config, profile: profile,
		client:         &http.Client{Transport: transport, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }},
		credentialName: profile.credential, callbackAddress: profile.callbackAddress,
	}, nil
}

func (runtime *Runtime) Ready() bool { return runtime.load() != nil }

func (runtime *Runtime) BindCredential(name string) error {
	if !validBoundCredential(runtime.profile.credential, name) {
		return ErrUnavailable
	}
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	runtime.cancelLogin()
	runtime.mu.Lock()
	runtime.credentialName, runtime.tokens = name, nil
	runtime.mu.Unlock()
	return nil
}

func (runtime *Runtime) Token(ctx context.Context) (string, error) {
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	current := runtime.load()
	if current == nil {
		return "", ErrCredentialExpired
	}
	if current.ExpiresAt.IsZero() || current.ExpiresAt.After(time.Now().Add(time.Minute)) {
		return current.AccessToken, nil
	}
	if current.RefreshToken == "" {
		return "", ErrCredentialExpired
	}
	refreshed, err := runtime.refresh(ctx, current)
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
		if err := runtime.logout(ctx); err != nil {
			return nil, err
		}
	}
	if action == "cancel" {
		runtime.cancelLogin()
	}
	if action == "login" && runtime.load() == nil {
		runtime.mu.RLock()
		pending := runtime.flow != nil && runtime.flow.expires.After(time.Now())
		runtime.mu.RUnlock()
		if !pending {
			if err := runtime.startLogin(ctx); err != nil {
				return nil, err
			}
		}
	}
	runtime.mu.RLock()
	flow := runtime.flow
	runtime.mu.RUnlock()
	if action == "status" && flow != nil && runtime.profile.deviceFlow && !time.Now().Before(flow.nextPoll) {
		if err := runtime.pollDeviceFlow(ctx, flow); err != nil {
			return nil, err
		}
		runtime.mu.RLock()
		flow = runtime.flow
		runtime.mu.RUnlock()
	}
	status, authURL := "disconnected", ""
	userCode := ""
	if runtime.load() != nil {
		status = "connected"
	} else if flow != nil && flow.expires.After(time.Now()) {
		status, authURL, userCode = "pending", flow.authURL, flow.userCode
	}
	return map[string]any{"status": status, "auth_url": authURL, "user_code": userCode, "scope": strings.Join(runtime.profile.requiredScopes, " ")}, nil
}

func (runtime *Runtime) startLogin(ctx context.Context) error {
	runtime.cancelLogin()
	if runtime.profile.deviceFlow {
		return runtime.startDeviceFlow(ctx)
	}
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
	port := listener.Addr().(*net.TCPAddr).Port
	redirectURI := "http://" + runtime.profile.callbackHost + ":" + strconv.Itoa(port) + runtime.profile.callbackPath
	challenge := sha256.Sum256([]byte(verifier))
	parameters := url.Values{
		"client_id": {runtime.config.ClientID}, "response_type": {"code"}, "redirect_uri": {redirectURI},
		"state": {state}, "code_challenge": {base64.RawURLEncoding.EncodeToString(challenge[:])}, "code_challenge_method": {"S256"},
	}
	if runtime.profile.name == "Slack" {
		parameters.Set("user_scope", strings.Join(runtime.profile.requiredScopes, ","))
	}
	flow := &loginFlow{state: state, verifier: verifier, authURL: runtime.profile.authURL + "?" + parameters.Encode(), redirectURI: redirectURI, expires: time.Now().Add(5 * time.Minute), listener: listener}
	mux := http.NewServeMux()
	mux.HandleFunc(runtime.profile.callbackPath, func(writer http.ResponseWriter, request *http.Request) { runtime.callback(flow, writer, request) })
	flow.server = &http.Server{Handler: mux, ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 10 * time.Second, WriteTimeout: 15 * time.Second, IdleTimeout: 5 * time.Second, MaxHeaderBytes: 8192}
	runtime.mu.Lock()
	runtime.flow = flow
	runtime.mu.Unlock()
	go func() { _ = flow.server.Serve(listener) }()
	go runtime.expire(flow)
	return nil
}

func (runtime *Runtime) startDeviceFlow(ctx context.Context) error {
	form := url.Values{"client_id": {runtime.config.ClientID}}
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, runtime.profile.authURL, strings.NewReader(form.Encode()))
	if err != nil {
		return ErrUnavailable
	}
	request.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	request.Header.Set("Accept", "application/json")
	response, err := runtime.client.Do(request)
	if err != nil {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		return ErrUnavailable
	}
	defer response.Body.Close()
	body, err := io.ReadAll(io.LimitReader(response.Body, 32769))
	if err != nil || len(body) > 32768 || response.StatusCode != http.StatusOK {
		return ErrUnavailable
	}
	var output struct {
		DeviceCode      string `json:"device_code"`
		UserCode        string `json:"user_code"`
		VerificationURI string `json:"verification_uri"`
		ExpiresIn       int64  `json:"expires_in"`
		Interval        int64  `json:"interval"`
	}
	if json.Unmarshal(body, &output) != nil || !validCredential(output.DeviceCode, 512) ||
		!validCredential(output.UserCode, 64) || output.ExpiresIn <= 0 || output.ExpiresIn > 1800 ||
		output.Interval < 1 || output.Interval > 60 || !validHTTPSURL(output.VerificationURI) {
		return ErrUnavailable
	}
	flow := &loginFlow{
		authURL: output.VerificationURI, deviceCode: output.DeviceCode, userCode: output.UserCode,
		expires:      time.Now().Add(time.Duration(output.ExpiresIn) * time.Second),
		pollInterval: time.Duration(output.Interval) * time.Second,
	}
	flow.nextPoll = time.Now().Add(flow.pollInterval)
	runtime.mu.Lock()
	runtime.flow = flow
	runtime.mu.Unlock()
	go runtime.expire(flow)
	return nil
}

func (runtime *Runtime) pollDeviceFlow(ctx context.Context, flow *loginFlow) error {
	form := url.Values{
		"client_id": {runtime.config.ClientID}, "device_code": {flow.deviceCode},
		"grant_type": {"urn:ietf:params:oauth:grant-type:device_code"},
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, runtime.profile.tokenURL, strings.NewReader(form.Encode()))
	if err != nil {
		return ErrUnavailable
	}
	request.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	request.Header.Set("Accept", "application/json")
	response, err := runtime.client.Do(request)
	if err != nil {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		return ErrUnavailable
	}
	defer response.Body.Close()
	body, err := io.ReadAll(io.LimitReader(response.Body, 32769))
	if err != nil || len(body) > 32768 || response.StatusCode != http.StatusOK {
		return ErrUnavailable
	}
	var status struct {
		Error string `json:"error"`
	}
	if json.Unmarshal(body, &status) != nil {
		return ErrUnavailable
	}
	now := time.Now()
	switch status.Error {
	case "authorization_pending":
		flow.nextPoll = now.Add(flow.pollInterval)
		return nil
	case "slow_down":
		flow.pollInterval += 5 * time.Second
		flow.nextPoll = now.Add(flow.pollInterval)
		return nil
	case "expired_token", "token_expired", "access_denied":
		runtime.finishLogin(flow)
		return nil
	case "":
		value, err := runtime.decodeGitHub(body, nil)
		if err != nil || runtime.save(value) != nil {
			return ErrCredentialExpired
		}
		runtime.finishLogin(flow)
		return nil
	default:
		return ErrCredentialExpired
	}
}

func (runtime *Runtime) expire(flow *loginFlow) {
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
		_, _ = io.WriteString(writer, callbackPage(runtime.profile.name+" connection failed", "Return to Floe and start the connection again."))
		return
	}
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	runtime.mu.RLock()
	active := runtime.flow == flow
	runtime.mu.RUnlock()
	if !active {
		writer.WriteHeader(http.StatusConflict)
		_, _ = io.WriteString(writer, callbackPage(runtime.profile.name+" connection expired", "Return to Floe and start the connection again."))
		return
	}
	ctx, cancel := context.WithTimeout(request.Context(), 15*time.Second)
	defer cancel()
	value, err := runtime.exchange(ctx, request.URL.Query().Get("code"), flow)
	if err == nil {
		err = runtime.save(value)
	}
	if err != nil {
		writer.WriteHeader(http.StatusBadGateway)
		_, _ = io.WriteString(writer, callbackPage(runtime.profile.name+" connection failed", "Return to Floe and try again."))
		runtime.finishLogin(flow)
		return
	}
	_, _ = io.WriteString(writer, callbackPage(runtime.profile.name+" connected", "You can close this window and return to Floe."))
	runtime.finishLogin(flow)
}

func (runtime *Runtime) exchange(ctx context.Context, code string, flow *loginFlow) (*tokenBundle, error) {
	form := url.Values{
		"client_id": {runtime.config.ClientID}, "code": {code}, "redirect_uri": {flow.redirectURI}, "code_verifier": {flow.verifier},
	}
	if runtime.config.ClientSecret != "" {
		form.Set("client_secret", runtime.config.ClientSecret)
	}
	return runtime.tokenRequest(ctx, form, nil)
}

func (runtime *Runtime) refresh(ctx context.Context, previous *tokenBundle) (*tokenBundle, error) {
	form := url.Values{"grant_type": {"refresh_token"}, "client_id": {runtime.config.ClientID}, "refresh_token": {previous.RefreshToken}}
	if runtime.config.ClientSecret != "" {
		form.Set("client_secret", runtime.config.ClientSecret)
	}
	return runtime.tokenRequest(ctx, form, previous)
}

func (runtime *Runtime) tokenRequest(ctx context.Context, form url.Values, previous *tokenBundle) (*tokenBundle, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, runtime.profile.tokenURL, strings.NewReader(form.Encode()))
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
	body, err := io.ReadAll(io.LimitReader(response.Body, 32769))
	if err != nil || len(body) > 32768 || response.StatusCode != http.StatusOK {
		return nil, ErrCredentialExpired
	}
	if runtime.profile.name == "Slack" {
		return runtime.decodeSlack(body, previous)
	}
	return runtime.decodeGitHub(body, previous)
}

func (runtime *Runtime) decodeGitHub(body []byte, previous *tokenBundle) (*tokenBundle, error) {
	var output struct {
		AccessToken           string `json:"access_token"`
		RefreshToken          string `json:"refresh_token"`
		Scope                 string `json:"scope"`
		TokenType             string `json:"token_type"`
		Error                 string `json:"error"`
		ExpiresIn             int64  `json:"expires_in"`
		RefreshTokenExpiresIn int64  `json:"refresh_token_expires_in"`
	}
	if json.Unmarshal(body, &output) != nil || output.Error != "" || !validCredential(output.AccessToken, 16384) || !strings.EqualFold(output.TokenType, "bearer") {
		return nil, ErrCredentialExpired
	}
	refresh, scope, expires := output.RefreshToken, output.Scope, time.Time{}
	if previous != nil {
		if refresh == "" {
			refresh = previous.RefreshToken
		}
		if scope == "" {
			scope = previous.Scope
		}
	}
	if output.ExpiresIn > 0 {
		expires = time.Now().Add(time.Duration(output.ExpiresIn) * time.Second)
	}
	return &tokenBundle{ClientID: runtime.config.ClientID, AccessToken: output.AccessToken, RefreshToken: refresh, Scope: scope, ExpiresAt: expires}, nil
}

func (runtime *Runtime) decodeSlack(body []byte, previous *tokenBundle) (*tokenBundle, error) {
	var output struct {
		OK           bool   `json:"ok"`
		Error        string `json:"error"`
		AccessToken  string `json:"access_token"`
		RefreshToken string `json:"refresh_token"`
		Scope        string `json:"scope"`
		TokenType    string `json:"token_type"`
		ExpiresIn    int64  `json:"expires_in"`
		AuthedUser   struct {
			AccessToken  string `json:"access_token"`
			RefreshToken string `json:"refresh_token"`
			Scope        string `json:"scope"`
			TokenType    string `json:"token_type"`
			ExpiresIn    int64  `json:"expires_in"`
		} `json:"authed_user"`
	}
	if json.Unmarshal(body, &output) != nil || !output.OK || output.Error != "" {
		return nil, ErrCredentialExpired
	}
	access, refresh, scope, tokenType, expiresIn := output.AuthedUser.AccessToken, output.AuthedUser.RefreshToken, output.AuthedUser.Scope, output.AuthedUser.TokenType, output.AuthedUser.ExpiresIn
	if access == "" {
		access, refresh, scope, tokenType, expiresIn = output.AccessToken, output.RefreshToken, output.Scope, output.TokenType, output.ExpiresIn
	}
	if !validCredential(access, 16384) || !strings.EqualFold(tokenType, "user") || !hasAllScopes(scope, runtime.profile.requiredScopes) {
		return nil, ErrCredentialExpired
	}
	expires := time.Time{}
	if previous != nil && refresh == "" {
		refresh = previous.RefreshToken
	}
	if expiresIn > 0 {
		expires = time.Now().Add(time.Duration(expiresIn) * time.Second)
	}
	return &tokenBundle{ClientID: runtime.config.ClientID, AccessToken: access, RefreshToken: refresh, Scope: scope, ExpiresAt: expires}, nil
}

func (runtime *Runtime) logout(context.Context) error {
	runtime.cancelLogin()
	if runtime.store.Delete(runtime.credentialKey()) != nil {
		return ErrUnavailable
	}
	runtime.mu.Lock()
	runtime.tokens = nil
	runtime.mu.Unlock()
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
	encoded, err := runtime.store.Get(runtime.credentialKey())
	if err != nil || encoded == "" || len(encoded) > 32768 {
		return nil
	}
	var value tokenBundle
	if json.Unmarshal([]byte(encoded), &value) != nil || value.ClientID != runtime.config.ClientID || !validCredential(value.AccessToken, 16384) {
		return nil
	}
	runtime.mu.Lock()
	runtime.tokens = &value
	runtime.mu.Unlock()
	return &value
}

func (runtime *Runtime) save(value *tokenBundle) error {
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

func (runtime *Runtime) credentialKey() string {
	runtime.mu.RLock()
	defer runtime.mu.RUnlock()
	return runtime.credentialName
}

func (runtime *Runtime) finishLogin(flow *loginFlow) {
	runtime.mu.Lock()
	if runtime.flow == flow {
		runtime.flow = nil
	}
	runtime.mu.Unlock()
	if flow.server != nil {
		go func() { _ = flow.server.Shutdown(context.Background()) }()
	}
}

func (runtime *Runtime) cancelLogin() {
	runtime.mu.Lock()
	flow := runtime.flow
	runtime.flow = nil
	runtime.mu.Unlock()
	if flow != nil {
		if flow.server != nil {
			_ = flow.server.Close()
		}
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

func validHTTPSURL(value string) bool {
	parsed, err := url.Parse(value)
	return err == nil && parsed.Scheme == "https" && parsed.Host != "" && parsed.User == nil
}

func hasAllScopes(value string, required []string) bool {
	granted := map[string]bool{}
	for _, scope := range strings.FieldsFunc(value, func(character rune) bool { return character == ' ' || character == ',' }) {
		granted[scope] = true
	}
	for _, scope := range required {
		if !granted[scope] {
			return false
		}
	}
	return true
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
