package console

import (
	"context"
	"embed"
	"encoding/json"
	"errors"
	"io"
	"mime"
	"net"
	"net/http"
	"regexp"
	"strings"
	"sync"
	"time"

	"floe/server/internal/inference"
)

//go:embed web/*
var assets embed.FS

type AuthRuntime interface {
	Action(context.Context, string) (any, error)
	inference.CodexClient
}

type session struct {
	csrf    string
	expires time.Time
}
type pairing struct {
	ID      string    `json:"id"`
	Code    string    `json:"code"`
	Expires time.Time `json:"expires"`
	proof   string
	token   string
}

type Console struct {
	mu                                           sync.Mutex
	directory, address, adminHash, internalToken string
	vault                                        Vault
	runtime                                      AuthRuntime
	state                                        diskState
	gateway                                      *inference.Gateway
	unavailable                                  map[string]bool
	sessions                                     map[string]session
	pair                                         *pairing
	loginAttempts                                int
	loginWindow                                  time.Time
	lastPair                                     time.Time
	testActive                                   bool
}

func New(directory, address string, vault Vault, runtime AuthRuntime) (*Console, error) {
	host, port, err := net.SplitHostPort(address)
	if err != nil || host != "127.0.0.1" || port == "" {
		return nil, errors.New("console requires 127.0.0.1:port")
	}
	state, admin, err := readState(directory)
	if err != nil {
		return nil, err
	}
	console := &Console{directory: directory, address: address, adminHash: digest(admin), internalToken: randomToken(), vault: vault, runtime: runtime, state: state, sessions: map[string]session{}}
	console.rebuild()
	return console, nil
}

func (console *Console) lookup(name string) string {
	value, _ := console.vault.Get(name)
	return value
}

func (console *Console) rebuild() {
	config := inference.Config{Targets: map[string]inference.Target{}, Routes: map[string]inference.Route{}}
	secrets := map[string]string{}
	for _, target := range console.state.Targets {
		if target.APIKeyEnv != "" {
			secrets[target.APIKeyEnv] = console.lookup(target.APIKeyEnv)
		}
	}
	lookup := func(name string) string { return secrets[name] }
	console.unavailable = map[string]bool{}
	for identifier, target := range console.state.Targets {
		if _, err := inference.New(inference.Config{Targets: map[string]inference.Target{identifier: target}}, console.internalToken, lookup, console.runtime); err != nil {
			console.unavailable[identifier] = true
		} else {
			config.Targets[identifier] = target
		}
	}
	for class, route := range console.state.Routes {
		candidate := inference.Config{Targets: config.Targets, Routes: map[string]inference.Route{class: route}}
		if _, err := inference.New(candidate, console.internalToken, lookup, console.runtime); err == nil {
			config.Routes[class] = route
		}
	}
	console.gateway, _ = inference.New(config, console.internalToken, lookup, console.runtime)
}

func reply(writer http.ResponseWriter, status int, value any) {
	writer.Header().Set("Content-Type", "application/json")
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(value)
}

func failure(writer http.ResponseWriter, status int, code string) {
	reply(writer, status, map[string]any{"error": map[string]string{"code": code}})
}

func decode(writer http.ResponseWriter, request *http.Request, output any) bool {
	mediaType, _, err := mime.ParseMediaType(request.Header.Get("Content-Type"))
	if err != nil || mediaType != "application/json" {
		return false
	}
	decoder := json.NewDecoder(http.MaxBytesReader(writer, request.Body, 16384))
	decoder.DisallowUnknownFields()
	return decoder.Decode(output) == nil && decoder.Decode(new(any)) == io.EOF
}

