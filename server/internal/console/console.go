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

	"floe/server/internal/connectors/common"
	"floe/server/internal/inference"
)

//go:embed web/*
var assets embed.FS

type AuthRuntime interface {
	Action(context.Context, string) (any, error)
	inference.CodexClient
}

type ConnectorAuthRuntime interface {
	Action(context.Context, string) (any, error)
	ConnectionSnapshot() (any, error)
	ReadCommunicationView(string, int, int) (any, error)
	ReadLogisticsView(context.Context) (common.LogisticsView, error)
}

type ConnectorOAuthRuntime interface {
	Action(context.Context, string) (any, error)
}

type CommunicationRuntime interface {
	ConnectionSnapshot(context.Context) (any, error)
	ReadCommunicationView(context.Context, string, int, int) (any, error)
}

type CalendarRuntime interface {
	ConnectionSnapshot(context.Context) (any, error)
	ReadCalendarView(context.Context, time.Time, time.Time, string, int) (any, error)
}

type WorkContextRuntime interface {
	ConnectionSnapshot(context.Context) (any, error)
	ReadWorkContextView(context.Context) (common.WorkContextView, error)
}

type LogisticsViewReader interface {
	ReadLogisticsView(context.Context) (common.LogisticsView, error)
}

type LogisticsRuntime interface {
	LogisticsViewReader
	ConnectionSnapshot(context.Context) (any, error)
}

