// Package credentials adapts the process credential store to the narrow
// Integrations connection capability and to one-runtime scoped connector IO.
package credentials

import (
	"context"

	"floe/server/internal/integrations"
)

type IntegrationAccess struct{ store Store }

func NewIntegrationAccess(store Store) *IntegrationAccess {
	return &IntegrationAccess{store: store}
}

func (access *IntegrationAccess) StoreConnectionCredential(ctx context.Context, binding integrations.CredentialBinding, secret string) error {
	if access == nil || access.store == nil || !validBinding(binding) {
		return ErrUnavailable
	}
	return access.store.Put(ctx, binding.Slot, secret)
}

func (access *IntegrationAccess) DeleteConnectionCredential(ctx context.Context, binding integrations.CredentialBinding) error {
	if access == nil || access.store == nil || !validBinding(binding) {
		return ErrUnavailable
	}
	return access.store.Delete(ctx, binding.Slot)
}

// ScopedStore permits a connector runtime to read and rotate only the exact
// credential slot pinned by its immutable connection binding.
type ScopedStore struct {
	store   Store
	binding integrations.CredentialBinding
}

func NewScopedStore(store Store, binding integrations.CredentialBinding) *ScopedStore {
	return &ScopedStore{store: store, binding: binding}
}

func (store *ScopedStore) Get(ctx context.Context, slot string) (string, error) {
	if !store.allowed(slot) {
		return "", ErrUnavailable
	}
	return store.store.Get(ctx, slot)
}

func (store *ScopedStore) Put(ctx context.Context, slot, value string) error {
	if !store.allowed(slot) {
		return ErrUnavailable
	}
	return store.store.Put(ctx, slot, value)
}

func (store *ScopedStore) Delete(ctx context.Context, slot string) error {
	if !store.allowed(slot) {
		return ErrUnavailable
	}
	return store.store.Delete(ctx, slot)
}

func (store *ScopedStore) allowed(slot string) bool {
	return store != nil && store.store != nil && validBinding(store.binding) && slot == store.binding.Slot
}

func validBinding(binding integrations.CredentialBinding) bool {
	return binding.Slot != "" && binding.ConnectionID != "" && binding.PersonID != "" && binding.Incarnation != "" && binding.Generation != 0
}

var _ integrations.CredentialAccess = (*IntegrationAccess)(nil)
var _ Store = (*ScopedStore)(nil)
