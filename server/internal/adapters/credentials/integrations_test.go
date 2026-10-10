package credentials_test

import (
	"context"
	"errors"
	"testing"

	credentialadapter "floe/server/internal/adapters/credentials"
	"floe/server/internal/integrations"
)

type memoryCredentialStore struct {
	values map[string]string
	getErr error
	putErr error
	delErr error
	reads  int
}

func (store *memoryCredentialStore) Get(_ context.Context, slot string) (string, error) {
	store.reads++
	if store.getErr != nil {
		return "", store.getErr
	}
	return store.values[slot], nil
}
func (store *memoryCredentialStore) Put(_ context.Context, slot, value string) error {
	if store.putErr != nil {
		return store.putErr
	}
	store.values[slot] = value
	return nil
}
func (store *memoryCredentialStore) Delete(_ context.Context, slot string) error {
	if store.delErr != nil {
		return store.delErr
	}
	delete(store.values, slot)
	return nil
}

func TestIntegrationCredentialCapabilityReadsBackOnlyItsConnectionAndPropagatesUnavailable(t *testing.T) {
	base := &memoryCredentialStore{values: map[string]string{}}
	binding := integrations.CredentialBinding{Slot: "FLOE_SYNTHETIC:opaque", ConnectionID: "connection", PersonID: "person", Incarnation: "incarnation", Generation: 1}
	access := credentialadapter.NewIntegrationAccess(base)
	if err := access.StoreConnectionCredential(context.Background(), binding, "synthetic-secret"); err != nil {
		t.Fatal(err)
	}
	scoped := credentialadapter.NewScopedStore(base, binding)
	got, err := scoped.Get(context.Background(), binding.Slot)
	if err != nil || got != "synthetic-secret" {
		t.Fatalf("credential readback failed: value=%q err=%v", got, err)
	}
	reads := base.reads
	if _, err := scoped.Get(context.Background(), "another-connection-slot"); !errors.Is(err, credentialadapter.ErrUnavailable) {
		t.Fatalf("runtime read escaped its immutable binding: %v", err)
	}
	if base.reads != reads {
		t.Fatal("mismatched slot reached the backing credential store")
	}

	base.getErr = credentialadapter.ErrLocked
	if _, err := scoped.Get(context.Background(), binding.Slot); !errors.Is(err, credentialadapter.ErrLocked) {
		t.Fatalf("credential unavailability was hidden: %v", err)
	}
	base.putErr = credentialadapter.ErrBusy
	if err := access.StoreConnectionCredential(context.Background(), binding, "replacement-secret"); !errors.Is(err, credentialadapter.ErrBusy) {
		t.Fatalf("credential write unavailability was hidden: %v", err)
	}
	base.delErr = credentialadapter.ErrLocked
	if err := access.DeleteConnectionCredential(context.Background(), binding); !errors.Is(err, credentialadapter.ErrLocked) {
		t.Fatalf("credential delete unavailability was hidden: %v", err)
	}
}
