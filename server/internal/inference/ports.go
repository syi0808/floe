package inference

import (
	"context"
	"floe/server/internal/trust"
)

type Trust interface {
	WithCurrentPrincipal(trust.Principal, func(trust.PrincipalSnapshot) error) error
	WithCurrentOperator(trust.OperatorPrincipal, func() error) error
	ActiveIssuer(trust.Principal) (trust.IssuerSnapshot, error)
}

// ProviderCredentialAccess exposes exact-slot operations for Inference-owned
// provider credentials. Creation is immutable and deletion is used only by
// durable owner cleanup state.
type ProviderCredentialAccess interface {
	ReadProviderCredential(context.Context, string) (string, error)
	CreateProviderCredential(context.Context, string, string) error
	DeleteProviderCredential(context.Context, string) error
}

type PurposeCatalog interface {
	Snapshot(context.Context) (PurposeInventory, error)
}
type ModelExecutor interface {
	InvokeAgent(context.Context, ResolvedModelTarget, AgentInvocation) (AgentResult, error)
	InvokeStructured(context.Context, ResolvedModelTarget, StructuredInvocation) (StructuredResult, error)
}
type ModelAccount interface {
	Ready(context.Context) error
	ReplayIdentity() string
	ProtocolCapabilities() []string
	ModelIdentity() ModelIdentity
	BudgetOverride() *ModelBudgetOverride
}

// ModelIdentity is safe catalog correlation data. It does not select a model
// or authorize a provider capability.
type ModelIdentity struct {
	ProviderID string
	ModelID    string
	Endpoint   string
}

func validModelIdentity(identity ModelIdentity) bool {
	endpoint, ok := CanonicalModelEndpoint(identity.Endpoint)
	return validModelIdentityBase(identity) && ok && endpoint == identity.Endpoint
}

// ResolvedModelTarget is a private selection value. It never contains provider credentials.
type ResolvedModelTarget struct {
	targetID, effort, accountIdentity string
	generation                        uint64
	capabilities                      []string
	capabilityStates                  CapabilityStates
	modelIdentity                     ModelIdentity
	budgetProfile                     ModelBudgetProfile
}

func (t ResolvedModelTarget) TargetID() string        { return t.targetID }
func (t ResolvedModelTarget) ReasoningEffort() string { return t.effort }
func (t ResolvedModelTarget) AccountIdentity() string { return t.accountIdentity }
func (t ResolvedModelTarget) CapabilityStates() CapabilityStates {
	return cloneCapabilityStates(t.capabilityStates)
}
func (t ResolvedModelTarget) SupportsAgent(in AgentInvocation) bool {
	return supportsAgent(t.capabilityStates, in)
}
func (t ResolvedModelTarget) SupportsStructuredOutput() bool {
	return supportsStructured(t.capabilityStates)
}
func (t ResolvedModelTarget) SelectedOutputReservationTokens() (uint32, bool) {
	if t.budgetProfile.SelectedOutputReservation.Status != LimitKnown || t.budgetProfile.SelectedOutputReservation.Tokens == nil {
		return 0, false
	}
	return *t.budgetProfile.SelectedOutputReservation.Tokens, true
}
