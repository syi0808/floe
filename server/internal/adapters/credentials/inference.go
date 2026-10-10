package credentials

import (
	"context"
	"strings"

	"floe/server/internal/inference"
)

// InferenceProviderAccess is limited to the generated FLOE_KEY_* references
// owned by Inference. Creation is immutable; deletion is limited to exact
// owner-issued references in durable cleanup state.
type InferenceProviderAccess struct{ store Store }

func NewInferenceProviderAccess(store Store) *InferenceProviderAccess {
	return &InferenceProviderAccess{store: store}
}

func (access *InferenceProviderAccess) ReadProviderCredential(ctx context.Context, reference string) (string, error) {
	if access == nil || access.store == nil || !validInferenceProviderReference(reference) {
		return "", ErrUnavailable
	}
	return access.store.Get(ctx, reference)
}

func (access *InferenceProviderAccess) CreateProviderCredential(ctx context.Context, reference, value string) error {
	if access == nil || access.store == nil || !validInferenceProviderReference(reference) || value == "" || len(value) > 8192 || strings.ContainsAny(value, "\r\n\x00") {
		return ErrUnavailable
	}
	creator, ok := access.store.(Creator)
	if !ok {
		return ErrUnavailable
	}
	return creator.Create(ctx, reference, value)
}

func (access *InferenceProviderAccess) DeleteProviderCredential(ctx context.Context, reference string) error {
	if access == nil || access.store == nil || !validInferenceProviderReference(reference) {
		return ErrUnavailable
	}
	return access.store.Delete(ctx, reference)
}

func validInferenceProviderReference(reference string) bool {
	if len(reference) < len("FLOE_KEY_")+1 || len(reference) > 256 || !strings.HasPrefix(reference, "FLOE_KEY_") {
		return false
	}
	for _, character := range reference {
		if !(character >= 'A' && character <= 'Z' || character >= '0' && character <= '9' || character == '_') {
			return false
		}
	}
	return true
}

var _ inference.ProviderCredentialAccess = (*InferenceProviderAccess)(nil)
