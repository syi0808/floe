package application

import (
	"context"
	"errors"
	"testing"

	"floe/server/internal/authorization"
	"floe/server/internal/connections"
	"floe/server/internal/credentials"
)

type providerIdentityRuntime struct {
	identity string
	err      error
}

func (*providerIdentityRuntime) BindCredential(string) error { return nil }
func (*providerIdentityRuntime) Ready() bool                 { return true }
func (*providerIdentityRuntime) Token(context.Context) (string, error) {
	return "provider-token", nil
}
func (*providerIdentityRuntime) Action(context.Context, string) (any, error) {
	return map[string]any{"status": "connected"}, nil
}
func (runtime *providerIdentityRuntime) ProviderIdentity(context.Context) (string, error) {
	return runtime.identity, runtime.err
}
func (runtime *providerIdentityRuntime) ProviderIdentityStatus() (string, bool) {
	return runtime.identity, runtime.err == nil
}
func (runtime *providerIdentityRuntime) WithVerifiedProviderIdentity(expectedCredential, expected string, consume func() error) error {
	if runtime.err != nil || runtime.identity != expected {
		return errors.New("identity unavailable")
	}
	return consume()
}

func TestOAuthCompletionPersistsVerifiedProviderIdentityAcrossReopen(t *testing.T) {
	fixture := setup(t)
	connectionID := "00000000-0000-4000-8000-000000000099"
	attemptID := "provider-identity-attempt"
	runtime := &providerIdentityRuntime{identity: "google:subject-a"}
	credential, _ := credentials.ConnectionName("FLOE_GOOGLE_CALENDAR_OAUTH", connectionID, fixturePersonID)
	fixture.vault.values[credential] = "fixture-token"
	fixture.console.mu.Lock()
	fixture.console.calendarAuth = runtime
	fixture.console.state.Clients["fixture-client"] = pairedClient{ClientID: "fixture-client", TokenHash: digest("fixture-token"), PersonID: fixturePersonID, DeviceID: fixtureDeviceID}
	scope := map[string]any{"calendar_id": "primary"}
	fixture.console.connections.PutAttempt(attemptID, &connections.Attempt{ID: attemptID, ConnectorID: "calendar.google", ConnectionID: connectionID, PersonID: fixturePersonID, Scope: scope, Credential: credential, Status: "connected"})
	fixture.console.state.Attempts[attemptID] = connectionAttemptRecord{AttemptID: attemptID, ConnectorID: "calendar.google", ConnectionID: connectionID, PersonID: fixturePersonID, Incarnation: "00000000-0000-4000-8000-000000000099", Epoch: 1, Scope: scope, Credential: credential}
	fixture.console.mu.Unlock()
	if !fixture.console.finishClientOAuthAttempt(attemptID) {
		t.Fatal("OAuth completion rejected verified identity")
	}
	fixture.console.mu.Lock()
	record := fixture.console.state.Connections[connectionID]
	fixture.console.mu.Unlock()
	if record.ProviderIdentity != "google:subject-a" || record.IdentityUnverified {
		t.Fatalf("identity was not persisted: %+v", record)
	}
	reopened, err := New(fixture.console.directory, "127.0.0.1:8431", fixture.vault, nil)
	if err != nil {
		t.Fatal(err)
	}
	reopened.mu.Lock()
	reopenedRecord := reopened.state.Connections[connectionID]
	reopened.mu.Unlock()
	if reopenedRecord.ProviderIdentity != "google:subject-a" || reopenedRecord.IdentityUnverified {
		t.Fatalf("reopened identity changed: %+v", reopenedRecord)
	}
	if reopenedRecord.Epoch != 1 {
		t.Fatalf("reopen changed source epoch: %d", reopenedRecord.Epoch)
	}
	reopened.calendarAuth = runtime
	principal := authorization.Principal{ClientID: "fixture-client", PersonID: fixturePersonID, DeviceID: fixtureDeviceID, Authenticated: true}
	reference := authorization.SourceReference{ConnectorID: "calendar.google", ConnectionID: connectionID, ExecutionOwner: reopened.state.ExecutionOwnerID, Incarnation: record.Incarnation, Epoch: record.Epoch}
	if err := reopened.WithCurrentSource(principal, reference, func(authorization.SourceSnapshot) error { return nil }); err != nil {
		t.Fatalf("verified source was denied: %v", err)
	}
	runtime.identity = "google:subject-b"
	if err := reopened.WithCurrentSource(principal, reference, func(authorization.SourceSnapshot) error { return nil }); err == nil {
		t.Fatal("changed runtime identity retained old source authorization")
	}
	if reopenedRecord.Epoch != 1 {
		t.Fatalf("runtime identity change implicitly altered epoch: %d", reopenedRecord.Epoch)
	}
}

func TestOAuthCompletionMissingProviderIdentityPersistsProtectedDeny(t *testing.T) {
	fixture := setup(t)
	connectionID := "00000000-0000-4000-8000-000000000098"
	attemptID := "provider-identity-missing"
	runtime := &providerIdentityRuntime{err: errors.New("identity unavailable")}
	credential, _ := credentials.ConnectionName("FLOE_GOOGLE_CALENDAR_OAUTH", connectionID, fixturePersonID)
	fixture.vault.values[credential] = "fixture-token"
	fixture.console.mu.Lock()
	fixture.console.calendarAuth = runtime
	fixture.console.state.Clients["fixture-client"] = pairedClient{ClientID: "fixture-client", TokenHash: digest("fixture-token"), PersonID: fixturePersonID, DeviceID: fixtureDeviceID}
	scope := map[string]any{"calendar_id": "primary"}
	fixture.console.connections.PutAttempt(attemptID, &connections.Attempt{ID: attemptID, ConnectorID: "calendar.google", ConnectionID: connectionID, PersonID: fixturePersonID, Scope: scope, Credential: credential, Status: "connected"})
	fixture.console.state.Attempts[attemptID] = connectionAttemptRecord{AttemptID: attemptID, ConnectorID: "calendar.google", ConnectionID: connectionID, PersonID: fixturePersonID, Incarnation: "00000000-0000-4000-8000-000000000098", Epoch: 1, Scope: scope, Credential: credential}
	fixture.console.mu.Unlock()
	if !fixture.console.finishClientOAuthAttempt(attemptID) {
		t.Fatal("OAuth completion should retain an unverified connection")
	}
	fixture.console.mu.Lock()
	record := fixture.console.state.Connections[connectionID]
	fixture.console.mu.Unlock()
	if !record.IdentityUnverified || record.ProviderIdentity != "" {
		t.Fatalf("missing identity was published as trusted: %+v", record)
	}
	principal := authorization.Principal{ClientID: "missing", PersonID: fixturePersonID, DeviceID: fixtureDeviceID, Authenticated: true}
	if err := fixture.console.WithCurrentSource(principal, authorization.SourceReference{ConnectorID: "calendar.google", ConnectionID: connectionID, ExecutionOwner: fixture.console.state.ExecutionOwnerID, Incarnation: record.Incarnation, Epoch: record.Epoch}, func(authorization.SourceSnapshot) error { return nil }); err == nil {
		t.Fatal("unverified provider identity authorized source")
	}
}
