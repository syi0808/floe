package storage_test

import (
	"encoding/json"
	"reflect"
	"testing"

	ownerstorage "floe/server/internal/adapters/storage"
	privatefiles "floe/server/internal/adapters/storage/privatefiles"
	"floe/server/internal/integrations"
	"floe/server/internal/trust"
)

func testFiles(t *testing.T, scope string) *privatefiles.Files {
	t.Helper()
	root, err := privatefiles.NewFiles(t.TempDir(), "p5-synthetic-"+scope, make([]byte, 32), true)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(root.Close)
	files, err := root.Scope(scope)
	if err != nil {
		t.Fatal(err)
	}
	return files
}

func TestTrustRepositoryKeepsFilesAndCredentialsAcrossReopen(t *testing.T) {
	files := testFiles(t, "trust")
	repository := ownerstorage.NewTrustRepository(files)
	initial := repository.Load()
	if initial.State.Disposition != trust.ReadAbsent || initial.Producer.Disposition != trust.ReadAbsent || initial.AdministratorCredential.Disposition != trust.ReadAbsent {
		t.Fatalf("fresh synthetic store did not report absent bootstrap values: %#v", initial)
	}
	first, err := trust.Open(repository, true)
	if err != nil {
		t.Fatal(err)
	}
	metadata, err := first.ProducerMetadata()
	if err != nil {
		t.Fatal(err)
	}
	token, err := repository.ReadAdministratorToken()
	if err != nil || len(token) < 32 {
		t.Fatalf("administrator credential readback failed: length=%d err=%v", len(token), err)
	}

	second, err := trust.Open(repository, false)
	if err != nil {
		t.Fatalf("Trust did not reopen: %v", err)
	}
	metadataAfterReopen, err := second.ProducerMetadata()
	if err != nil || metadataAfterReopen != metadata {
		t.Fatalf("producer identity changed across reopen: got=%#v err=%v", metadataAfterReopen, err)
	}
	if _, login := second.LoginOperator(token); login.Code != "" {
		t.Fatalf("administrator credential did not authenticate after reopen: %s", login.Code)
	}

	identityBytes, err := files.Read("producer-identity.json", 4096)
	if err != nil {
		t.Fatal(err)
	}
	var identityShape map[string]json.RawMessage
	if err := json.Unmarshal(identityBytes, &identityShape); err != nil {
		t.Fatal(err)
	}
	for _, key := range []string{"schema_version", "key_id", "private_key", "public_key"} {
		if _, ok := identityShape[key]; !ok {
			t.Fatalf("producer identity file lost the %q field", key)
		}
	}
	stateBytes, err := files.Read("trust.json", 1<<20)
	if err != nil {
		t.Fatal(err)
	}
	var stateShape map[string]json.RawMessage
	if err := json.Unmarshal(stateBytes, &stateShape); err != nil {
		t.Fatal(err)
	}
	for _, key := range []string{"schema_version", "revision", "instance_id", "execution_owner", "clients", "issuers", "revoked_issuers", "cleanup"} {
		if _, ok := stateShape[key]; !ok {
			t.Fatalf("Trust state file lost the %q field", key)
		}
	}
}

func TestTrustRepositoryLeavesPartialBootstrapUntouched(t *testing.T) {
	files := testFiles(t, "trust-partial")
	repository := ownerstorage.NewTrustRepository(files)
	identity, err := trust.GenerateProducerIdentity(trust.NewID())
	if err != nil {
		t.Fatal(err)
	}
	encoded, err := json.Marshal(identity)
	if err != nil {
		t.Fatal(err)
	}
	if err := files.Write("producer-identity.json", encoded); err != nil {
		t.Fatal(err)
	}
	before, err := files.Read("producer-identity.json", 4096)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := trust.Open(repository, true); err == nil {
		t.Fatal("partial prior initialization was accepted")
	}
	after, err := files.Read("producer-identity.json", 4096)
	if err != nil || !reflect.DeepEqual(after, before) {
		t.Fatalf("partial producer identity was replaced: err=%v", err)
	}
	if _, err := files.Read("trust.json", 1<<20); err == nil {
		t.Fatal("Trust regenerated state for a partial prior initialization")
	}
	if _, err := repository.ReadAdministratorToken(); err == nil {
		t.Fatal("Trust generated an administrator token for a partial prior initialization")
	}
}

func TestIntegrationsRepositoryRoundTripsWholeSnapshotAndRejectsDuplicateJSON(t *testing.T) {
	files := testFiles(t, "integrations")
	repository := ownerstorage.NewIntegrationsRepository(files)
	if loaded := repository.LoadState(); loaded.Disposition != integrations.LoadAbsent {
		t.Fatalf("fresh integration store was not absent: %#v", loaded)
	}
	state := integrations.StateSnapshot{
		SchemaVersion: 1,
		Revision:      8,
		Connections:   map[string]integrations.Record{},
		Attempts:      map[string]integrations.AttemptSnapshot{},
		Cleanup:       map[string]integrations.CleanupSnapshot{},
		Receipts:      map[string]trust.CleanupReceipt{},
		Disconnects:   map[string]integrations.DisconnectSnapshot{},
	}
	if outcome := repository.SaveState(state); outcome.Disposition != integrations.WriteCommitted {
		t.Fatalf("snapshot commit failed: %#v", outcome)
	}
	loaded := repository.LoadState()
	if loaded.Disposition != integrations.LoadPresent || !reflect.DeepEqual(loaded.Snapshot, state) {
		t.Fatalf("whole state snapshot failed to read back: disposition=%v state=%#v", loaded.Disposition, loaded.Snapshot)
	}
	encoded, err := json.Marshal(state)
	if err != nil {
		t.Fatal(err)
	}
	if err := files.Write("integrations.json", []byte(`{"schema_version":1,"schema_version":1}`)); err != nil {
		t.Fatal(err)
	}
	if loaded = repository.LoadState(); loaded.Disposition != integrations.LoadInvalid {
		t.Fatalf("strict repository read accepted duplicate JSON keys: %#v", loaded)
	}
	if err := files.Write("integrations.json", encoded); err != nil {
		t.Fatal(err)
	}
	if loaded = repository.LoadState(); loaded.Disposition != integrations.LoadPresent {
		t.Fatalf("repository did not recover after the valid fixture was restored: %#v", loaded)
	}
}
