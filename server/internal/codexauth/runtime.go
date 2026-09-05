package codexauth

import (
	"bufio"
	"bytes"
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
	codexClientID      = "app_EMoamEEZ73f0CkXaXp7hrann"
	defaultAuthURL     = "https://auth.openai.com/oauth/authorize"
	defaultTokenURL    = "https://auth.openai.com/oauth/token"
	defaultEndpoint    = "https://chatgpt.com/backend-api/codex/responses"
	defaultRedirectURI = "http://localhost:1455/auth/callback"
	credentialName     = "FLOE_CODEX_OAUTH"
)

var unavailable = errors.New("Codex authentication unavailable")
var invalidOutput = errors.New("invalid Codex output")

type Store interface {
	Get(string) (string, error)
	Put(string, string) error
	Delete(string) error
}

type tokenBundle struct {
	AccessToken  string    `json:"access_token"`
	RefreshToken string    `json:"refresh_token"`
	IDToken      string    `json:"id_token"`
	AccountID    string    `json:"account_id"`
	Email        string    `json:"email,omitempty"`
	ExpiresAt    time.Time `json:"expires_at"`
}

type loginFlow struct {
	state, verifier, authURL string
	expires                  time.Time
	server                   *http.Server
	listener                 net.Listener
}

type Runtime struct {
	operation                    sync.Mutex
	mu                           sync.RWMutex
	store                        Store
	client                       *http.Client
	tokens                       *tokenBundle
	flow                         *loginFlow
	authURL, tokenURL, endpoint  string
	redirectURI, callbackAddress string
}

func New(store Store) *Runtime {
	transport := &http.Transport{
		Proxy:               nil,
		DialContext:         (&net.Dialer{Timeout: 5 * time.Second, KeepAlive: 30 * time.Second}).DialContext,
		TLSClientConfig:     &tls.Config{MinVersion: tls.VersionTLS12},
		TLSHandshakeTimeout: 5 * time.Second,
		MaxIdleConns:        4,
		IdleConnTimeout:     30 * time.Second,
	}
	return &Runtime{
		store: store,
		client: &http.Client{
			Transport:     transport,
			CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
		},
		authURL: defaultAuthURL, tokenURL: defaultTokenURL, endpoint: defaultEndpoint,
		redirectURI: defaultRedirectURI, callbackAddress: "127.0.0.1:1455",
	}
}

func randomValue() (string, error) {
	value := make([]byte, 32)
	if _, err := rand.Read(value); err != nil {
		return "", unavailable
	}
	return base64.RawURLEncoding.EncodeToString(value), nil
}

func (runtime *Runtime) load() *tokenBundle {
	runtime.mu.RLock()
	current := runtime.tokens
	runtime.mu.RUnlock()
	if current != nil {
		copy := *current
		return &copy
	}
	if runtime.store == nil {
		return nil
	}
	encoded, err := runtime.store.Get(credentialName)
	if err != nil || encoded == "" || len(encoded) > 32768 {
		return nil
	}
	var value tokenBundle
	if json.Unmarshal([]byte(encoded), &value) != nil || value.AccessToken == "" || value.RefreshToken == "" || value.AccountID == "" || value.ExpiresAt.IsZero() {
		return nil
	}
	runtime.mu.Lock()
	runtime.tokens = &value
	runtime.mu.Unlock()
	return &value
}

func (runtime *Runtime) save(value *tokenBundle) error {
	if runtime.store == nil || value == nil || value.AccessToken == "" || value.RefreshToken == "" || value.AccountID == "" {
		return unavailable
	}
	encoded, err := json.Marshal(value)
	if err != nil || runtime.store.Put(credentialName, string(encoded)) != nil {
		return unavailable
	}
	copy := *value
	runtime.mu.Lock()
	runtime.tokens = &copy
	runtime.mu.Unlock()
	return nil
}

func (runtime *Runtime) Ready() bool { return runtime.load() != nil }

