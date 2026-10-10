package pairing

import (
	"floe/server/internal/operation"
	"floe/server/internal/trust"
)

// ActionResult is the closed set of semantic results produced by pairing
// operations. HTTP decides how each variant is represented on the wire.
type ActionResult interface {
	pairingActionResult()
}

type Acknowledgement struct{}

func (Acknowledgement) pairingActionResult() {}

type IssuerResult struct {
	KeyID, PublicKey, Fingerprint string
}

type StartResult struct {
	SchemaVersion                             int
	PairingID, Code, Proof                    string
	ExpiresAtUnixMS                           int64
	PersonID, DeviceID                        string
	Producer                                  trust.ProducerMetadata
	Issuer                                    IssuerResult
	ChallengeID, Challenge, ProducerSignature string
}

func (StartResult) pairingActionResult() {}

type PairingCredentials struct {
	Issuer            IssuerResult
	IssuerFingerprint string
	ClientID          string
	Token             string
	Producer          trust.ProducerMetadata
}

type StatusResult struct {
	SchemaVersion int
	PairingID     string
	Status        string
	PersonID      string
	DeviceID      string
	Credentials   *PairingCredentials
}

func (StatusResult) pairingActionResult() {}

type StateResult struct {
	SchemaVersion      int
	PairingID          string
	Status             string
	CredentialDelivery *string
}

func (StateResult) pairingActionResult() {}

type CredentialDeliveryResult struct {
	SchemaVersion     int
	PairingID         string
	Status            string
	ClientID          string
	PersonID          string
	DeviceID          string
	Token             string
	Producer          trust.ProducerMetadata
	IssuerFingerprint string
	Issuer            IssuerResult
}

func (CredentialDeliveryResult) pairingActionResult() {}

func actionFailure(category operation.Category, code string) (ActionResult, error) {
	return nil, operation.Fail(category, code)
}

func actionError(err error) (ActionResult, error) {
	return nil, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
}

func stateFailure(category operation.Category, code string) (StateResult, error) {
	return StateResult{}, operation.Fail(category, code)
}

func stateError(err error) (StateResult, error) {
	return StateResult{}, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
}
