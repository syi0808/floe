package credentials

import (
	"context"
	"reflect"
	"testing"

	"floe/server/internal/pairing"
)

type pairingStoreCall struct {
	operation string
	slot      string
	value     string
}

type pairingStoreSpy struct{ calls []pairingStoreCall }

func (store *pairingStoreSpy) Get(_ context.Context, slot string) (string, error) {
	store.calls = append(store.calls, pairingStoreCall{operation: "get", slot: slot})
	return "stored", nil
}

func (store *pairingStoreSpy) Put(_ context.Context, slot, value string) error {
	store.calls = append(store.calls, pairingStoreCall{operation: "put", slot: slot, value: value})
	return nil
}

func (store *pairingStoreSpy) Delete(_ context.Context, slot string) error {
	store.calls = append(store.calls, pairingStoreCall{operation: "delete", slot: slot})
	return nil
}

func TestPairingAccessMapsOnlyPairingRecords(t *testing.T) {
	backing := &pairingStoreSpy{}
	access := NewPairingAccess(backing)
	ctx := context.Background()
	pairingID := pairing.PairingID("11111111-2222-4333-8444-555555555555")
	operationID := pairing.OperationID("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee")

	if _, err := access.ReadPairingToken(ctx, pairingID); err != nil {
		t.Fatal(err)
	}
	if err := access.StorePairingToken(ctx, pairingID, "token"); err != nil {
		t.Fatal(err)
	}
	if err := access.DeletePairingToken(ctx, pairingID); err != nil {
		t.Fatal(err)
	}
	if _, err := access.ReadReceiptIndex(ctx); err != nil {
		t.Fatal(err)
	}
	if err := access.StoreReceiptIndex(ctx, "index"); err != nil {
		t.Fatal(err)
	}
	if _, err := access.ReadPairingAttempt(ctx, operationID); err != nil {
		t.Fatal(err)
	}
	if err := access.StorePairingAttempt(ctx, operationID, "attempt"); err != nil {
		t.Fatal(err)
	}

	want := []pairingStoreCall{
		{operation: "get", slot: "FLOE_PAIRING_" + string(pairingID)},
		{operation: "put", slot: "FLOE_PAIRING_" + string(pairingID), value: "token"},
		{operation: "delete", slot: "FLOE_PAIRING_" + string(pairingID)},
		{operation: "get", slot: "FLOE_PAIRING_INDEX"},
		{operation: "put", slot: "FLOE_PAIRING_INDEX", value: "index"},
		{operation: "get", slot: "FLOE_PAIRING_ATTEMPT_" + string(operationID)},
		{operation: "put", slot: "FLOE_PAIRING_ATTEMPT_" + string(operationID), value: "attempt"},
	}
	if !reflect.DeepEqual(backing.calls, want) {
		t.Fatalf("Pairing record mapping changed:\n got %#v\nwant %#v", backing.calls, want)
	}
}

func TestPairingClearClientDeletesOnlyItsToken(t *testing.T) {
	backing := &pairingStoreSpy{}
	operations := pairing.NewOperations(nil, NewPairingAccess(backing), nil)
	id := pairing.PairingID("11111111-2222-4333-8444-555555555555")

	if err := operations.ClearClient(context.Background(), string(id)); err != nil {
		t.Fatal(err)
	}
	want := []pairingStoreCall{{operation: "delete", slot: "FLOE_PAIRING_" + string(id)}}
	if !reflect.DeepEqual(backing.calls, want) {
		t.Fatalf("Pairing cleanup mapping changed:\n got %#v\nwant %#v", backing.calls, want)
	}
}

func TestPairingAccessRejectsNonPairingReferencesBeforeStoreIO(t *testing.T) {
	backing := &pairingStoreSpy{}
	access := NewPairingAccess(backing)
	ctx := context.Background()

	for _, reference := range []pairing.PairingID{
		pairing.PairingID("FLOE_PAIRING_INDEX"),
		pairing.PairingID("FLOE_PAIRING_ATTEMPT_11111111-2222-4333-8444-555555555555"),
		pairing.PairingID("FLOE_GITHUB_TOKEN"),
		pairing.PairingID("FLOE_OPENAI_API_KEY"),
	} {
		if _, err := access.ReadPairingToken(ctx, reference); err != ErrUnavailable {
			t.Fatalf("ReadPairingToken(%q) err = %v, want ErrUnavailable", reference, err)
		}
		if err := access.StorePairingToken(ctx, reference, "token"); err != ErrUnavailable {
			t.Fatalf("StorePairingToken(%q) err = %v, want ErrUnavailable", reference, err)
		}
		if err := access.DeletePairingToken(ctx, reference); err != ErrUnavailable {
			t.Fatalf("DeletePairingToken(%q) err = %v, want ErrUnavailable", reference, err)
		}
	}
	for _, reference := range []pairing.OperationID{
		pairing.OperationID("FLOE_PAIRING_INDEX"),
		pairing.OperationID("FLOE_PAIRING_11111111-2222-4333-8444-555555555555"),
		pairing.OperationID("FLOE_GOOGLE_11111111-2222-4333-8444-555555555555"),
		pairing.OperationID("not-an-operation-id"),
	} {
		if _, err := access.ReadPairingAttempt(ctx, reference); err != ErrUnavailable {
			t.Fatalf("ReadPairingAttempt(%q) err = %v, want ErrUnavailable", reference, err)
		}
		if err := access.StorePairingAttempt(ctx, reference, "attempt"); err != ErrUnavailable {
			t.Fatalf("StorePairingAttempt(%q) err = %v, want ErrUnavailable", reference, err)
		}
	}
	if len(backing.calls) != 0 {
		t.Fatalf("invalid references reached backing store: %#v", backing.calls)
	}
}
