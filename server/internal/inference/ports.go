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
}

// ResolvedModelTarget is a private selection value. It never contains provider credentials.
type ResolvedModelTarget struct {
	targetID, effort, accountIdentity string
	generation                        uint64
	capabilities                      []string
}

func (t ResolvedModelTarget) TargetID() string        { return t.targetID }
func (t ResolvedModelTarget) ReasoningEffort() string { return t.effort }
func (t ResolvedModelTarget) AccountIdentity() string { return t.accountIdentity }
