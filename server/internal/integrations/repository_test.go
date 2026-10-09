package integrations

import (
	"context"
	"errors"
	"reflect"
	"testing"
	"time"

	"floe/server/internal/trust"
)

type memoryRepository struct {
	state              StateSnapshot
	present            bool
	health             RepositoryHealth
	nextWrite          *WriteOutcome
	commitUnknownWrite bool
}

func newMemoryRepository() *memoryRepository {
	return &memoryRepository{state: initialState(), health: RepositoryReady}
}

func (repository *memoryRepository) LoadState() LoadOutcome {
	if !repository.present {
		return LoadOutcome{Disposition: LoadAbsent}
	}
	return LoadOutcome{Disposition: LoadPresent, Snapshot: clone(repository.state)}
}
func (repository *memoryRepository) Health() RepositoryHealth { return repository.health }
func (repository *memoryRepository) SaveState(state StateSnapshot) WriteOutcome {
	outcome := WriteOutcome{Disposition: WriteCommitted}
	if repository.nextWrite != nil {
		outcome = *repository.nextWrite
		repository.nextWrite = nil
	}
	if outcome.Disposition == WriteCommitted || repository.commitUnknownWrite && outcome.Disposition == WriteIndeterminate {
		repository.state = clone(state)
		repository.present = true
	}
	return outcome
}

type integrationTrustStub struct {
	pending []trust.CleanupTicket
	acks    []trust.CleanupReceipt
}

func (owner *integrationTrustStub) WithPairingOperation(_ trust.OperatorPrincipal, _, _, _ string, operation func(trust.PrincipalSnapshot) error) error {
	return operation(trust.PrincipalSnapshot{})
}
func (owner *integrationTrustStub) WithCurrentPrincipal(_ trust.Principal, operation func(trust.PrincipalSnapshot) error) error {
	return operation(trust.PrincipalSnapshot{})
}
func (owner *integrationTrustStub) PendingCleanup() ([]trust.CleanupTicket, error) {
	return append([]trust.CleanupTicket(nil), owner.pending...), nil
}
func (owner *integrationTrustStub) AcknowledgeCleanup(_ context.Context, _ trust.CleanupTicket, receipt trust.CleanupReceipt) error {
	owner.acks = append(owner.acks, receipt)
	return nil
}
func (*integrationTrustStub) ProducerMetadata() (trust.ProducerMetadata, error) {
	return trust.ProducerMetadata{}, nil
}
func (*integrationTrustStub) SignProducerChallenge([]byte) ([]byte, error) { return nil, nil }
func (*integrationTrustStub) RequiredSecurityError() error                 { return nil }

type integrationCredentialStub struct {
	storeErr  error
	deleteErr error
	stored    map[string]string
	deleted   []string
}

func (access *integrationCredentialStub) StoreConnectionCredential(_ context.Context, binding CredentialBinding, secret string) error {
	if access.storeErr != nil {
		return access.storeErr
	}
	if access.stored == nil {
		access.stored = map[string]string{}
	}
	access.stored[binding.Slot] = secret
	return nil
}
func (access *integrationCredentialStub) DeleteConnectionCredential(_ context.Context, binding CredentialBinding) error {
	access.deleted = append(access.deleted, binding.Slot)
	if access.deleteErr != nil {
		return access.deleteErr
	}
	delete(access.stored, binding.Slot)
	return nil
}

type setupStub struct{ disconnects int }

func (*setupStub) Begin(context.Context, CredentialBinding) (AuthorizationProgress, error) {
	return AuthorizationProgress{}, nil
}
func (*setupStub) Poll(context.Context, AttemptRef) (AuthorizationProgress, error) {
	return AuthorizationProgress{}, nil
}
func (setup *setupStub) Cancel(context.Context, AttemptRef) error { return nil }
func (setup *setupStub) Disconnect(context.Context, CredentialBinding) error {
	setup.disconnects++
	return nil
}
func (*setupStub) CachedStatus(CredentialBinding) CredentialStatus { return CredentialStatus{} }

type factoryStub struct {
	setup *setupStub
	opens int
}

