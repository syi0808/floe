package console

import (
	"context"
	"crypto/ed25519"
	"embed"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"mime"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"floe/server/internal/authorization"
	"floe/server/internal/connectors/common"
	"floe/server/internal/inference"
)

var personIDPattern = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$`)
var deviceIDPattern = regexp.MustCompile(`^[A-Za-z0-9._:-]{1,128}$`)
var connectorIDPattern = regexp.MustCompile(`^[A-Za-z0-9._:-]{1,128}$`)
var connectionIDPattern = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-4[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$`)

func validPersonID(value string) bool     { return personIDPattern.MatchString(value) }
func validDeviceID(value string) bool     { return deviceIDPattern.MatchString(value) }
func validConnectionID(value string) bool { return connectionIDPattern.MatchString(value) }

func connectionSnapshotMetadata(snapshot any) (map[string]any, string, string, bool) {
	encoded, err := json.Marshal(snapshot)
	if err != nil {
		return nil, "", "", false
	}
	var value map[string]any
	if json.Unmarshal(encoded, &value) != nil {
		return nil, "", "", false
	}
	connection, ok := value["connection"].(map[string]any)
	if !ok {
		return nil, "", "", false
	}
	connectorID, ok := connection["connector_id"].(string)
	if !ok || connectorID == "" {
		return nil, "", "", false
	}
	deviceID := ""
	if descriptor, ok := value["descriptor"].(map[string]any); ok {
		if execution, ok := descriptor["execution"].(map[string]any); ok && execution["kind"] == "device" {
			deviceID, _ = execution["device_id"].(string)
		}
	}
	return value, connectorID, deviceID, true
}

func (console *Console) ownedConnectionSnapshots(snapshots []any, scope clientScope) ([]any, error) {
	console.mu.Lock()
	defer console.mu.Unlock()
	owned := make([]any, 0, len(snapshots))
	for _, snapshot := range snapshots {
		value, connectorID, deviceID, ok := connectionSnapshotMetadata(snapshot)
		if !ok {
			return nil, errors.New("invalid connection ownership")
		}
		record, exists := console.connectionForPerson(connectorID, scope.PersonID)
		if !exists {
			continue
		}
		if record.Device != nil && (record.Device.DeviceID != scope.DeviceID || record.Device.DeviceID != deviceID) || record.Device == nil && deviceID != "" {
			return nil, errors.New("connection device binding mismatch")
		}
		connection := value["connection"].(map[string]any)
		connection["person_id"] = record.PersonID
		connection["connection_id"] = record.ConnectionID
		if record.Device != nil {
			connection["device_binding"] = map[string]any{"device_id": record.Device.DeviceID}
		}
		connection["authority"] = map[string]any{"execution_owner": console.state.ExecutionOwnerID, "incarnation": record.Incarnation, "epoch": record.Epoch, "identity_unverified": record.IdentityUnverified}
		owned = append(owned, value)
	}
	return owned, nil
}

//go:embed web/*
var assets embed.FS

type AuthRuntime interface {
	Action(context.Context, string) (any, error)
	inference.CodexClient
}

type ConnectorAuthRuntime interface {
	ConnectorOAuthRuntime
	ConnectionSnapshot() (any, error)
	ReadCommunicationView(string, int, int) (any, error)
	ReadLogisticsView(context.Context) (common.LogisticsView, error)
}

type ConnectorOAuthRuntime interface {
	Action(context.Context, string) (any, error)
	BindCredential(string) error
	Ready() bool
}

type ProviderIdentityRuntime interface {
	ProviderIdentity(context.Context) (string, error)
}

type ProviderIdentityStatusRuntime interface {
	ProviderIdentityStatus() (string, bool)
}

type ProviderIdentityFenceRuntime interface {
	WithVerifiedProviderIdentity(string, string, func() error) error
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
	ConnectorOAuthRuntime
	Token(context.Context) (string, error)
}

