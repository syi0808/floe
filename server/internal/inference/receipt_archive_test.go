package inference

import (
	"context"
	"testing"

	"floe/server/internal/operation"
	"floe/server/internal/trust"
)

func fullReceiptState() ConfigState {
	state := configurationTestState("target-a", "model-a")
	for i := 0; i < maxConfigurationReceipts; i++ {
		id := trust.NewID()
		state.Receipts = append(state.Receipts, ConfigurationReceipt{OperationID: id, Fingerprint: trust.Digest(id), Category: "ready", Code: "ok"})
	}
	return state
}

func TestArchiveFailurePrecedesCredentialMutation(t *testing.T) {
	for _, disposition := range []ConfigWriteDisposition{ConfigWriteRejected, ConfigWriteIndeterminate, ConfigWriteIntegrityFailure} {
		t.Run(string(rune('0'+disposition)), func(t *testing.T) {
			repo := &configurationTestRepository{state: fullReceiptState(), present: true, pageWriteDisposition: disposition}
			credentials := &configurationTestCredentialAccess{}
			owner, _ := openConfigurationTestOwner(t, repo, credentials)
			err := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, providerTargetUpdate(trust.NewID(), "model-b", "synthetic-new-key"))
			if !matchesOperationError(err, operation.Unavailable, "receipt_archive_unavailable") || credentials.writeCount() != 0 || repo.writes != 0 {
				t.Fatalf("archive failure allowed new credential intent: %v", err)
			}
			if len(configurationSnapshot(owner).Receipts) != maxConfigurationReceipts {
				t.Fatal("archive failure discarded receipts")
			}
		})
	}
}

func TestUnpublishedArchiveDoesNotManufactureOperationReceipt(t *testing.T) {
	repo := &configurationTestRepository{state: fullReceiptState(), present: true, nextWrite: &ConfigWriteOutcome{Disposition: ConfigWriteRejected}}
	owner, _ := openConfigurationTestOwner(t, repo, &configurationTestCredentialAccess{})
	input := RouteUpdate{OperationID: trust.NewID(), Purpose: string(QuickResponse), Target: "target-a", Enabled: false}
	if err := owner.UpdateRoute(context.Background(), trust.OperatorPrincipal{}, input); err == nil {
		t.Fatal("rejected config succeeded")
	}
	if configurationSnapshot(owner).ReceiptArchive.Pages != 0 || len(repo.pages) != 1 {
		t.Fatal("rejected config published archive head")
	}
	result, err := owner.RecoverOperation(context.Background(), trust.OperatorPrincipal{}, input.OperationID)
	if err != nil || result.Status != "no_record" {
		t.Fatal("orphan page manufactured new operation receipt")
	}
	if err := owner.UpdateRoute(context.Background(), trust.OperatorPrincipal{}, input); err != nil {
		t.Fatal(err)
	}
	if configurationSnapshot(owner).ReceiptArchive.Pages != 1 || len(repo.pages) != 1 {
		t.Fatal("retry did not reuse immutable archive")
	}
}