func (factory *factoryStub) Open(context.Context, RuntimeConfig) (Runtime, error) {
	factory.opens++
	return Runtime{Setup: factory.setup}, nil
}

func validRecord(t *testing.T) Record {
	t.Helper()
	definition, ok := DefinitionFor("home_assistant.states")
	if !ok {
		t.Fatal("synthetic connector definition missing")
	}
	scope, err := ValidatedConnectorScope(definition, map[string]any{
		"base_url": "http://127.0.0.1:8123",
		"entities": []any{"sensor.synthetic"},
	})
	if err != nil {
		t.Fatal(err)
	}
	connectionID, personID, incarnation := trust.NewID(), trust.NewID(), trust.NewID()
	namespace := definition.CredentialName
	if namespace == "" {
		namespace = definition.OAuthCredential
	}
	slot, err := CredentialSlot(namespace, connectionID, personID)
	if err != nil {
		t.Fatal(err)
	}
	return Record{ConnectionID: connectionID, Revision: 1, ConnectorID: definition.ID, PersonID: personID, Scope: scope, Credential: slot, Incarnation: incarnation, Epoch: 1}
}

func TestRepositoryCommitOutcomesDoNotPartiallyAdopt(t *testing.T) {
	repository := newMemoryRepository()
	trustOwner := &integrationTrustStub{}
	credentials := &integrationCredentialStub{}
	service, err := New(context.Background(), repository, trustOwner, credentials, nil)
	if err != nil {
		t.Fatal(err)
	}
	before := clone(service.state)
	id := trust.NewID()
	change := DisconnectSnapshot{OperationID: id, ClientID: trust.NewID(), PersonID: trust.NewID(), DeviceID: "synthetic-device", ConnectorID: "home_assistant.states", ConnectionID: trust.NewID(), ConnectionRevision: 1, CleanupState: "pending"}
	next := clone(before)
	next.Disconnects[id] = change
	repository.nextWrite = &WriteOutcome{Disposition: WriteRejected, Cause: errors.New("synthetic precommit failure")}
	if err := service.persist(next); err == nil {
		t.Fatal("known precommit failure unexpectedly succeeded")
	}
	if service.state.Revision != before.Revision || service.state.Disconnects[id].OperationID != "" {
		t.Fatal("Integrations adopted state after a known precommit failure")
	}
	if repository.present && repository.state.Disconnects[id].OperationID != "" {
		t.Fatal("repository changed after a known precommit failure")
	}

	repository.commitUnknownWrite = true
	repository.nextWrite = &WriteOutcome{Disposition: WriteIndeterminate, Cause: errors.New("synthetic post-rename ambiguity")}
	if err := service.persist(next); err == nil {
		t.Fatal("ambiguous commit unexpectedly reported success")
	}
	if service.state.Revision != before.Revision || service.state.Disconnects[id].OperationID != "" {
		t.Fatal("Integrations adopted an indeterminate snapshot in the live process")
	}
	if service.storageReadyLocked() {
		t.Fatal("indeterminate persistence did not disable Integrations")
	}
	loaded := repository.LoadState()
	if loaded.Disposition != LoadPresent || loaded.Snapshot.Disconnects[id] != change {
		t.Fatal("synthetic post-rename commit was not retained by the repository")
	}
	reopened, err := New(context.Background(), repository, trustOwner, credentials, nil)
	if err != nil {
		t.Fatalf("reopen committed snapshot after lost acknowledgement: %v", err)
	}
	if reopened.state.Disconnects[id] != change {
		t.Fatal("reopen did not adopt the durable snapshot")
	}
}