func (console *Console) ServeHTTP(writer http.ResponseWriter, request *http.Request) {
	writer.Header().Set("Cache-Control", "no-store")
	writer.Header().Set("X-Content-Type-Options", "nosniff")
	writer.Header().Set("Referrer-Policy", "no-referrer")
	writer.Header().Set("Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'")
	if request.Host != console.address {
		failure(writer, 403, "invalid_host")
		return
	}
	if request.Header.Get("Origin") != "" && request.Header.Get("Origin") != "http://"+console.address {
		failure(writer, 403, "invalid_origin")
		return
	}
	if strings.HasPrefix(request.URL.Path, "/v1/") {
		console.serveInference(writer, request)
		return
	}
	if strings.HasPrefix(request.URL.Path, "/pair/") {
		if request.Header.Get("Origin") != "" {
			failure(writer, 403, "unauthorized")
			return
		}
		console.servePair(writer, request)
		return
	}
	if request.Method == "GET" && (request.URL.Path == "/" || request.URL.Path == "/manage" || request.URL.Path == "/manage/" || request.URL.Path == "/manage/app.js" || request.URL.Path == "/manage/style.css") {
		name, contentType := "index.html", "text/html; charset=utf-8"
		if strings.HasSuffix(request.URL.Path, "app.js") {
			name, contentType = "app.js", "text/javascript; charset=utf-8"
		}
		if strings.HasSuffix(request.URL.Path, "style.css") {
			name, contentType = "style.css", "text/css; charset=utf-8"
		}
		data, _ := assets.ReadFile("web/" + name)
		writer.Header().Set("Content-Type", contentType)
		_, _ = writer.Write(data)
		return
	}
	if request.Method == "POST" && request.Header.Get("Origin") != "http://"+console.address {
		failure(writer, 403, "invalid_origin")
		return
	}
	if request.URL.Path == "/manage/api/login" && request.Method == "POST" {
		console.login(writer, request)
		return
	}
	console.mu.Lock()
	cookie, err := request.Cookie("floe_management")
	var current session
	if err == nil {
		current = console.sessions[digest(cookie.Value)]
	}
	authorized := current.expires.After(time.Now())
	console.mu.Unlock()
	if !authorized || (request.Method != "GET" && request.Header.Get("X-Floe-CSRF") != current.csrf) {
		failure(writer, 401, "unauthorized")
		return
	}
	console.manage(writer, request, current)
}

func (console *Console) login(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Token string `json:"token"`
	}
	if !decode(writer, request, &input) {
		failure(writer, 400, "validation")
		return
	}
	console.mu.Lock()
	defer console.mu.Unlock()
	now := time.Now()
	if now.Sub(console.loginWindow) > time.Minute {
		console.loginWindow, console.loginAttempts = now, 0
	}
	console.loginAttempts++
	if console.loginAttempts > 10 {
		failure(writer, 429, "try_later")
		return
	}
	if digest(input.Token) != console.adminHash {
		failure(writer, 401, "unauthorized")
		return
	}
	for key, value := range console.sessions {
		if !value.expires.After(now) {
			delete(console.sessions, key)
		}
	}
	if len(console.sessions) >= 8 {
		failure(writer, 429, "too_many_sessions")
		return
	}
	token := randomToken()
	console.sessions[digest(token)] = session{csrf: randomToken(), expires: now.Add(12 * time.Hour)}
	http.SetCookie(writer, &http.Cookie{Name: "floe_management", Value: token, Path: "/manage", HttpOnly: true, SameSite: http.SameSiteStrictMode, MaxAge: 43200})
	reply(writer, 200, map[string]bool{"ok": true})
}

func (console *Console) serveInference(writer http.ResponseWriter, request *http.Request) {
	if request.Header.Get("Origin") != "" {
		failure(writer, 403, "unauthorized")
		return
	}
	auth := request.Header.Get("Authorization")
	console.mu.Lock()
	allowed := false
	if strings.HasPrefix(auth, "Bearer ") {
		hash := digest(strings.TrimPrefix(auth, "Bearer "))
		for _, value := range console.state.Clients {
			if hash == value {
				allowed = true
			}
		}
	}
	gateway := console.gateway
	console.mu.Unlock()
	if !allowed {
		failure(writer, 401, "unauthorized")
		return
	}
	forward := request.Clone(request.Context())
	forward.Header.Set("Authorization", "Bearer "+console.internalToken)
	gateway.ServeHTTP(writer, forward)
}