func TestReferencedArchiveFailureNeverBecomesOperationAbsence(t *testing.T) {
	for _, mode := range []string{"missing", "unavailable", "corrupt", "sequence"} {
		t.Run(mode, func(t *testing.T) {
			repo := &configurationTestRepository{state: fullReceiptState(), present: true}
			owner, engine := openConfigurationTestOwner(t, repo, &configurationTestCredentialAccess{})
			input := RouteUpdate{OperationID: trust.NewID(), Purpose: string(QuickResponse), Target: "target-a", Enabled: true}
			if err := owner.UpdateRoute(context.Background(), trust.OperatorPrincipal{}, input); err != nil {
				t.Fatal(err)
			}
			state := configurationSnapshot(owner)
			repo.mu.Lock()
			switch mode {
			case "missing":
				delete(repo.pages, state.ReceiptArchive.Head)
			case "unavailable":
				repo.pageReadDisposition = ConfigReadUnavailable
			case "corrupt":
				page := repo.pages[state.ReceiptArchive.Head]
				page.Receipts[0].Fingerprint = trust.Digest("changed")
				repo.pages[state.ReceiptArchive.Head] = page
			case "sequence":
				page := repo.pages[state.ReceiptArchive.Head]
				page.Sequence++
				repo.pages[state.ReceiptArchive.Head] = page
			}
			writes := repo.writes
			repo.mu.Unlock()
			input.OperationID = trust.NewID()
			if err := owner.UpdateRoute(context.Background(), trust.OperatorPrincipal{}, input); !matchesOperationError(err, operation.Unavailable, "") {
				t.Fatalf("archive failure admitted new operation: %v", err)
			}
			if repo.writes != writes {
				t.Fatal("archive failure changed config")
			}
			if _, _, _, err := engine.current(context.Background(), QuickResponse); err == nil {
				t.Fatal("integrity uncertainty left engine ready")
			}
			if result, err := owner.RecoverOperation(context.Background(), trust.OperatorPrincipal{}, input.OperationID); err == nil || result.Status == "no_record" {
				t.Fatal("archive loss reported command absence")
			}
		})
	}
}

func TestArchiveLookupCancellationDoesNotRetireLiveConfiguration(t *testing.T) {
	repo := &configurationTestRepository{state: fullReceiptState(), present: true}
	owner, engine := openConfigurationTestOwner(t, repo, &configurationTestCredentialAccess{})
	input := RouteUpdate{OperationID: trust.NewID(), Purpose: string(QuickResponse), Target: "target-a", Enabled: true}
	if err := owner.UpdateRoute(context.Background(), trust.OperatorPrincipal{}, input); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	input.OperationID = trust.NewID()
	if err := owner.UpdateRoute(ctx, trust.OperatorPrincipal{}, input); err == nil {
		t.Fatal("canceled lookup admitted mutation")
	}
	if _, _, _, err := engine.current(context.Background(), QuickResponse); err != nil {
		t.Fatalf("observer cancellation retired live configuration: %v", err)
	}
	if err := owner.UpdateRoute(context.Background(), trust.OperatorPrincipal{}, input); err != nil {
		t.Fatal(err)
	}
}

func TestArchiveHeadPublicationAckLossRecoversExactAuthority(t *testing.T) {
	for _, committed := range []bool{false, true} {
		t.Run(map[bool]string{false: "not_committed", true: "committed"}[committed], func(t *testing.T) {
			state := fullReceiptState()
			repo := &configurationTestRepository{state: state, present: true, nextWrite: &ConfigWriteOutcome{Disposition: ConfigWriteIndeterminate}, commitIndeterminate: committed}
			owner, _ := openConfigurationTestOwner(t, repo, &configurationTestCredentialAccess{})
			input := RouteUpdate{OperationID: trust.NewID(), Purpose: string(QuickResponse), Target: "target-a", Enabled: false}
			if err := owner.UpdateRoute(context.Background(), trust.OperatorPrincipal{}, input); err == nil {
				t.Fatal("lost acknowledgement reported success")
			}
			result, err := owner.RecoverOperation(context.Background(), trust.OperatorPrincipal{}, input.OperationID)
			if err != nil || (result.Status != "no_record") != committed {
				t.Fatalf("wrong authoritative result: %#v %v", result, err)
			}
			owner.mu.Lock()
			old, err := owner.findReceipt(context.Background(), state.Receipts[0].OperationID)
			owner.mu.Unlock()
			if err != nil || old == nil || *old != state.Receipts[0] {
				t.Fatal("snapshot publication uncertainty lost prior receipt")
			}
			if err := owner.UpdateRoute(context.Background(), trust.OperatorPrincipal{}, input); err != nil {
				t.Fatal(err)
			}
			if configurationSnapshot(owner).ReceiptArchive.Pages != 1 {
				t.Fatal("retry duplicated archive chain")
			}
		})
	}
}
