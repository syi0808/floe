package credentials

import (
	"context"

	"floe/server/internal/pairing"
	"floe/server/internal/trust"
)

// PairingAccess maps Pairing's record operations onto the process credential
// store without exposing arbitrary credential slots to the Pairing owner.
type PairingAccess struct{ store Store }

func NewPairingAccess(store Store) *PairingAccess {
	return &PairingAccess{store: store}
}

func (access *PairingAccess) ReadPairingToken(ctx context.Context, id pairing.PairingID) (string, error) {
	if !access.validPairingID(id) {
		return "", ErrUnavailable
	}
	return access.store.Get(ctx, "FLOE_PAIRING_"+string(id))
}

func (access *PairingAccess) StorePairingToken(ctx context.Context, id pairing.PairingID, value string) error {
	if !access.validPairingID(id) {
		return ErrUnavailable
	}
	return access.store.Put(ctx, "FLOE_PAIRING_"+string(id), value)
}

func (access *PairingAccess) DeletePairingToken(ctx context.Context, id pairing.PairingID) error {
	if !access.validPairingID(id) {
		return ErrUnavailable
	}
	return access.store.Delete(ctx, "FLOE_PAIRING_"+string(id))
}

func (access *PairingAccess) ReadReceiptIndex(ctx context.Context) (string, error) {
	if access == nil || access.store == nil {
		return "", ErrUnavailable
	}
	return access.store.Get(ctx, "FLOE_PAIRING_INDEX")
}

func (access *PairingAccess) StoreReceiptIndex(ctx context.Context, value string) error {
	if access == nil || access.store == nil {
		return ErrUnavailable
	}
	return access.store.Put(ctx, "FLOE_PAIRING_INDEX", value)
}

func (access *PairingAccess) ReadPairingAttempt(ctx context.Context, id pairing.OperationID) (string, error) {
	if !access.validOperationID(id) {
		return "", ErrUnavailable
	}
	return access.store.Get(ctx, "FLOE_PAIRING_ATTEMPT_"+string(id))
}

func (access *PairingAccess) StorePairingAttempt(ctx context.Context, id pairing.OperationID, value string) error {
	if !access.validOperationID(id) {
		return ErrUnavailable
	}
	return access.store.Put(ctx, "FLOE_PAIRING_ATTEMPT_"+string(id), value)
}

func (access *PairingAccess) validPairingID(id pairing.PairingID) bool {
	return access != nil && access.store != nil && trust.ValidID(string(id))
}

func (access *PairingAccess) validOperationID(id pairing.OperationID) bool {
	return access != nil && access.store != nil && trust.ValidID(string(id))
}

var _ pairing.CredentialAccess = (*PairingAccess)(nil)