type session struct {
	csrf    string
	expires time.Time
}
type pairing struct {
	ID                  string    `json:"id"`
	Code                string    `json:"code"`
	Expires             time.Time `json:"expires"`
	PersonID            string    `json:"person_id"`
	DeviceID            string    `json:"device_id"`
	IssuerKeyID         string    `json:"issuer_key_id"`
	IssuerPublicKey     string    `json:"issuer_public_key"`
	IssuerFingerprint   string    `json:"issuer_fingerprint"`
	ProducerFingerprint string    `json:"producer_fingerprint"`
	ProducerAudience    string    `json:"producer_audience"`
	LocalConfirmed      bool      `json:"local_confirmed"`
	AdminApproved       bool      `json:"admin_approved"`
	status              string
	enrollmentID        string
	challengeID         string
	challengeBytes      []byte
	challengeB64        string
	producerSignature   []byte
	proof               string
	token               string
}

type clientScope struct {
	ClientID string
	PersonID string
	DeviceID string
}

type Console struct {
	mu                                           sync.Mutex
	directory, address, adminHash, internalToken string
	vault                                        Vault
	runtime                                      AuthRuntime
	gmail                                        ConnectorAuthRuntime
	microsoftAuth                                ConnectorOAuthRuntime
	microsoftMail                                CommunicationRuntime
	work                                         map[string]WorkContextRuntime
	logistics                                    map[string]LogisticsRuntime
	driveAuth                                    DriveAuthRuntime
	githubAuth                                   DriveAuthRuntime
	slackAuth                                    DriveAuthRuntime
	calendarAuth                                 DriveAuthRuntime
	microsoftCalendarAuth                        DriveAuthRuntime
	microsoftTeamsAuth                           DriveAuthRuntime
	calendars                                    map[string]CalendarRuntime
	state                                        diskState
	gateway                                      *inference.Gateway
	authorization                                *authorization.Engine
	trustUnavailable                             atomic.Bool
	producerUnavailable                          atomic.Bool
	producer                                     *producerIdentity
	unavailable                                  map[string]bool
	sessions                                     map[string]session
	pair                                         *pairing
	loginAttempts                                int
	loginWindow                                  time.Time
	lastPair                                     time.Time
	testActive                                   bool
	connectorAttempts                            map[string]*connectorAttempt
	connectorLifecycleMu                         sync.Mutex
	connectorLifecycles                          map[string]*sync.Mutex
	connectorReservations                        map[string]connectionRecord
	calendarAdmissions                           map[string]calendarAdmissionState
	remoteViewAdmissions                         map[string]remoteViewAdmissionState
	allowLegacyPairing                           bool
}

func (console *Console) authorityEngine() *authorization.Engine {
	if console.trustUnavailable.Load() {
		return nil
	}
	return console.authorization
}

func (console *Console) latchTrustUnavailable() {
	console.trustUnavailable.Store(true)
}

func (console *Console) SetWorkContext(runtime WorkContextRuntime) {
	console.mu.Lock()
	defer console.mu.Unlock()
	console.work = map[string]WorkContextRuntime{}
	if connectorID, ok := contextRuntimeConnectorID(runtime); ok {
		if record, exists := console.connectionForConnector(connectorID); exists {
			console.work[record.ConnectionID] = runtime
		}
	}
}

func (console *Console) SetLogistics(runtime LogisticsRuntime) {
	console.mu.Lock()
	defer console.mu.Unlock()
	console.logistics = map[string]LogisticsRuntime{}
	if connectorID, ok := contextRuntimeConnectorID(runtime); ok {
		if record, exists := console.connectionForConnector(connectorID); exists {
			console.logistics[record.ConnectionID] = runtime
		}
	}
}

func contextRuntimeConnectorID(runtime interface {
	ConnectionSnapshot(context.Context) (any, error)
}) (string, bool) {
	if runtime == nil {
		return "", false
	}
	snapshot, err := runtime.ConnectionSnapshot(context.Background())
	if err != nil {
		return "", false
	}
	_, connectorID, _, ok := connectionSnapshotMetadata(snapshot)
	return connectorID, ok
}

func (console *Console) SetDriveAuth(runtime DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("google_drive.files", runtime); err != nil {
		return err
	}
	console.driveAuth = runtime
	console.retryPendingCleanupsLocked()
	return console.rebuildConnectorRuntimes()
}

func (console *Console) SetCalendarAuth(runtime DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("calendar.google", runtime); err != nil {
		return err
	}
	console.calendarAuth = runtime
	console.retryPendingCleanupsLocked()
	return console.rebuildConnectorRuntimes()
}