func (runtime *Runtime) Action(ctx context.Context, action string) (any, error) {
	if action != "status" && action != "login" && action != "cancel" && action != "logout" {
		return nil, unavailable
	}
	if !runtime.operation.TryLock() {
		return nil, unavailable
	}
	defer runtime.operation.Unlock()
	if ctx.Err() != nil {
		return nil, unavailable
	}
	if action == "logout" {
		runtime.cancelLogin()
		if runtime.store == nil || runtime.store.Delete(credentialName) != nil {
			return nil, unavailable
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
	return map[string]any{"status": status, "auth_url": authURL, "inference_enabled": status == "connected"}, nil
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
	challenge := sha256.Sum256([]byte(verifier))
	parameters := url.Values{
		"client_id": {codexClientID}, "response_type": {"code"}, "redirect_uri": {runtime.redirectURI},
		"scope": {"openid email profile offline_access"}, "state": {state},
		"code_challenge": {base64.RawURLEncoding.EncodeToString(challenge[:])}, "code_challenge_method": {"S256"},
		"prompt": {"login"}, "id_token_add_organizations": {"true"}, "codex_cli_simplified_flow": {"true"},
	}
	listener, err := net.Listen("tcp", runtime.callbackAddress)
	if err != nil {
		return unavailable
	}
	flow := &loginFlow{state: state, verifier: verifier, authURL: runtime.authURL + "?" + parameters.Encode(), expires: time.Now().Add(5 * time.Minute), listener: listener}
	mux := http.NewServeMux()
	mux.HandleFunc("/auth/callback", func(writer http.ResponseWriter, request *http.Request) { runtime.callback(flow, writer, request) })
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
	if request.Method != http.MethodGet || request.URL.Query().Get("state") != flow.state || request.URL.Query().Get("code") == "" || time.Now().After(flow.expires) {
		writer.WriteHeader(http.StatusBadRequest)
		_, _ = io.WriteString(writer, callbackPage("Codex connection failed", "Return to Floe and start the connection again."))
		return
	}
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	runtime.mu.RLock()
	active := runtime.flow == flow
	runtime.mu.RUnlock()
	if !active {
		writer.WriteHeader(http.StatusConflict)
		_, _ = io.WriteString(writer, callbackPage("Codex connection expired", "Return to Floe and start the connection again."))
		return
	}
	ctx, cancel := context.WithTimeout(request.Context(), 15*time.Second)
	defer cancel()
	value, err := runtime.exchange(ctx, request.URL.Query().Get("code"), flow.verifier)
	if err == nil {
		err = runtime.save(value)
	}
	if err != nil {
		writer.WriteHeader(http.StatusBadGateway)
		_, _ = io.WriteString(writer, callbackPage("Codex connection failed", "Return to Floe and try again."))
		runtime.finishLogin(flow)
		return
	}
	_, _ = io.WriteString(writer, callbackPage("Codex connected", "You can close this window and return to Floe."))
	runtime.finishLogin(flow)
}

func (runtime *Runtime) finishLogin(flow *loginFlow) {
	runtime.mu.Lock()
	if runtime.flow == flow {
		runtime.flow = nil
	}
	runtime.mu.Unlock()
	go func() { _ = flow.server.Shutdown(context.Background()) }()
}

func callbackPage(title, message string) string {
	return "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'><title>" + html.EscapeString(title) + "</title><body style='font:16px system-ui;padding:48px'><h1>" + html.EscapeString(title) + "</h1><p>" + html.EscapeString(message) + "</p></body>"
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

func (runtime *Runtime) exchange(ctx context.Context, code, verifier string) (*tokenBundle, error) {
	return runtime.tokenRequest(ctx, url.Values{
		"grant_type": {"authorization_code"}, "client_id": {codexClientID}, "code": {code},
		"redirect_uri": {runtime.redirectURI}, "code_verifier": {verifier},
	}, "")
}

func (runtime *Runtime) refresh(ctx context.Context, current *tokenBundle) (*tokenBundle, error) {
	return runtime.tokenRequest(ctx, url.Values{
		"grant_type": {"refresh_token"}, "client_id": {codexClientID}, "refresh_token": {current.RefreshToken},
		"scope": {"openid profile email"},
	}, current.RefreshToken)
}

func (runtime *Runtime) tokenRequest(ctx context.Context, form url.Values, previousRefresh string) (*tokenBundle, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, runtime.tokenURL, strings.NewReader(form.Encode()))
	if err != nil {
		return nil, unavailable
	}
	request.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	request.Header.Set("Accept", "application/json")
	response, err := runtime.client.Do(request)
	if err != nil {
		return nil, unavailable
	}
	defer response.Body.Close()
	body, err := io.ReadAll(io.LimitReader(response.Body, 65537))
	if err != nil || len(body) > 65536 || response.StatusCode != http.StatusOK {
		return nil, unavailable
	}
	var value struct {
		AccessToken  string `json:"access_token"`
		RefreshToken string `json:"refresh_token"`
		IDToken      string `json:"id_token"`
		ExpiresIn    int    `json:"expires_in"`
	}
	if json.Unmarshal(body, &value) != nil || value.AccessToken == "" || value.IDToken == "" || value.ExpiresIn < 60 || value.ExpiresIn > 2592000 {
		return nil, unavailable
	}
	if value.RefreshToken == "" {
		value.RefreshToken = previousRefresh
	}
	accountID, email := tokenIdentity(value.IDToken)
	if value.RefreshToken == "" || accountID == "" {
		return nil, unavailable
	}
	return &tokenBundle{AccessToken: value.AccessToken, RefreshToken: value.RefreshToken, IDToken: value.IDToken, AccountID: accountID, Email: email, ExpiresAt: time.Now().Add(time.Duration(value.ExpiresIn) * time.Second)}, nil
}

func tokenIdentity(value string) (string, string) {
	parts := strings.Split(value, ".")
	if len(parts) != 3 {
		return "", ""
	}
	payload, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil || len(payload) > 32768 {
		return "", ""
	}
	var claims struct {
		Email string `json:"email"`
		Auth  struct {
			AccountID        string `json:"account_id"`
			ChatGPTAccountID string `json:"chatgpt_account_id"`
		} `json:"https://api.openai.com/auth"`
	}
	if json.Unmarshal(payload, &claims) != nil {
		return "", ""
	}
	if claims.Auth.AccountID != "" {
		return claims.Auth.AccountID, claims.Email
	}
	return claims.Auth.ChatGPTAccountID, claims.Email
}

func (runtime *Runtime) access(ctx context.Context) (*tokenBundle, error) {
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	current := runtime.load()
	if current == nil {
		return nil, unavailable
	}
	if current.ExpiresAt.After(time.Now().Add(2 * time.Minute)) {
		return current, nil
	}
	refreshCtx, cancel := context.WithTimeout(context.WithoutCancel(ctx), 15*time.Second)
	defer cancel()
	next, err := runtime.refresh(refreshCtx, current)
	if err != nil || runtime.save(next) != nil {
		return nil, unavailable
	}
	return next, nil
}

func (runtime *Runtime) Generate(ctx context.Context, model, reasoningEffort, instructions string, input, schema json.RawMessage) (string, error) {
	credential, err := runtime.access(ctx)
	if err != nil {
		return "", err
	}
	requestBody := map[string]any{
		"model": model, "instructions": instructions,
		"input": []any{map[string]any{"type": "message", "role": "user", "content": []any{map[string]string{"type": "input_text", "text": string(input)}}}},
		"tools": []any{}, "tool_choice": "none", "parallel_tool_calls": true, "stream": true, "store": false,
		"include": []string{"reasoning.encrypted_content"},
		"text":    map[string]any{"format": map[string]any{"type": "json_schema", "name": "floe_result", "strict": true, "schema": schema}},
	}
	if reasoningEffort != "" {
		requestBody["reasoning"] = map[string]string{"effort": reasoningEffort, "summary": "auto"}
	}
	payload, err := json.Marshal(requestBody)
	if err != nil {
		return "", invalidOutput
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, runtime.endpoint, bytes.NewReader(payload))
	if err != nil {
		return "", unavailable
	}
	request.Header.Set("Authorization", "Bearer "+credential.AccessToken)
	request.Header.Set("ChatGPT-Account-Id", credential.AccountID)
	request.Header.Set("Content-Type", "application/json")
	request.Header.Set("Accept", "text/event-stream")
	request.Header.Set("Originator", "floe")
	request.Header.Set("User-Agent", "floe-server/0.1")
	response, err := runtime.client.Do(request)
	if err != nil {
		if ctx.Err() != nil {
			return "", ctx.Err()
		}
		return "", unavailable
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return "", unavailable
	}
	return readResponse(response.Body)
}

func readResponse(reader io.Reader) (string, error) {
	scanner := bufio.NewScanner(io.LimitReader(reader, 1048577))
	scanner.Buffer(make([]byte, 4096), 1048576)
	var output string
	completed := false
	for scanner.Scan() {
		line := scanner.Text()
		if !strings.HasPrefix(line, "data:") {
			continue
		}
		data := strings.TrimSpace(strings.TrimPrefix(line, "data:"))
		if data == "" || data == "[DONE]" {
			continue
		}
		var event struct {
			Type  string `json:"type"`
			Delta string `json:"delta"`
			Text  string `json:"text"`
		}
		if json.Unmarshal([]byte(data), &event) != nil {
			return "", invalidOutput
		}
		switch event.Type {
		case "response.output_text.delta":
			output += event.Delta
		case "response.output_text.done":
			if event.Text != "" {
				output = event.Text
			}
			completed = true
		case "response.failed", "response.incomplete", "error":
			return "", unavailable
		}
		if len(output) > 8192 {
			return "", invalidOutput
		}
	}
	if scanner.Err() != nil || !completed || output == "" || !json.Valid([]byte(output)) {
		return "", invalidOutput
	}
	return output, nil
}

func (runtime *Runtime) Close() {
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	runtime.cancelLogin()
}
