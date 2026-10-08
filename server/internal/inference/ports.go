package inference

import (
	"context"
	"floe/server/internal/trust"
	"strings"
)

type Trust interface {
	WithCurrentPrincipal(trust.Principal, func(trust.PrincipalSnapshot) error) error
	WithCurrentOperator(trust.OperatorPrincipal, func() error) error
	ActiveIssuer(trust.Principal) (trust.IssuerSnapshot, error)
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
	Capabilities() []string
	ModelIdentity() ModelIdentity
	BudgetOverride() *ModelBudgetOverride
}

// ModelIdentity is safe catalog correlation data. It does not select a model
// or authorize a provider capability.
type ModelIdentity struct {
	ProviderID string
	ModelID    string
}

func validModelIdentity(identity ModelIdentity) bool {
	return ValidAlias(identity.ProviderID) && identity.ModelID != "" && len(identity.ModelID) <= 128 &&
		identity.ModelID == strings.TrimSpace(identity.ModelID) && !strings.ContainsAny(identity.ModelID, "\r\n\x00")
}

// ResolvedModelTarget is a private selection value. It never contains provider credentials.
type ResolvedModelTarget struct {
	targetID, effort, accountIdentity string
	generation                        uint64
	capabilities                      []string
	modelIdentity                     ModelIdentity
	budgetProfile                     ModelBudgetProfile
}

func (t ResolvedModelTarget) TargetID() string        { return t.targetID }
func (t ResolvedModelTarget) ReasoningEffort() string { return t.effort }
func (t ResolvedModelTarget) AccountIdentity() string { return t.accountIdentity }
func (t ResolvedModelTarget) SelectedOutputReservationTokens() (uint32, bool) {
	if t.budgetProfile.SelectedOutputReservation.Status != LimitKnown || t.budgetProfile.SelectedOutputReservation.Tokens == nil {
		return 0, false
	}
	return *t.budgetProfile.SelectedOutputReservation.Tokens, true
}