func (console *Console) SetMicrosoftCalendarAuth(runtime DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("calendar.microsoft", runtime); err != nil {
		return err
	}
	console.microsoftCalendarAuth = runtime
	console.retryPendingCleanupsLocked()
	return console.rebuildConnectorRuntimes()
}

func (console *Console) SetMicrosoftTeamsAuth(runtime DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("microsoft.teams", runtime); err != nil {
		return err
	}
	console.microsoftTeamsAuth = runtime
	console.retryPendingCleanupsLocked()
	return console.rebuildConnectorRuntimes()
}

func (console *Console) SetGitHubAuth(runtime DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("github.issues", runtime); err != nil {
		if !console.hasLegacyConnectorCredential("github.issues", githubTokenKey) {
			return err
		}
	}
	console.githubAuth = runtime
	console.retryPendingCleanupsLocked()
	return console.rebuildConnectorRuntimes()
}

func (console *Console) SetSlackAuth(runtime DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("slack.conversations", runtime); err != nil {
		if !console.hasLegacyConnectorCredential("slack.conversations", slackTokenKey) {
			return err
		}
	}
	console.slackAuth = runtime
	console.retryPendingCleanupsLocked()
	return console.rebuildConnectorRuntimes()
}

func (console *Console) hasLegacyConnectorCredential(connectorID, namespace string) bool {
	for _, record := range console.state.Connections {
		if record.ConnectorID == connectorID && strings.HasPrefix(record.Credential, namespace+":") {
			return true
		}
	}
	return false
}

func (console *Console) SetGmailAuth(runtime ConnectorAuthRuntime) {
	console.mu.Lock()
	defer console.mu.Unlock()
	if console.bindConfiguredOAuthRuntime("gmail", runtime) != nil {
		console.gmail = nil
		return
	}
	console.gmail = runtime
	console.retryPendingCleanupsLocked()
}

func (console *Console) SetMicrosoftMail(auth ConnectorOAuthRuntime, runtime CommunicationRuntime) {
	console.mu.Lock()
	defer console.mu.Unlock()
	if console.bindConfiguredOAuthRuntime("microsoft.mail", auth) != nil {
		console.microsoftAuth, console.microsoftMail = nil, nil
		return
	}
	console.microsoftAuth = auth
	console.microsoftMail = runtime
	console.retryPendingCleanupsLocked()
}

func (console *Console) bindConfiguredOAuthRuntime(connectorID string, runtime ConnectorOAuthRuntime) error {
	_, exists := clientConnectorDefinitionFor(connectorID)
	if !exists || runtime == nil {
		return nil
	}
	for _, record := range console.state.Connections {
		if record.ConnectorID == connectorID {
			return bindClientOAuthCredential(runtime, record)
		}
	}
	for _, cleanup := range console.state.Cleanups {
		for _, step := range cleanup.Connections {
			if step.ConnectorID == connectorID && !step.RuntimeComplete {
				return runtime.BindCredential(step.Credential)
			}
		}
	}
	return nil
}

func (console *Console) retryPendingCleanupsLocked() {
	for personID := range console.state.Cleanups {
		_ = console.retryPersonCleanupLocked(personID)
	}
}