func (console *Console) servePair(writer http.ResponseWriter, request *http.Request) {
	if request.Method != "POST" {
		failure(writer, 404, "not_found")
		return
	}
	var input struct {
		Proof string `json:"proof"`
	}
	if !decode(writer, request, &input) {
		failure(writer, 400, "validation")
		return
	}
	console.mu.Lock()
	defer console.mu.Unlock()
	now := time.Now()
	if request.URL.Path == "/pair/start" {
		if now.Sub(console.lastPair) < 10*time.Second || (console.pair != nil && console.pair.Expires.After(now)) {
			failure(writer, 429, "pairing_in_progress")
			return
		}
		if len(console.state.Clients) >= 16 {
			failure(writer, 409, "too_many_clients")
			return
		}
		console.lastPair = now
		console.pair = &pairing{ID: randomToken(), Code: strings.ToUpper(randomToken()[:8]), Expires: now.Add(5 * time.Minute), proof: randomToken()}
		reply(writer, 200, map[string]any{"id": console.pair.ID, "code": console.pair.Code, "proof": console.pair.proof, "expires": console.pair.Expires})
		return
	}
	if console.pair == nil || !console.pair.Expires.After(now) || digest(input.Proof) != digest(console.pair.proof) {
		failure(writer, 401, "pairing_expired")
		return
	}
	if request.URL.Path == "/pair/cancel" {
		console.pair = nil
		reply(writer, 200, map[string]bool{"ok": true})
		return
	}
	if request.URL.Path == "/pair/poll" {
		if console.pair.token == "" {
			reply(writer, 200, map[string]string{"status": "pending"})
			return
		}
		reply(writer, 200, map[string]string{"status": "approved", "token": console.pair.token, "client_id": console.pair.ID})
		return
	}
	failure(writer, 404, "not_found")
}

func (console *Console) manage(writer http.ResponseWriter, request *http.Request, current session) {
	if strings.HasPrefix(request.URL.Path, "/manage/api/codex/") && request.Method == "POST" {
		if console.runtime == nil {
			failure(writer, 503, "codex_unavailable")
			return
		}
		ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
		defer cancel()
		value, err := console.runtime.Action(ctx, strings.TrimPrefix(request.URL.Path, "/manage/api/codex/"))
		if err != nil {
			failure(writer, 502, "codex_unavailable")
			return
		}
		reply(writer, 200, value)
		return
	}
	if request.URL.Path == "/manage/api/test" && request.Method == "POST" {
		console.testTarget(writer, request)
		return
	}
	if request.URL.Path == "/manage/api/route" && request.Method == "POST" {
		console.mu.Lock()
		defer console.mu.Unlock()
		console.updateRoute(writer, request)
		return
	}
	console.mu.Lock()
	defer console.mu.Unlock()
	if request.URL.Path == "/manage/api/state" && request.Method == "GET" {
		targets := map[string]any{}
		for identifier, target := range console.state.Targets {
			available := !console.unavailable[identifier]
			if target.Provider == "codex_oauth" {
				available = available && console.runtime != nil && console.runtime.Ready()
			}
			targets[identifier] = map[string]any{"provider": target.Provider, "model": target.Model, "base_url": target.BaseURL, "has_credential": target.APIKeyEnv != "", "available": available, "requires_external_consent": target.Provider != "ollama"}
		}
		clients := []string{}
		for identifier := range console.state.Clients {
			clients = append(clients, identifier)
		}
		var pending *pairing
		if console.pair != nil && console.pair.token == "" && console.pair.Expires.After(time.Now()) {
			pending = console.pair
		}
		reply(writer, 200, map[string]any{"csrf": current.csrf, "targets": targets, "routes": console.state.Routes, "clients": clients, "pairing": pending, "address": "http://" + console.address})
		return
	}
	if request.Method != "POST" {
		failure(writer, 404, "not_found")
		return
	}
	if request.URL.Path == "/manage/api/logout" {
		cookie, _ := request.Cookie("floe_management")
		delete(console.sessions, digest(cookie.Value))
		http.SetCookie(writer, &http.Cookie{Name: "floe_management", Path: "/manage", MaxAge: -1, HttpOnly: true, SameSite: http.SameSiteStrictMode})
		reply(writer, 200, map[string]bool{"ok": true})
		return
	}
	if request.URL.Path == "/manage/api/target" {
		console.updateTarget(writer, request)
		return
	}
	var input struct {
		ID string `json:"id"`
	}
	if !decode(writer, request, &input) {
		failure(writer, 400, "validation")
		return
	}
	next := cloneState(console.state)
	switch request.URL.Path {
	case "/manage/api/pair/approve":
		if console.pair == nil || console.pair.ID != input.ID || !console.pair.Expires.After(time.Now()) || console.pair.token != "" {
			failure(writer, 409, "pairing_expired")
			return
		}
		token := randomToken()
		next.Clients[input.ID] = digest(token)
		if console.save(next) != nil {
			failure(writer, 500, "save_failed")
			return
		}
		console.state, console.pair.token = next, token
	case "/manage/api/pair/reject":
		if console.pair != nil && console.pair.ID == input.ID {
			console.pair = nil
		}
	case "/manage/api/client/delete":
		delete(next.Clients, input.ID)
		if console.save(next) != nil {
			failure(writer, 500, "save_failed")
			return
		}
		console.state = next
		if console.pair != nil && console.pair.ID == input.ID {
			console.pair = nil
		}
	case "/manage/api/target/delete":
		old := next.Targets[input.ID]
		delete(next.Targets, input.ID)
		for class, route := range next.Routes {
			if route.Target == input.ID {
				delete(next.Routes, class)
			}
		}
		if console.save(next) != nil {
			failure(writer, 500, "save_failed")
			return
		}
		console.state = next
		console.rebuild()
		if old.APIKeyEnv != "" && console.vault.Delete(old.APIKeyEnv) != nil {
			failure(writer, 500, "credential_cleanup_failed")
			return
		}
	default:
		failure(writer, 404, "not_found")
		return
	}
	reply(writer, 200, map[string]bool{"ok": true})
}

