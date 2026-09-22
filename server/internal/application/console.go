package application

import (
	"context"
	"errors"
	"net"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"

	"floe/server/internal/authorization"
	"floe/server/internal/connections"
	"floe/server/internal/inference"
	"floe/server/internal/pairing"
	httptransport "floe/server/internal/transport/http"
)

var personIDPattern = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$`)
var deviceIDPattern = regexp.MustCompile(`^[A-Za-z0-9._:-]{1,128}$`)
var connectorIDPattern = regexp.MustCompile(`^[A-Za-z0-9._:-]{1,128}$`)
var connectionIDPattern = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-4[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$`)

func validPersonID(value string) bool     { return personIDPattern.MatchString(value) }
func validDeviceID(value string) bool     { return deviceIDPattern.MatchString(value) }
func validConnectionID(value string) bool { return connectionIDPattern.MatchString(value) }

// OwnedConnection reports the connection this Person owns for a connector and
// the authority that executes it.
//
// Both are read under this Console's own lock so a snapshot is never stamped
// with an authority that no longer owns the connection.
func (console *Console) OwnedConnection(connectorID, personID string) (connections.Record, string, bool) {
	console.mu.Lock()
	defer console.mu.Unlock()
	record, exists := console.connectionForPerson(connectorID, personID)
	if !exists {
		return connections.Record{}, "", false
	}
	return record, console.state.ExecutionOwnerID, true
}

type AuthRuntime interface {
	Action(context.Context, string) (any, error)
	inference.CodexClient
}

// Console assembles the local server. It owns creation and lifetime; every
// business decision belongs to the owner it delegates to.
//
// Each owner keeps its own state and its own mutex; this struct never exposes a
// shared map or lock that two owners could both modify.
type Console struct {
	mu                    sync.Mutex
	directory, address    string
	vault                 Vault
	runtime               AuthRuntime
	gmail                 connections.ConnectorAuthRuntime
	microsoftAuth         connections.ConnectorOAuthRuntime
	microsoftMail         connections.CommunicationRuntime
	work                  map[string]connections.WorkContextRuntime
	logistics             map[string]connections.LogisticsRuntime
	driveAuth             connections.DriveAuthRuntime
	githubAuth            connections.DriveAuthRuntime
	slackAuth             connections.DriveAuthRuntime
	calendarAuth          connections.DriveAuthRuntime
	microsoftCalendarAuth connections.DriveAuthRuntime
	microsoftTeamsAuth    connections.DriveAuthRuntime
	calendars             map[string]connections.CalendarRuntime
	state                 diskState
	gateway               *inference.Gateway
	authorizationEngine   *authorization.Engine

	// Owner-scoped state. None of these are shared between owners.
	admissions    *authorization.Admissions
	connections   *connections.Registry
	pairing       *pairing.Operations
	handler       *httptransport.Handler
	internalToken string
	unavailable   map[string]bool

	testActive bool
}

func (console *Console) authorityEngine() *authorization.Engine {
	if console.admissions.TrustUnavailable() {
		return nil
	}
	return console.authorizationEngine
}

func (console *Console) RequiredSecurityError() error {
	if console.admissions.ProducerUnavailable() {
		return errors.New("producer identity unavailable")
	}
	if console.admissions.TrustUnavailable() {
		return errors.New("trust store unavailable")
	}
	return nil
}

func (console *Console) latchTrustUnavailable() {
	console.admissions.LatchTrustUnavailable()
}

func (console *Console) SetWorkContext(runtime connections.WorkContextRuntime) {
	console.mu.Lock()
	defer console.mu.Unlock()
	console.work = map[string]connections.WorkContextRuntime{}
	if connectorID, ok := contextRuntimeConnectorID(runtime); ok {
		if record, exists := console.connectionForConnector(connectorID); exists {
			console.work[record.ConnectionID] = runtime
		}
	}
}

func (console *Console) SetLogistics(runtime connections.LogisticsRuntime) {
	console.mu.Lock()
	defer console.mu.Unlock()
	console.logistics = map[string]connections.LogisticsRuntime{}
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
	return connections.SnapshotConnectorID(snapshot)
}

func (console *Console) SetDriveAuth(runtime connections.DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("google_drive.files", runtime); err != nil {
		return err
	}
	console.driveAuth = runtime
	console.retryPendingCleanupsLocked()
	return console.rebuildConnectorRuntimes()
}

func (console *Console) SetCalendarAuth(runtime connections.DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("calendar.google", runtime); err != nil {
		return err
	}
	console.calendarAuth = runtime
	console.retryPendingCleanupsLocked()
	return console.rebuildConnectorRuntimes()
}

func (console *Console) SetMicrosoftCalendarAuth(runtime connections.DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("calendar.microsoft", runtime); err != nil {
		return err
	}
	console.microsoftCalendarAuth = runtime
	console.retryPendingCleanupsLocked()
	return console.rebuildConnectorRuntimes()
}

func (console *Console) SetMicrosoftTeamsAuth(runtime connections.DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("microsoft.teams", runtime); err != nil {
		return err
	}
	console.microsoftTeamsAuth = runtime
	console.retryPendingCleanupsLocked()
	return console.rebuildConnectorRuntimes()
}

func (console *Console) SetGitHubAuth(runtime connections.DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("github.issues", runtime); err != nil {
		if !console.hasLegacyConnectorCredential("github.issues", connections.GithubTokenKey) {
			return err
		}
	}
	console.githubAuth = runtime
	console.retryPendingCleanupsLocked()
	return console.rebuildConnectorRuntimes()
}

func (console *Console) SetSlackAuth(runtime connections.DriveAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("slack.conversations", runtime); err != nil {
		if !console.hasLegacyConnectorCredential("slack.conversations", connections.SlackTokenKey) {
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

func (console *Console) SetGmailAuth(runtime connections.ConnectorAuthRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("gmail", runtime); err != nil {
		console.gmail = nil
		return err
	}
	console.gmail = runtime
	console.retryPendingCleanupsLocked()
	return nil
}

func (console *Console) SetMicrosoftMail(auth connections.ConnectorOAuthRuntime, runtime connections.CommunicationRuntime) error {
	console.mu.Lock()
	defer console.mu.Unlock()
	if err := console.bindConfiguredOAuthRuntime("microsoft.mail", auth); err != nil {
		console.microsoftAuth, console.microsoftMail = nil, nil
		return err
	}
	console.microsoftAuth = auth
	console.microsoftMail = runtime
	console.retryPendingCleanupsLocked()
	return nil
}

func (console *Console) bindConfiguredOAuthRuntime(connectorID string, runtime connections.ConnectorOAuthRuntime) error {
	_, exists := connections.DefinitionFor(connectorID)
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
	console := &Console{directory: directory, address: address, internalToken: randomToken(), vault: vault, runtime: runtime, state: state, admissions: authorization.NewAdmissions(), connections: connections.NewRegistry()}
	producer, producerError := loadProducerIdentity(filepath.Join(directory, "producer-identity.json"), !stateExists)
	if producerError != nil {
		console.admissions.LatchProducerUnavailable()
	} else {
		console.admissions.SetProducer(producer)
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
		console.admissions.LatchTrustUnavailable()
	}
	if state.TrustCorrupt {
		console.admissions.LatchTrustUnavailable()
	}
	console.authorizationEngine = engine
	console.rebuild()
	console.pairing = console.newPairing(false, nil)
	console.handler = console.newHandler(digest(admin))
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

var identifierPattern = regexp.MustCompile(`^[A-Za-z0-9_-]{1,64}$`)