func New(directory, address string, vault Vault, runtime AuthRuntime) (*Console, error) {
	host, port, err := net.SplitHostPort(address)
	if err != nil || host != "127.0.0.1" || port == "" {
		return nil, errors.New("console requires 127.0.0.1:port")
	}
	_, stateError := os.Stat(filepath.Join(directory, "state.json"))
	stateExists := stateError == nil
	state, admin, err := readState(directory)
	if err != nil {
		return nil, err
	}
	console := &Console{directory: directory, address: address, adminHash: digest(admin), internalToken: randomToken(), vault: vault, runtime: runtime, state: state, sessions: map[string]session{}, connectorAttempts: map[string]*connectorAttempt{}, connectorLifecycles: map[string]*sync.Mutex{}, connectorReservations: map[string]connectionRecord{}, remoteViewAdmissions: map[string]remoteViewAdmissionState{}}
	producer, producerError := loadProducerIdentity(filepath.Join(directory, "producer-identity.json"), !stateExists)
	if producerError != nil {
		console.producerUnavailable.Store(true)
	} else {
		console.producer = producer
	}
	if !stateExists {
		if saveError := console.save(state); saveError != nil {
			return nil, errors.New("initial server state unavailable")
		}
	}
	var engine *authorization.Engine
	if !state.TrustCorrupt {
		engine, err = authorization.New(authorization.Options{Store: consoleTrustStore{console: console}})
	}
	if err != nil {
		engine = nil
		console.trustUnavailable.Store(true)
	}
	if state.TrustCorrupt {
		console.trustUnavailable.Store(true)
	}
	console.authorization = engine
	console.rebuild()
	if err := console.recoverConnectionAttemptsLocked(); err != nil {
		return nil, errors.New("connection attempt recovery unavailable")
	}
	console.retryPendingCleanupsLocked()
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
	var scope clientScope
	if strings.HasPrefix(auth, "Bearer ") {
		hash := digest(strings.TrimPrefix(auth, "Bearer "))
		for identifier, value := range console.state.Clients {
			if hash == value.TokenHash {
				scope = clientScope{ClientID: identifier, PersonID: value.PersonID, DeviceID: value.DeviceID}
			}
		}
	}
	gateway := console.gateway
	gmail := console.gmail
	microsoftMail := console.microsoftMail
	work := make(map[string]WorkContextRuntime, len(console.work))
	for connectorID, runtime := range console.work {
		work[connectorID] = runtime
	}
	logistics := make(map[string]LogisticsRuntime, len(console.logistics))
	for connectorID, runtime := range console.logistics {
		logistics[connectorID] = runtime
	}
	calendars := make(map[string]CalendarRuntime, len(console.calendars))
	for connectorID, runtime := range console.calendars {
		calendars[connectorID] = runtime
	}
	connectionRecords := make(map[string]connectionRecord, len(console.state.Connections))
	for connectionID, record := range console.state.Connections {
		connectionRecords[connectionID] = record
	}
	console.mu.Unlock()
	if !validPersonID(scope.PersonID) || !validDeviceID(scope.DeviceID) {
		failure(writer, 401, "unauthorized")
		return
	}
	console.mu.Lock()
	_, cleanupPending := console.state.Cleanups[scope.PersonID]
	console.mu.Unlock()
	if cleanupPending {
		failure(writer, 503, "person_cleanup_pending")
		return
	}
	if strings.HasPrefix(request.URL.Path, "/v1/authority/enrollment") || request.URL.Path == "/v1/authority/producer" || request.URL.Path == "/v1/authority/calendar/source" {
		console.serveAuthority(writer, request, scope)
		return
	}
	if strings.HasPrefix(request.URL.Path, "/v1/connectors") {
		console.serveClientConnectors(writer, request, scope)
		return
	}
	if request.URL.Path == "/v1/connections" {
		if request.Method != http.MethodGet {
			failure(writer, 404, "not_found")
			return
		}
		connections := []any{}
		if gmail != nil && connectionOwnedBy(connectionRecords, "gmail", scope) {
			snapshot, err := gmail.ConnectionSnapshot()
			if err != nil {
				failure(writer, 503, "connections_unavailable")
				return
			}
			connections = append(connections, snapshot)
		}
		if microsoftMail != nil && connectionOwnedBy(connectionRecords, "microsoft.mail", scope) {
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
		for connectionID, runtime := range work {
			if runtimeConnectionOwnedBy(connectionRecords, connectionID, scope) {
				runtimes = append(runtimes, runtime)
			}
		}
		for connectionID, runtime := range logistics {
			if runtimeConnectionOwnedBy(connectionRecords, connectionID, scope) {
				runtimes = append(runtimes, runtime)
			}
		}
		for connectionID, runtime := range calendars {
			if runtimeConnectionOwnedBy(connectionRecords, connectionID, scope) {
				runtimes = append(runtimes, runtime)
			}
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
		var err error
		connections, err = console.ownedConnectionSnapshots(connections, scope)
		if err != nil {
			failure(writer, 503, "connection_scope_unavailable")
			return
		}
		reply(writer, 200, map[string]any{"schema_version": 1, "person_id": scope.PersonID, "device_id": scope.DeviceID, "connections": connections})
		return
	}
	if request.URL.Path == "/v1/views/mail.communication" {
		failure(writer, http.StatusBadRequest, "admission_required")
		return
	}
	if strings.HasPrefix(request.URL.Path, "/v1/views/calendar.timeline") {
		if request.URL.Path == "/v1/views/calendar.timeline/source-preview" {
			console.serveRemoteViewSourcePreview(writer, request, "calendar.timeline", scope, connectionRecords)
			return
		}
		console.serveCalendarAuthority(writer, request, scope, calendars, connectionRecords)
		return
	}
	if request.URL.Path == "/v1/views/mail.communication/source-preview" || request.URL.Path == "/v1/views/work.context/source-preview" || request.URL.Path == "/v1/views/life.logistics/source-preview" {
		viewID, _, _ := remoteViewRoute(strings.TrimSuffix(request.URL.Path, "/source-preview") + "/admit")
		console.serveRemoteViewSourcePreview(writer, request, viewID, scope, connectionRecords)
		return
	}
	if strings.HasSuffix(request.URL.Path, "/admit") || strings.HasSuffix(request.URL.Path, "/read") || strings.HasSuffix(request.URL.Path, "/release") {
		if strings.HasPrefix(request.URL.Path, "/v1/views/mail.communication/") || strings.HasPrefix(request.URL.Path, "/v1/views/work.context/") || strings.HasPrefix(request.URL.Path, "/v1/views/life.logistics/") {
			communication := make([]CommunicationRuntime, 0, 2)
			if gmail != nil && connectionOwnedBy(connectionRecords, "gmail", scope) {
				communication = append(communication, legacyCommunicationAdapter{runtime: gmail})
			}
			if microsoftMail != nil && connectionOwnedBy(connectionRecords, "microsoft.mail", scope) {
				communication = append(communication, microsoftMail)
			}
			workRuntimes := make([]WorkContextRuntime, 0, len(work))
			for connectionID, runtime := range work {
				if runtimeConnectionOwnedBy(connectionRecords, connectionID, scope) {
					workRuntimes = append(workRuntimes, runtime)
				}
			}
			logisticsRuntimes := make([]LogisticsRuntime, 0, len(logistics))
			for connectionID, runtime := range logistics {
				if runtimeConnectionOwnedBy(connectionRecords, connectionID, scope) {
					logisticsRuntimes = append(logisticsRuntimes, runtime)
				}
			}
			console.serveRemoteViewAuthority(writer, request, request.URL.Path, authorization.Principal{ClientID: scope.ClientID, PersonID: scope.PersonID, DeviceID: scope.DeviceID, Authenticated: true}, connectionRecords, communication, workRuntimes, logisticsRuntimes)
			return
		}
	}
	if request.URL.Path == "/v1/views/work.context" {
		failure(writer, http.StatusBadRequest, "admission_required")
		return
	}
	if request.URL.Path == "/v1/views/life.logistics" {
		failure(writer, http.StatusBadRequest, "admission_required")
		return
	}
	forward := request.Clone(request.Context())
	forward.Header.Set("Authorization", "Bearer "+console.internalToken)
	gateway.ServeHTTP(writer, forward)
}

func connectionOwnedBy(connections map[string]connectionRecord, connectorID string, scope clientScope) bool {
	for _, record := range connections {
		if record.ConnectorID == connectorID && record.PersonID == scope.PersonID && (record.Device == nil || record.Device.DeviceID == scope.DeviceID) {
			return true
		}
	}
	return false
}

func runtimeConnectionOwnedBy(connections map[string]connectionRecord, connectionID string, scope clientScope) bool {
	record, exists := connections[connectionID]
	return exists && record.PersonID == scope.PersonID && (record.Device == nil || record.Device.DeviceID == scope.DeviceID)
}

func (console *Console) servePair(writer http.ResponseWriter, request *http.Request) {
	if request.Method != "POST" {
		failure(writer, 404, "not_found")
		return
	}
	var input struct {
		SchemaVersion   int    `json:"schema_version"`
		PairingID       string `json:"pairing_id"`
		Proof           string `json:"proof"`
		PersonID        string `json:"person_id"`
		DeviceID        string `json:"device_id"`
		IssuerKeyID     string `json:"issuer_key_id"`
		IssuerPublicKey string `json:"issuer_public_key"`
		ChallengeID     string `json:"challenge_id"`
		KeyID           string `json:"key_id"`
		Signature       string `json:"signature"`
	}
	if !decode(writer, request, &input) {
		failure(writer, 400, "validation")
		return
	}
	if request.URL.Path == "/pair/start" && console.allowLegacyPairing && input.SchemaVersion == 0 && input.IssuerKeyID == "" && input.IssuerPublicKey == "" {
		if !validPersonID(input.PersonID) || !validDeviceID(input.DeviceID) {
			failure(writer, 400, "identity_required")
			return
		}
		console.mu.Lock()
		now := time.Now()
		for personID := range console.state.Cleanups {
			if err := console.retryPersonCleanupLocked(personID); err != nil {
				console.mu.Unlock()
				failure(writer, 503, "person_cleanup_pending")
				return
			}
		}
		if now.Sub(console.lastPair) < 10*time.Second || (console.pair != nil && console.pair.Expires.After(now) && console.pair.status != "rejected") {
			console.mu.Unlock()
			failure(writer, 429, "pairing_in_progress")
			return
		}
		if len(console.state.Clients) >= 16 {
			console.mu.Unlock()
			failure(writer, 409, "too_many_clients")
			return
		}
		for personID := range console.state.Clients {
			if console.state.Clients[personID].PersonID != input.PersonID {
				console.mu.Unlock()
				failure(writer, 409, "person_mismatch")
				return
			}
		}
		console.lastPair = now
		console.pair = &pairing{ID: randomToken(), Code: strings.ToUpper(randomToken()[:8]), Expires: now.Add(5 * time.Minute), PersonID: input.PersonID, DeviceID: input.DeviceID, proof: randomToken()}
		pending := *console.pair
		console.mu.Unlock()
		reply(writer, 200, map[string]any{"id": pending.ID, "code": pending.Code, "proof": pending.proof, "expires": pending.Expires})
		return
	}
	if request.URL.Path == "/pair/start" {
		if input.SchemaVersion != 1 || !validPersonID(input.PersonID) || !validDeviceID(input.DeviceID) || !validConnectionID(input.IssuerKeyID) {
			failure(writer, 400, "identity_required")
			return
		}
		publicKey, err := base64.RawURLEncoding.DecodeString(input.IssuerPublicKey)
		if err != nil || len(publicKey) != ed25519.PublicKeySize || base64.RawURLEncoding.EncodeToString(publicKey) != input.IssuerPublicKey {
			failure(writer, 400, "validation")
			return
		}
		metadata, err := console.producerMetadata()
		if err != nil {
			failure(writer, 503, "producer_unavailable")
			return
		}
		audience, ok := metadata["audience"].(string)
		if !ok || audience == "" {
			failure(writer, 503, "producer_unavailable")
			return
		}
		console.mu.Lock()
		now := time.Now()
		for personID := range console.state.Cleanups {
			if err := console.retryPersonCleanupLocked(personID); err != nil {
				console.mu.Unlock()
				failure(writer, 503, "person_cleanup_pending")
				return
			}
		}
		if now.Sub(console.lastPair) < 10*time.Second || (console.pair != nil && console.pair.Expires.After(now) && console.pair.status != "rejected") {
			console.mu.Unlock()
			failure(writer, 429, "pairing_in_progress")
			return
		}
		if len(console.state.Clients) >= 16 {
			console.mu.Unlock()
			failure(writer, 409, "too_many_clients")
			return
		}
		for _, client := range console.state.Clients {
			if client.PersonID != input.PersonID {
				console.mu.Unlock()
				failure(writer, 409, "person_mismatch")
				return
			}
		}
		console.lastPair = now
		pairingID, err := newConnectionID()
		if err != nil {
			failure(writer, http.StatusServiceUnavailable, "pairing_unavailable")
			return
		}
		pollingProof := randomToken()
		console.mu.Unlock()
		principal := authorization.Principal{ClientID: pairingID, PersonID: input.PersonID, DeviceID: input.DeviceID, Authenticated: true}
		engine := console.authorityEngine()
		if engine == nil {
			failure(writer, 503, "authority_unavailable")
			return
		}
		enrollment, challenge, err := engine.BeginEnrollment(principal, input.IssuerKeyID, ed25519.PublicKey(publicKey), audience)
		if err != nil {
			failure(writer, 409, "pairing_denied")
			return
		}
		producerFingerprint, _ := metadata["fingerprint"].(string)
		pending := &pairing{
			ID: pairingID, Code: strings.ToUpper(randomToken()[:8]), Expires: challenge.ExpiresAt,
			PersonID: input.PersonID, DeviceID: input.DeviceID, IssuerKeyID: enrollment.KeyID,
			IssuerPublicKey:   base64.RawURLEncoding.EncodeToString(publicKey),
			IssuerFingerprint: enrollment.Fingerprint, ProducerFingerprint: producerFingerprint, ProducerAudience: audience,
			enrollmentID: enrollment.ID, challengeID: challenge.ID, challengeBytes: append([]byte(nil), challenge.Bytes...),
			challengeB64: challenge.BytesB64, producerSignature: console.producer.signChallenge(challenge.Bytes), proof: pollingProof,
		}
		console.mu.Lock()
		if console.pair != nil && console.pair.Expires.After(time.Now()) {
			console.mu.Unlock()
			_ = engine.ApproveEnrollment(enrollment.ID, enrollment.Fingerprint, false)
			failure(writer, 429, "pairing_in_progress")
			return
		}
		console.pair = pending
		console.mu.Unlock()
		reply(writer, 200, map[string]any{
			"schema_version": 1, "pairing_id": pending.ID, "code": pending.Code, "proof": pending.proof,
			"expires_at_unix_ms": pending.Expires.UnixMilli(), "person_id": pending.PersonID, "device_id": pending.DeviceID,
			"producer":     metadata,
			"issuer":       map[string]any{"key_id": pending.IssuerKeyID, "public_key": pending.IssuerPublicKey, "fingerprint": pending.IssuerFingerprint},
			"challenge_id": pending.challengeID, "challenge_b64url": pending.challengeB64,
			"producer_signature": base64.RawURLEncoding.EncodeToString(pending.producerSignature),
		})
		return
	}
	console.mu.Lock()
	now := time.Now()
	if console.pair == nil || digest(input.Proof) != digest(console.pair.proof) {
		console.mu.Unlock()
		failure(writer, 401, "pairing_expired")
		return
	}
	if console.pair.IssuerKeyID != "" && (request.URL.Path == "/pair/poll" || request.URL.Path == "/pair/cancel") && (input.SchemaVersion != 1 || input.PairingID != console.pair.ID) {
		console.mu.Unlock()
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	if !console.pair.Expires.After(now) {
		if request.URL.Path == "/pair/poll" {
			pending := *console.pair
			console.mu.Unlock()
			reply(writer, 200, map[string]any{"schema_version": 1, "pairing_id": pending.ID, "status": "expired", "person_id": pending.PersonID, "device_id": pending.DeviceID})
			return
		}
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "pairing_expired")
		return
	}
	if request.URL.Path == "/pair/cancel" {
		enrollmentID, fingerprint := console.pair.enrollmentID, console.pair.IssuerFingerprint
		console.pair = nil
		console.mu.Unlock()
		if engine := console.authorityEngine(); engine != nil {
			_ = engine.ApproveEnrollment(enrollmentID, fingerprint, false)
		}
		reply(writer, 200, map[string]bool{"ok": true})
		return
	}
	if request.URL.Path == "/pair/confirm" {
		pending := *console.pair
		console.mu.Unlock()
		if input.SchemaVersion != 1 || input.PairingID != pending.ID || input.ChallengeID != pending.challengeID || input.KeyID != pending.IssuerKeyID || input.Signature == "" {
			failure(writer, 400, "validation")
			return
		}
		engine := console.authorityEngine()
		if engine == nil {
			failure(writer, 503, "authority_unavailable")
			return
		}
		principal := authorization.Principal{ClientID: pending.ID, PersonID: pending.PersonID, DeviceID: pending.DeviceID, Authenticated: true}
		if err := engine.CompleteEnrollment(pending.enrollmentID, principal, authorization.Proof{ChallengeID: input.ChallengeID, KeyID: input.KeyID, Signature: input.Signature}); err != nil {
			failure(writer, 403, "pairing_denied")
			return
		}
		console.mu.Lock()
		if console.pair == nil || console.pair.ID != pending.ID {
			console.mu.Unlock()
			_ = engine.ApproveEnrollment(pending.enrollmentID, pending.IssuerFingerprint, false)
			failure(writer, 409, "pairing_expired")
			return
		}
		console.pair.LocalConfirmed = true
		console.pair.status = "local_confirmed"
		console.mu.Unlock()
		reply(writer, 200, map[string]any{"schema_version": 1, "pairing_id": pending.ID, "status": "local_confirmed"})
		return
	}
	if request.URL.Path == "/pair/poll" {
		pending := *console.pair
		console.mu.Unlock()
		status := pending.status
		if status == "" {
			status = "pending"
		}
		if pending.token != "" {
			status = "approved"
		}
		response := map[string]any{"schema_version": 1, "pairing_id": pending.ID, "status": status, "person_id": pending.PersonID, "device_id": pending.DeviceID}
		if pending.token != "" {
			response["issuer"] = map[string]any{"key_id": pending.IssuerKeyID, "public_key": pending.IssuerPublicKey, "fingerprint": pending.IssuerFingerprint}
			response["issuer_fingerprint"] = pending.IssuerFingerprint
			response["client_id"], response["token"] = pending.ID, pending.token
			if producer, err := console.producerMetadata(); err == nil {
				response["producer"] = producer
			}
		}
		reply(writer, 200, response)
		return
	}
	console.mu.Unlock()
	failure(writer, 404, "not_found")
}

func (console *Console) manage(writer http.ResponseWriter, request *http.Request, current session) {
	if strings.HasPrefix(request.URL.Path, "/manage/api/authority/") {
		console.manageAuthority(writer, request)
		return
	}
	if request.URL.Path == "/manage/api/pair/approve" {
		if request.Method != http.MethodPost {
			failure(writer, http.StatusMethodNotAllowed, "method_not_allowed")
			return
		}
		console.managePairApprove(writer, request)
		return
	}
	if request.URL.Path == "/manage/api/pair/reject" {
		if request.Method != http.MethodPost {
			failure(writer, http.StatusMethodNotAllowed, "method_not_allowed")
			return
		}
		console.managePairReject(writer, request)
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
	console.mu.Lock()
	locked := true
	defer func() {
		if locked {
			console.mu.Unlock()
		}
	}()
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
	case "/manage/api/client/delete":
		removed, exists := next.Clients[input.ID]
		if !exists {
			failure(writer, 404, "client_not_found")
			return
		}
		transientAttempts := console.removeClientAttemptsLocked(&next, input.ID)
		delete(next.Clients, input.ID)
		for keyID, trusted := range next.TrustedIssuers {
			if trusted.ClientID == input.ID {
				delete(next.TrustedIssuers, keyID)
				next.RevokedIssuerKeys[keyID] = true
			}
		}
		transientAttempts = append(transientAttempts, console.removePersonConnectionsLocked(&next, removed.PersonID)...)
		if console.save(next) != nil {
			failure(writer, 500, "save_failed")
			return
		}
		console.state = next
		for _, attemptID := range transientAttempts {
			delete(console.connectorAttempts, attemptID)
		}
		postDeleteError := ""
		if err := console.rebuildConnectorRuntimes(); err != nil {
			postDeleteError = "invalid_connector_configuration"
		}
		if console.pair != nil && console.pair.ID == input.ID {
			console.pair = nil
		}
		if postDeleteError == "" {
			if err := console.retryPersonCleanupLocked(removed.PersonID); err != nil && !errors.Is(err, errConnectorLifecycleInProgress) {
				postDeleteError = "connection_cleanup_pending"
			}
		}
		if engine := console.authorityEngine(); engine != nil {
			principal := authorization.Principal{ClientID: input.ID, PersonID: removed.PersonID, DeviceID: removed.DeviceID, Authenticated: true}
			console.mu.Unlock()
			locked = false
			engine.ForgetPrincipal(principal)
		}
		if postDeleteError != "" {
			failure(writer, 500, postDeleteError)
			return
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
	clientScopes := make(map[string]any, len(state.Clients))
	for identifier := range state.Clients {
		clients = append(clients, identifier)
		client := state.Clients[identifier]
		clientScopes[identifier] = map[string]any{"person_id": client.PersonID, "device_id": client.DeviceID}
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
	reply(writer, 200, map[string]any{"csrf": current.csrf, "providers": providers, "clients": clients, "client_scopes": clientScopes, "pairing": pending, "address": "http://" + address, "traces": gateway.Traces(20)})
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
