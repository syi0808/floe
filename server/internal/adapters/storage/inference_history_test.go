package storage

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"floe/server/internal/adapters/models/providers"
	"floe/server/internal/inference"
	"floe/server/internal/trust"
)

type historyTrust struct{}

func (historyTrust) WithCurrentPrincipal(_ trust.Principal, f func(trust.PrincipalSnapshot) error) error {
	return f(trust.PrincipalSnapshot{})
}
func (historyTrust) WithCurrentOperator(_ trust.OperatorPrincipal, f func() error) error { return f() }
func (historyTrust) ActiveIssuer(trust.Principal) (trust.IssuerSnapshot, error) {
	return trust.IssuerSnapshot{}, nil
}

type historyMetadata struct{}

func (historyMetadata) Snapshot(context.Context) (inference.CapabilityEvidenceSnapshot, error) {
	return inference.CapabilityEvidenceSnapshot{ContractVersion: inference.CapabilityEvidenceContractVersion, Entries: []inference.CapabilityEvidenceEntry{}}, nil
}

type noHistoryCredentials struct{}

func (noHistoryCredentials) ReadProviderCredential(context.Context, string) (string, error) {
	panic("history-only test must not read credentials")
}
func (noHistoryCredentials) CreateProviderCredential(context.Context, string, string) error {
	panic("history-only test must not create credentials")
}
func (noHistoryCredentials) DeleteProviderCredential(context.Context, string) error {
	panic("history-only test must not delete credentials")
}

func TestEncryptedConfigurationHistoryExceedsLifetimeThresholdAndReopens(t *testing.T) {
	files, root := inferenceTestFiles(t)
	factory := providers.NewFactory(func(context.Context, string) (string, error) { panic("history test must not read credentials") }, nil)
	open := func() *inference.Configuration {
		engine, err := inference.NewService(historyTrust{}, historyMetadata{})
		if err != nil {
			t.Fatal(err)
		}
		owner, err := inference.OpenConfiguration(context.Background(), NewInferenceConfigRepository(files), engine, historyTrust{}, noHistoryCredentials{}, factory)
		if err != nil {
			t.Fatal(err)
		}
		t.Cleanup(owner.Close)
		return owner
	}
	owner := open()
	first := inference.TargetUpdate{OperationID: trust.NewID(), ID: "history", Provider: "openai_compatible", BaseURL: "https://example.invalid", Model: "synthetic-model"}
	if err := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, first); err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 129; i++ {
		input := inference.RouteUpdate{OperationID: trust.NewID(), Purpose: string(inference.QuickResponse), Target: "history", Enabled: i%2 == 0}
		if err := owner.UpdateRoute(context.Background(), trust.OperatorPrincipal{}, input); err != nil {
			t.Fatalf("change %d: %v", i+2, err)
		}
	}
	repository := NewInferenceConfigRepository(files)
	state := repository.LoadConfig().State
	if state.ReceiptArchive.Pages != 2 || len(state.Receipts) != 2 {
		t.Fatal("live state retained lifetime history")
	}
	encoded, err := json.Marshal(state)
	if err != nil || len(encoded) > 4096 {
		t.Fatal("live configuration grew with history")
	}
	owner.Close()
	reopened := open()
	if err := reopened.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, first); err != nil {
		t.Fatal(err)
	}
	if repository.LoadConfig().State.Revision != state.Revision {
		t.Fatal("reopen replay repeated mutation")
	}
	result, err := reopened.RecoverOperation(context.Background(), trust.OperatorPrincipal{}, first.OperationID)
	if err != nil || result.Status == "no_record" {
		t.Fatalf("archived operation lost: %#v %v", result, err)
	}
	if err := reopened.DeleteTarget(context.Background(), trust.OperatorPrincipal{}, "history", trust.NewID()); err != nil {
		t.Fatal(err)
	}
	if err := reopened.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, first); err != nil {
		t.Fatal(err)
	}
	if _, exists := repository.LoadConfig().State.Targets["history"]; exists {
		t.Fatal("old exact replay resurrected deleted target")
	}
	// The on-disk archive is encrypted, not a plaintext side log.
	name, err := receiptPageName(state.ReceiptArchive.Head)
	if err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(filepath.Join(root, "inference", name))
	if err != nil {
		t.Fatal(err)
	}
	if json.Valid(raw) {
		t.Fatal("receipt archive was stored as plaintext")
	}
}

func TestEncryptedReceiptPagesAreBoundedAndHashVerified(t *testing.T) {
	files, _ := inferenceTestFiles(t)
	repository := NewInferenceConfigRepository(files)
	page := inference.ConfigReceiptPage{Sequence: 1, Receipts: []inference.ConfigurationReceipt{}}
	for i := 0; i < 64; i++ {
		id := trust.NewID()
		page.Receipts = append(page.Receipts, inference.ConfigurationReceipt{OperationID: id, Fingerprint: trust.Digest(id), Category: "ready", Code: "ok"})
	}
	digest, err := inference.ConfigReceiptPageDigest(page)
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 2; i++ {
		if repository.SaveReceiptPage(digest, page).Disposition != inference.ConfigWriteCommitted {
			t.Fatal("exact immutable page did not commit/replay")
		}
	}
	if repository.LoadReceiptPage(digest).Disposition != inference.ConfigReadPresent {
		t.Fatal("page did not round trip")
	}
	page.Receipts[0].Fingerprint = trust.Digest("changed")
	if repository.SaveReceiptPage(digest, page).Disposition != inference.ConfigWriteRejected {
		t.Fatal("old page identity accepted changed contents")
	}
	name, _ := receiptPageName(digest)
	encoded, _ := json.Marshal(page)
	if err := files.Write(name, encoded); err != nil {
		t.Fatal(err)
	}
	if repository.LoadReceiptPage(digest).Disposition != inference.ConfigReadInvalid {
		t.Fatal("hash mismatch accepted")
	}
	if repository.LoadReceiptPage("../inference").Disposition != inference.ConfigReadInvalid {
		t.Fatal("untrusted archive path accepted")
	}
	if err := files.Write(name, make([]byte, inference.MaxConfigReceiptPageBytes+1)); err != nil {
		t.Fatal(err)
	}
	if repository.LoadReceiptPage(digest).Disposition == inference.ConfigReadPresent {
		t.Fatal("oversized page accepted")
	}
}

func TestObsoleteConfigurationWithoutArchiveIdentityRemainsUntouched(t *testing.T) {
	files, _ := inferenceTestFiles(t)
	old := []byte(`{"schema_version":3,"revision":0,"targets":{},"routes":{},"providers":{},"owned_slots":[],"cleanup":[],"receipts":[]}`)
	if err := files.Write(inferenceConfigurationFile, old); err != nil {
		t.Fatal(err)
	}
	if NewInferenceConfigRepository(files).LoadConfig().Disposition != inference.ConfigReadInvalid {
		t.Fatal("obsolete config silently admitted")
	}
	retained, err := files.Read(inferenceConfigurationFile, inference.MaxConfigSnapshotBytes)
	if err != nil || string(retained) != string(old) {
		t.Fatal("obsolete config was reset or migrated")
	}
}
