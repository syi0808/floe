package authority

import (
	"context"
	"errors"
	"time"

	sourcecontract "floe/server/internal/contracts/source"
	"floe/server/internal/trust"
)

// ViewEnforcer is Authority's implementation of the inward Views enforcement
// port. It issues and consumes signed state; it never resolves or reads a View.
type ViewEnforcer struct {
	engine *Engine
	trust  ProducerTrust
	fence  SourceFence
}

type admissionCapacityError struct{}

func (admissionCapacityError) Error() string    { return "admission capacity reached" }
func (admissionCapacityError) Category() string { return "limited" }
func (admissionCapacityError) Code() string     { return "admission_capacity" }

func NewViewEnforcer(engine *Engine, producer ProducerTrust, fence SourceFence) (*ViewEnforcer, error) {
	if engine == nil || producer == nil || fence == nil {
		return nil, ErrUnavailable
	}
	return &ViewEnforcer{engine: engine, trust: producer, fence: fence}, nil
}

func (enforcer *ViewEnforcer) IssueViewAdmission(
	principal trust.Principal,
	snapshot sourcecontract.Snapshot,
	purpose, consumer, grantID, grantIncarnation string,
	grantEpoch uint64,
	resources []string,
	queryDigest [32]byte,
	bounds sourcecontract.Bounds,
) (string, []byte, time.Time, error) {
	metadata, err := enforcer.trust.ProducerMetadata()
	if err != nil {
		return "", nil, time.Time{}, ErrUnavailable
	}
	request := Request{Source: sourcecontract.Clone(snapshot), policy: assistantPolicy{
		Audience: metadata.Audience, Purpose: purpose, Consumer: consumer,
		Grant:     GrantReference{ID: grantID, Incarnation: grantIncarnation, Epoch: grantEpoch},
		Resources: append([]string(nil), resources...), QueryDigest: queryDigest,
		MaxItems: bounds.MaxItems, MaxBytes: bounds.MaxBytes,
	}}
	challenge, err := enforcer.engine.IssueAdmission(principal, request, enforcer.fence)
	if err != nil {
		if errors.Is(err, ErrCapacity) {
			return "", nil, time.Time{}, admissionCapacityError{}
		}
		return "", nil, time.Time{}, err
	}
	return challenge.ID, append([]byte(nil), challenge.Bytes...), challenge.ExpiresAt, nil
}

func (enforcer *ViewEnforcer) CancelViewAdmission(id string) {
	enforcer.engine.CancelAdmission(id)
}

func (enforcer *ViewEnforcer) ClaimViewAdmission(principal trust.Principal, viewID sourcecontract.ID, proof trust.Proof) (string, sourcecontract.Snapshot, [32]byte, sourcecontract.Bounds, error) {
	id, request, err := enforcer.engine.ClaimAdmission(principal, viewID, proof, enforcer.fence)
	if err != nil {
		return "", sourcecontract.Snapshot{}, [32]byte{}, sourcecontract.Bounds{}, err
	}
	policy, ok := request.policy.(assistantPolicy)
	if !ok {
		enforcer.engine.CancelAdmission(id)
		return "", sourcecontract.Snapshot{}, [32]byte{}, sourcecontract.Bounds{}, ErrDenied
	}
	return id, sourcecontract.Clone(request.Source), policy.QueryDigest, request.bounds(), nil
}

func (enforcer *ViewEnforcer) StageViewResult(id string, principal trust.Principal, result []byte, count uint32) (string, []byte, time.Time, error) {
	release, err := enforcer.engine.StageResult(id, principal, result, count, enforcer.fence)
	if err != nil {
		return "", nil, time.Time{}, err
	}
	return release.ID, append([]byte(nil), release.Bytes...), release.ExpiresAt, nil
}

func (enforcer *ViewEnforcer) CancelViewRelease(id string) {
	enforcer.engine.CancelRelease(id)
}

func (enforcer *ViewEnforcer) ClaimViewRelease(ctx context.Context, principal trust.Principal, viewID sourcecontract.ID, proof trust.Proof) ([]byte, error) {
	return enforcer.engine.ClaimRelease(ctx, principal, viewID, proof, enforcer.fence)
}
