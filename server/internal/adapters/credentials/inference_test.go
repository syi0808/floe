package credentials

import (
	"context"
	"errors"
	"testing"

	"floe/server/internal/credentials"
)

type inferenceCredentialStore struct {
	values map[string]string
	getErr error
	putErr error
}

func (store *inferenceCredentialStore) Get(ctx context.Context, name string) (string, error) {
	if err := ctx.Err(); err != nil {
		return "", err
	}
	if store.getErr != nil {
		return "", store.getErr
	}
	return store.values[name], nil
}

func (store *inferenceCredentialStore) Put(ctx context.Context, name, value string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if store.putErr != nil {
		return store.putErr
	}
	store.values[name] = value
	return nil
}

func (store *inferenceCredentialStore) Create(ctx context.Context, name, value string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if store.putErr != nil {
		return store.putErr
	}
	if _, exists := store.values[name]; exists {
		return credentials.ErrUnavailable
	}
	store.values[name] = value
	return nil
}

func (*inferenceCredentialStore) Delete(context.Context, string) error { return nil }

func TestInferenceProviderAccessScopesProviderCredentialIO(t *testing.T) {
	store := &inferenceCredentialStore{values: map[string]string{}}
	access := NewInferenceProviderAccess(store)
	ctx := context.Background()
	const reference = "FLOE_KEY_SYNTHETIC_123"
	if err := access.CreateProviderCredential(ctx, reference, "synthetic-key"); err != nil {
		t.Fatal(err)
	}
	if err := access.CreateProviderCredential(ctx, reference, "replacement-key"); !errors.Is(err, credentials.ErrUnavailable) {
		t.Fatalf("create-only adapter replaced an existing slot: %v", err)
	}
	value, err := access.ReadProviderCredential(ctx, reference)
	if err != nil || value != "synthetic-key" {
		t.Fatalf("provider credential round trip failed: value=%q err=%v", value, err)
	}
	if _, err = access.ReadProviderCredential(ctx, "FLOE_GMAIL_OAUTH"); !errors.Is(err, credentials.ErrUnavailable) {
		t.Fatalf("adapter accepted a non-Inference credential reference: %v", err)
	}
	if err = access.CreateProviderCredential(ctx, reference, "bad\nkey"); !errors.Is(err, credentials.ErrUnavailable) {
		t.Fatalf("adapter accepted an invalid provider credential value: %v", err)
	}
	store.getErr = credentials.ErrUnavailable
	if _, err = access.ReadProviderCredential(ctx, reference); !errors.Is(err, credentials.ErrUnavailable) {
		t.Fatalf("credential-store unavailability was lost: %v", err)
	}
}