func (console *Console) updateRoute(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Class           string `json:"inference_class"`
		Target          string `json:"target"`
		ReasoningEffort string `json:"reasoning_effort"`
	}
	if !decode(writer, request, &input) || !inference.ValidClass(input.Class) {
		failure(writer, 400, "validation")
		return
	}
	next := cloneState(console.state)
	if input.Target == "" {
		delete(next.Routes, input.Class)
	} else {
		next.Routes[input.Class] = inference.Route{Target: input.Target, ReasoningEffort: input.ReasoningEffort}
		if _, err := inference.New(inference.Config{Targets: next.Targets, Routes: next.Routes}, console.internalToken, console.lookup, console.runtime); err != nil {
			failure(writer, 400, "invalid_route")
			return
		}
	}
	if console.save(next) != nil {
		failure(writer, 500, "save_failed")
		return
	}
	console.state = next
	console.rebuild()
	reply(writer, 200, map[string]bool{"ok": true})
}

var identifierPattern = regexp.MustCompile(`^[A-Za-z0-9_-]{1,64}$`)

func (console *Console) updateTarget(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		ID       string `json:"id"`
		Provider string `json:"provider"`
		BaseURL  string `json:"base_url"`
		Model    string `json:"model"`
		APIKey   string `json:"api_key"`
	}
	if !decode(writer, request, &input) || !identifierPattern.MatchString(input.ID) || len(input.APIKey) > 8192 || strings.ContainsAny(input.APIKey, "\r\n\x00") {
		failure(writer, 400, "validation")
		return
	}
	old := console.state.Targets[input.ID]
	if input.Provider == "codex_oauth" {
		input.BaseURL = "https://chatgpt.com/backend-api/codex"
		input.APIKey = ""
	}
	target := inference.Target{Provider: input.Provider, BaseURL: input.BaseURL, Model: input.Model}
	if input.APIKey != "" {
		target.APIKeyEnv = "FLOE_KEY_" + strings.ToUpper(randomToken())
	} else if old.Provider == target.Provider && old.BaseURL == target.BaseURL {
		target.APIKeyEnv = old.APIKeyEnv
	}
	lookup := console.lookup
	if input.APIKey != "" {
		lookup = func(string) string { return input.APIKey }
	}
	if _, err := inference.New(inference.Config{Targets: map[string]inference.Target{input.ID: target}}, console.internalToken, lookup, console.runtime); err != nil {
		failure(writer, 400, "invalid_target")
		return
	}
	next := cloneState(console.state)
	next.Targets[input.ID] = target
	if len(next.Targets) > 32 {
		failure(writer, 400, "too_many_targets")
		return
	}
	if input.APIKey != "" && console.vault.Put(target.APIKeyEnv, input.APIKey) != nil {
		failure(writer, 503, "credential_store_unavailable")
		return
	}
	if console.save(next) != nil {
		if input.APIKey != "" {
			_ = console.vault.Delete(target.APIKeyEnv)
		}
		failure(writer, 500, "save_failed")
		return
	}
	console.state = next
	console.rebuild()
	if old.APIKeyEnv != "" && old.APIKeyEnv != target.APIKeyEnv && console.vault.Delete(old.APIKeyEnv) != nil {
		failure(writer, 500, "credential_cleanup_failed")
		return
	}
	reply(writer, 200, map[string]bool{"ok": true})
}
