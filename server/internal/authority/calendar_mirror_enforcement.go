package authority

import (
	"context"
	"errors"
	"time"

	sourcecontract "floe/server/internal/contracts/source"
	"floe/server/internal/trust"
	viewcontracts "floe/server/internal/views/contracts"
)

// CalendarMirrorEnforcer owns the product Calendar permit policy. It has no
// source resolver or Reader and exposes no Engine request or policy value.
type CalendarMirrorEnforcer struct {
	engine *Engine
	trust  ProducerTrust
	fence  SourceFence
}

func NewCalendarMirrorEnforcer(engine *Engine, producer ProducerTrust, fence SourceFence) (*CalendarMirrorEnforcer, error) {
	if engine == nil || producer == nil || fence == nil {
		return nil, ErrUnavailable
	}
	return &CalendarMirrorEnforcer{engine: engine, trust: producer, fence: fence}, nil
}

func (enforcer *CalendarMirrorEnforcer) IssueCalendarMirrorAdmission(
	principal trust.Principal,
	snapshot sourcecontract.Snapshot,
	claims viewcontracts.ProductCalendarClaims,
	expires time.Time,
) (string, []byte, time.Time, error) {
	now := enforcer.engine.clock.Now()
	if !principal.Valid() || claims.PersonID != principal.PersonID() || claims.ClientID != principal.ClientID() || claims.DeviceID != principal.DeviceID() || !expires.After(now) || expires.After(now.Add(time.Minute)) {
		return "", nil, time.Time{}, ErrInvalid
	}
	metadata, err := enforcer.trust.ProducerMetadata()
	if err != nil {
		return "", nil, time.Time{}, ErrUnavailable
	}
	issuer, err := enforcer.engine.trust.ActiveIssuer(principal)
	if err != nil || claims.EnrollmentID != issuer.EnrollmentID || claims.Audience != metadata.Audience || claims.ProducerInstance != metadata.InstanceID || claims.ProducerKeyFingerprint != metadata.Fingerprint {
		return "", nil, time.Time{}, ErrDenied
	}
	policy := productCalendarPolicy{claims: viewcontracts.CloneProductCalendarClaims(claims), expires: expires}
	request := Request{Source: sourcecontract.Clone(snapshot), policy: policy}
	if validateRequest(request) != nil {
		return "", nil, time.Time{}, ErrInvalid
	}
	challenge, err := enforcer.engine.IssueAdmission(principal, request, enforcer.fence)
	if err != nil {
		if errors.Is(err, ErrCapacity) {
			return "", nil, time.Time{}, admissionCapacityError{}
		}
		return "", nil, time.Time{}, err
	}
	return challenge.ID, append([]byte(nil), challenge.Bytes...), challenge.ExpiresAt, nil
}

func (enforcer *CalendarMirrorEnforcer) CancelCalendarMirrorAdmission(id string) {
	enforcer.engine.CancelAdmission(id)
}

func (enforcer *CalendarMirrorEnforcer) ClaimCalendarMirrorAdmission(principal trust.Principal, proof trust.Proof) (string, sourcecontract.Snapshot, viewcontracts.ProductCalendarClaims, time.Time, error) {
	id, request, err := enforcer.engine.ClaimAdmission(principal, viewcontracts.CalendarMirror, proof, enforcer.fence)
	if err != nil {
		return "", sourcecontract.Snapshot{}, viewcontracts.ProductCalendarClaims{}, time.Time{}, err
	}
	policy, ok := request.policy.(productCalendarPolicy)
	if !ok {
		enforcer.engine.CancelAdmission(id)
		return "", sourcecontract.Snapshot{}, viewcontracts.ProductCalendarClaims{}, time.Time{}, ErrDenied
	}
	return id, sourcecontract.Clone(request.Source), viewcontracts.CloneProductCalendarClaims(policy.claims), policy.expires, nil
}

func (enforcer *CalendarMirrorEnforcer) StageCalendarMirrorResult(id string, principal trust.Principal, result []byte, count uint32) (string, []byte, time.Time, error) {
	release, err := enforcer.engine.StageResult(id, principal, result, count, enforcer.fence)
	if err != nil {
		return "", nil, time.Time{}, err
	}
	return release.ID, append([]byte(nil), release.Bytes...), release.ExpiresAt, nil
}

func (enforcer *CalendarMirrorEnforcer) CancelCalendarMirrorRelease(id string) {
	enforcer.engine.CancelRelease(id)
}

func (enforcer *CalendarMirrorEnforcer) ClaimCalendarMirrorRelease(ctx context.Context, principal trust.Principal, proof trust.Proof) ([]byte, error) {
	return enforcer.engine.ClaimRelease(ctx, principal, viewcontracts.CalendarMirror, proof, enforcer.fence)
}