func TestStartupFailsInterruptedAttemptBeforeCleanupAndRestartResumesEachPhase(t *testing.T) {
	record := validRecord(t)
	attemptID := trust.NewID()
	state := initialState()
	state.Attempts[attemptID] = AttemptSnapshot{
		ID: attemptID, ClientID: trust.NewID(), PersonID: record.PersonID, DeviceID: "synthetic-device", ConnectorID: record.ConnectorID,
		Record: record, Status: Pending, CreatedAt: time.Now().UnixMilli(), Revision: 1, Started: true, CatalogRevision: 1, RequestedScope: CloneConnectorScope(record.Scope),
	}
	repository := &memoryRepository{state: state, present: true, health: RepositoryReady}
	trustOwner := &integrationTrustStub{}
	credentials := &integrationCredentialStub{stored: map[string]string{record.Credential: "synthetic-secret"}, deleteErr: errors.New("synthetic credential store unavailable")}
	firstSetup := &setupStub{}
	firstFactory := &factoryStub{setup: firstSetup}
	factories := map[string]RuntimeFactory{record.ConnectorID: firstFactory}
	first, err := New(context.Background(), repository, trustOwner, credentials, factories)
	if err != nil {
		t.Fatal(err)
	}
	failed := first.state.Attempts[attemptID]
	if failed.Status != Failed || failed.ErrorCode != "authorization_interrupted" || !failed.Started {
		t.Fatalf("interrupted authorization was not durably failed: %#v", failed)
	}
	if len(first.state.Cleanup) != 1 {
		t.Fatalf("interrupted attempt did not retain a cleanup journal: %#v", first.state.Cleanup)
	}
	for _, cleanup := range first.state.Cleanup {
		if !cleanup.RuntimeDone[record.ConnectionID] || cleanup.VaultDone[record.ConnectionID] {
			t.Fatalf("startup did not retain the completed runtime and pending credential phases separately: %#v", cleanup)
		}
	}
	if firstSetup.disconnects != 1 || len(credentials.deleted) != 1 {
		t.Fatalf("first cleanup pass did not attempt both phases: runtime=%d credential=%d", firstSetup.disconnects, len(credentials.deleted))
	}
	first.Close()

	credentials.deleteErr = nil
	secondFactory := &factoryStub{setup: &setupStub{}}
	second, err := New(context.Background(), repository, trustOwner, credentials, map[string]RuntimeFactory{record.ConnectorID: secondFactory})
	if err != nil {
		t.Fatal(err)
	}
	if len(second.state.Cleanup) != 0 {
		t.Fatalf("cleanup did not settle after restart: %#v", second.state.Cleanup)
	}
	if secondFactory.opens != 0 {
		t.Fatalf("completed runtime cleanup was replayed after restart: opens=%d", secondFactory.opens)
	}
	if len(credentials.deleted) != 2 {
		t.Fatalf("credential cleanup was not retried exactly once: attempts=%d", len(credentials.deleted))
	}
	second.Close()
}

func TestImmutableCleanupReceiptReplayAcknowledgesWithoutRewriting(t *testing.T) {
	ticket := trust.CleanupTicket{ID: trust.NewID(), Revision: 1, PersonID: trust.NewID(), ClientID: trust.NewID(), Kind: trust.PersonAllSources, TrustGeneration: 2}
	receipt := trust.CleanupReceipt{TicketID: ticket.ID, TicketRevision: ticket.Revision, TrustGeneration: ticket.TrustGeneration, IntegrationRevision: 14}
	state := initialState()
	state.Revision = 12
	state.Receipts[ticket.ID] = receipt
	repository := &memoryRepository{state: state, present: true, health: RepositoryReady}
	trustOwner := &integrationTrustStub{pending: []trust.CleanupTicket{ticket}}
	service, err := New(context.Background(), repository, trustOwner, &integrationCredentialStub{}, nil)
	if err != nil {
		t.Fatal(err)
	}
	committedRevision := service.state.Revision
	if got := service.state.Receipts[ticket.ID]; got != receipt {
		t.Fatalf("startup changed the immutable cleanup receipt: got=%#v want=%#v", got, receipt)
	}
	if err := service.ApplyRevocation(context.Background(), ticket); err != nil {
		t.Fatal(err)
	}
	if service.state.Revision != committedRevision || !reflect.DeepEqual(service.state.Receipts[ticket.ID], receipt) {
		t.Fatal("receipt replay wrote or changed the committed receipt")
	}
	if len(trustOwner.acks) != 2 || trustOwner.acks[0] != receipt || trustOwner.acks[1] != receipt {
		t.Fatalf("receipt replay did not return the exact immutable receipt: %#v", trustOwner.acks)
	}
}