type DriveAuthRuntime interface {
	Action(context.Context, string) (any, error)
	Token(context.Context) (string, error)
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
	gmail                                        ConnectorAuthRuntime
	microsoftAuth                                ConnectorOAuthRuntime
	microsoftMail                                CommunicationRuntime
	work                                         []WorkContextRuntime
	logistics                                    []LogisticsRuntime
	driveAuth                                    DriveAuthRuntime
	calendarAuth                                 DriveAuthRuntime
	calendars                                    []CalendarRuntime
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

func (console *Console) SetWorkContext(runtime WorkContextRuntime) {
	console.mu.Lock()
	defer console.mu.Unlock()
	if runtime == nil {
		console.work = nil
	} else {
		console.work = []WorkContextRuntime{runtime}
	}
}

func (console *Console) SetLogistics(runtime LogisticsRuntime) {
	console.mu.Lock()
	defer console.mu.Unlock()
	if runtime == nil {
		console.logistics = nil
	} else {
		console.logistics = []LogisticsRuntime{runtime}
	}
}

func (console *Console) SetDriveAuth(runtime DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	console.driveAuth = runtime
	return console.rebuildConnectorRuntimes()
}

func (console *Console) SetCalendarAuth(runtime DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	console.calendarAuth = runtime
	return console.rebuildConnectorRuntimes()
}

func (console *Console) SetGmailAuth(runtime ConnectorAuthRuntime) {
	console.mu.Lock()
	defer console.mu.Unlock()
	console.gmail = runtime
}

func (console *Console) SetMicrosoftMail(auth ConnectorOAuthRuntime, runtime CommunicationRuntime) {
	console.mu.Lock()
	defer console.mu.Unlock()
	console.microsoftAuth = auth
	console.microsoftMail = runtime
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
	if err := console.rebuildConnectorRuntimes(); err != nil {
		return nil, errors.New("invalid connector configuration")
	}
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
	for _, profile := range console.state.Providers {
		if profile.APIKeyEnv != "" {
			secrets[profile.APIKeyEnv] = console.lookup(profile.APIKeyEnv)
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
	for provider, profile := range console.state.Providers {
		for class := range profile.Classes {
			identifier := profileTargetID(provider, class)
			target := console.profileTarget(provider, class, profile)
			if _, err := inference.New(inference.Config{Targets: map[string]inference.Target{identifier: target}}, console.internalToken, lookup, console.runtime); err != nil {
				console.unavailable[identifier] = true
			} else {
				config.Targets[identifier] = target
			}
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
	gmail := console.gmail
	microsoftMail := console.microsoftMail
	work := append([]WorkContextRuntime(nil), console.work...)
	logistics := append([]LogisticsRuntime(nil), console.logistics...)
	calendars := append([]CalendarRuntime(nil), console.calendars...)
	console.mu.Unlock()
	if !allowed {
		failure(writer, 401, "unauthorized")
		return
	}
	if request.URL.Path == "/v1/connections" {
		if request.Method != http.MethodGet {
			failure(writer, 404, "not_found")
			return
		}
		connections := []any{}
		if gmail != nil {
			snapshot, err := gmail.ConnectionSnapshot()
			if err != nil {
				failure(writer, 503, "connections_unavailable")
				return
			}
			connections = append(connections, snapshot)
		}
		if microsoftMail != nil {
			snapshot, err := microsoftMail.ConnectionSnapshot(request.Context())
			if err != nil {
				failure(writer, 503, "connections_unavailable")
				return
			}
			connections = append(connections, snapshot)
		}
		runtimes := make([]interface {
			ConnectionSnapshot(context.Context) (any, error)
		}, 0, len(work)+len(logistics)+len(calendars))
		for _, runtime := range work {
			runtimes = append(runtimes, runtime)
		}
		for _, runtime := range logistics {
			runtimes = append(runtimes, runtime)
		}
		for _, runtime := range calendars {
			runtimes = append(runtimes, runtime)
		}
		for _, runtime := range runtimes {
			if runtime == nil {
				continue
			}
			snapshot, err := runtime.ConnectionSnapshot(request.Context())
			if err != nil {
				failure(writer, 503, "connections_unavailable")
				return
			}
			connections = append(connections, snapshot)
		}
		reply(writer, 200, map[string]any{"schema_version": 1, "connections": connections})
		return
	}
	if request.URL.Path == "/v1/views/mail.communication" {
		if request.Method != http.MethodPost || gmail == nil && microsoftMail == nil {
			failure(writer, 404, "not_found")
			return
		}
		var input struct {
			SchemaVersion int    `json:"schema_version"`
			Query         string `json:"query"`
			Cursor        int    `json:"cursor"`
			Limit         int    `json:"limit"`
		}
		if !decode(writer, request, &input) || input.SchemaVersion != 1 || len(input.Query) > 512 || input.Cursor < 0 || input.Limit < 1 || input.Limit > 100 {
			failure(writer, 400, "validation")
			return
		}
		var view any
		var err error
		if gmail != nil {
			view, err = gmail.ReadCommunicationView(input.Query, input.Cursor, input.Limit)
		}
		if (gmail == nil || err != nil) && microsoftMail != nil {
			view, err = microsoftMail.ReadCommunicationView(request.Context(), input.Query, input.Cursor, input.Limit)
		}
		if err != nil {
			failure(writer, 503, "view_unavailable")
			return
		}
		reply(writer, 200, map[string]any{"schema_version": 1, "view": view})
		return
	}
	if request.URL.Path == "/v1/views/calendar.timeline" {
		if request.Method != http.MethodPost || len(calendars) == 0 {
			failure(writer, 404, "not_found")
			return
		}
		var input struct {
			SchemaVersion    int    `json:"schema_version"`
			RangeStartUnixMS int64  `json:"range_start_unix_ms"`
			RangeEndUnixMS   int64  `json:"range_end_unix_ms"`
			Cursor           string `json:"cursor"`
			Limit            int    `json:"limit"`
		}
		if !decode(writer, request, &input) || input.SchemaVersion != 1 || input.RangeStartUnixMS < 0 || input.RangeEndUnixMS <= input.RangeStartUnixMS || input.RangeEndUnixMS-input.RangeStartUnixMS > int64(32*24*time.Hour/time.Millisecond) || len(input.Cursor) > 2048 || strings.ContainsAny(input.Cursor, "\r\n\x00") || input.Limit < 1 || input.Limit > 128 {
			failure(writer, 400, "validation")
			return
		}
		view, err := calendars[0].ReadCalendarView(request.Context(), time.UnixMilli(input.RangeStartUnixMS), time.UnixMilli(input.RangeEndUnixMS), input.Cursor, input.Limit)
		if err != nil {
			failure(writer, 503, "view_unavailable")
			return
		}
		reply(writer, 200, map[string]any{"schema_version": 1, "view": view})
		return
	}
	if request.URL.Path == "/v1/views/work.context" {
		console.serveWorkContextView(writer, request, work)
		return
	}
	if request.URL.Path == "/v1/views/life.logistics" {
		readers := make([]LogisticsViewReader, 0, len(logistics)+1)
		if gmail != nil {
			readers = append(readers, gmail)
		}
		for _, runtime := range logistics {
			readers = append(readers, runtime)
		}
		console.serveLogisticsView(writer, request, readers)
		return
	}
	forward := request.Clone(request.Context())
	forward.Header.Set("Authorization", "Bearer "+console.internalToken)
	gateway.ServeHTTP(writer, forward)
}

func (console *Console) serveLogisticsView(writer http.ResponseWriter, request *http.Request, runtimes []LogisticsViewReader) {
	if request.Method != http.MethodPost || len(runtimes) == 0 {
		failure(writer, 404, "not_found")
		return
	}
	var input struct {
		SchemaVersion int `json:"schema_version"`
	}
	if !decode(writer, request, &input) || input.SchemaVersion != 1 {
		failure(writer, 400, "validation")
		return
	}
	views := make([]common.LogisticsView, 0, len(runtimes))
	for _, runtime := range runtimes {
		view, err := runtime.ReadLogisticsView(request.Context())
		if err == nil {
			views = append(views, view)
		}
	}
	if len(views) == 0 {
		failure(writer, 503, "view_unavailable")
		return
	}
	view, err := common.MergeLogisticsViews(views, time.Now().UnixMilli())
	if err != nil {
		failure(writer, 503, "view_unavailable")
		return
	}
	reply(writer, 200, map[string]any{"schema_version": 1, "view": view})
}

func (console *Console) serveWorkContextView(writer http.ResponseWriter, request *http.Request, runtimes []WorkContextRuntime) {
	if request.Method != http.MethodPost || len(runtimes) == 0 {
		failure(writer, 404, "not_found")
		return
	}
	var input struct {
		SchemaVersion int `json:"schema_version"`
	}
	if !decode(writer, request, &input) || input.SchemaVersion != 1 {
		failure(writer, 400, "validation")
		return
	}
	views := make([]common.WorkContextView, 0, len(runtimes))
	for _, runtime := range runtimes {
		view, err := runtime.ReadWorkContextView(request.Context())
		if err == nil {
			views = append(views, view)
		}
	}
	if len(views) == 0 {
		failure(writer, 503, "view_unavailable")
		return
	}
	view, err := common.MergeWorkContextViews(views, time.Now().UnixMilli())
	if err != nil {
		failure(writer, 503, "view_unavailable")
		return
	}
	reply(writer, 200, map[string]any{"schema_version": 1, "view": view})
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
	if strings.HasPrefix(request.URL.Path, "/manage/api/calendar/") && request.Method == "POST" {
		console.mu.Lock()
		runtime := console.calendarAuth
		console.mu.Unlock()
		if runtime == nil {
			failure(writer, 503, "calendar_unavailable")
			return
		}
		ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
		defer cancel()
		value, err := runtime.Action(ctx, strings.TrimPrefix(request.URL.Path, "/manage/api/calendar/"))
		if err != nil {
			failure(writer, 502, "calendar_unavailable")
			return
		}
		reply(writer, 200, value)
		return
	}
	if strings.HasPrefix(request.URL.Path, "/manage/api/microsoft-mail/") && request.Method == "POST" {
		console.mu.Lock()
		runtime := console.microsoftAuth
		console.mu.Unlock()
		if runtime == nil {
			failure(writer, 503, "microsoft_mail_unavailable")
			return
		}
		ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
		defer cancel()
		value, err := runtime.Action(ctx, strings.TrimPrefix(request.URL.Path, "/manage/api/microsoft-mail/"))
		if err != nil {
			failure(writer, 502, "microsoft_mail_unavailable")
			return
		}
		reply(writer, 200, value)
		return
	}
	if strings.HasPrefix(request.URL.Path, "/manage/api/gmail/") && request.Method == "POST" {
		console.mu.Lock()
		runtime := console.gmail
		console.mu.Unlock()
		if runtime == nil {
			failure(writer, 503, "gmail_unavailable")
			return
		}
		ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
		defer cancel()
		value, err := runtime.Action(ctx, strings.TrimPrefix(request.URL.Path, "/manage/api/gmail/"))
		if err != nil {
			failure(writer, 502, "gmail_unavailable")
			return
		}
		reply(writer, 200, value)
		return
	}
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
	if strings.HasPrefix(request.URL.Path, "/manage/api/drive/") && request.Method == "POST" {
		console.mu.Lock()
		runtime := console.driveAuth
		console.mu.Unlock()
		if runtime == nil {
			failure(writer, 503, "drive_unavailable")
			return
		}
		ctx, cancel := context.WithTimeout(request.Context(), 20*time.Second)
		defer cancel()
		value, err := runtime.Action(ctx, strings.TrimPrefix(request.URL.Path, "/manage/api/drive/"))
		if err != nil {
			failure(writer, 502, "drive_unavailable")
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
	if request.URL.Path == "/manage/api/provider" && request.Method == "POST" {
		console.mu.Lock()
		defer console.mu.Unlock()
		console.updateProvider(writer, request)
		return
	}
	if request.URL.Path == "/manage/api/state" && request.Method == "GET" {
		console.writeState(writer, current)
		return
	}
	if request.URL.Path == "/manage/api/connector/github" && request.Method == "POST" {
		console.mu.Lock()
		defer console.mu.Unlock()
		console.updateGitHubConnector(writer, request)
		return
	}
	if request.URL.Path == "/manage/api/connector/home-assistant" && request.Method == "POST" {
		console.mu.Lock()
		defer console.mu.Unlock()
		console.updateHomeAssistantConnector(writer, request)
		return
	}
	if request.URL.Path == "/manage/api/connector/slack" && request.Method == "POST" {
		console.mu.Lock()
		defer console.mu.Unlock()
		console.updateSlackConnector(writer, request)
		return
	}
	if request.URL.Path == "/manage/api/connector/google-drive" && request.Method == "POST" {
		console.mu.Lock()
		defer console.mu.Unlock()
		console.updateGoogleDriveConnector(writer, request)
		return
	}
	if request.URL.Path == "/manage/api/connector/google-calendar" && request.Method == "POST" {
		console.mu.Lock()
		defer console.mu.Unlock()
		console.updateGoogleCalendarConnector(writer, request)
		return
	}
	console.mu.Lock()
	defer console.mu.Unlock()
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

func (console *Console) writeState(writer http.ResponseWriter, current session) {
	console.mu.Lock()
	state := cloneState(console.state)
	unavailable := make(map[string]bool, len(console.unavailable))
	for identifier, value := range console.unavailable {
		unavailable[identifier] = value
	}
	clients := make([]string, 0, len(state.Clients))
	for identifier := range state.Clients {
		clients = append(clients, identifier)
	}
	var pending *pairing
	if console.pair != nil && console.pair.token == "" && console.pair.Expires.After(time.Now()) {
		copy := *console.pair
		pending = &copy
	}
	runtime, gateway, address := console.runtime, console.gateway, console.address
	console.mu.Unlock()

	providers := map[string]any{}
	for provider, profile := range state.Providers {
		classes := map[string]any{}
		for class, configured := range profile.Classes {
			identifier := profileTargetID(provider, class)
			available := !unavailable[identifier]
			if provider == "codex_oauth" {
				available = available && runtime != nil && runtime.Ready()
			}
			classes[class] = map[string]any{"model": configured.Model, "reasoning_effort": configured.ReasoningEffort, "active": state.Routes[class].Target == identifier, "available": available}
		}
		providers[provider] = map[string]any{"base_url": profile.BaseURL, "has_credential": profile.APIKeyEnv != "", "classes": classes}
	}
	connectors := map[string]any{
		"github":          map[string]any{"configured": state.Connectors.GitHub != nil},
		"slack":           map[string]any{"configured": state.Connectors.Slack != nil},
		"google_drive":    map[string]any{"configured": state.Connectors.GoogleDrive != nil},
		"google_calendar": map[string]any{"configured": state.Connectors.GoogleCalendar != nil},
		"home_assistant":  map[string]any{"configured": state.Connectors.HomeAssistant != nil},
	}
	reply(writer, 200, map[string]any{"csrf": current.csrf, "providers": providers, "connectors": connectors, "clients": clients, "pairing": pending, "address": "http://" + address, "traces": gateway.Traces(20)})
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
