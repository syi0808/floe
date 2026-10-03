// Package authority owns signed source admission, bounded staging and one-use release.
package authority

import (
	"bytes"
	"context"
	cryptorand "crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"floe/server/internal/trust"
	"floe/server/internal/views"
	"fmt"
	"reflect"
	"strings"
	"sync"
	"time"
)

const (
	SchemaVersion = 1

	ChallengeTTL           = 30 * time.Second
	MaxPendingPerClient    = 128
	MaxStageBytesPerResult = 1 << 20
	MaxStageBytesGlobal    = 16 << 20
	MaxProofBytes          = 4 << 10
	MaxChallengeBytes      = 64 << 10
	MaxJSONDepth           = 16
	MaxResources           = 64
	MaxResourceBytes       = 256
	MaxQueryDigestBytes    = 32
	MaxPurposeBytes        = 128
	MaxConsumerBytes       = 256
	MaxAudienceBytes       = 256
)

var (
	ErrInvalid     = errors.New("invalid authorization message")
	ErrDenied      = errors.New("authorization denied")
	ErrExpired     = errors.New("authorization challenge expired")
	ErrReplay      = errors.New("authorization challenge already used")
	ErrUnavailable = errors.New("authorization persistence unavailable")
	ErrConflict    = errors.New("authorization state conflict")
)

type Operation string

const (
	OperationEnrollment Operation = "enrollment"
	OperationAdmission  Operation = "admission"
	OperationRelease    Operation = "release"
)

type Clock interface {
	Now() time.Time
	Monotonic() time.Duration
}

type systemClock struct{ started time.Time }

func (c systemClock) Now() time.Time           { return time.Now() }
func (c systemClock) Monotonic() time.Duration { return time.Since(c.started) }

type Trust interface {
	ActiveIssuer(trust.Principal) (trust.IssuerSnapshot, error)
	WithActiveIssuer(trust.Principal, string, func(trust.IssuerSnapshot) error) error
	WithCurrentPrincipal(trust.Principal, func(trust.PrincipalSnapshot) error) error
}
type Options struct {
	Clock  Clock
	Random func([]byte) error
	Trust  Trust
}
type GrantReference struct {
	ID          string
	Incarnation string
	Epoch       uint64
}

// Request retains exactly one closed authority policy and a full source fence.
type Request struct {
	Source views.SourceSnapshot
	policy requestPolicy
}
type requestPolicy interface {
	requestBounds() views.Bounds
	clonePolicy() requestPolicy
}
type assistantPolicy struct {
	Audience, Purpose, Consumer string
	Grant                       GrantReference
	Resources                   []string
	QueryDigest                 [32]byte
	MaxItems, MaxBytes          uint32
}

func (p assistantPolicy) requestBounds() views.Bounds {
	return views.Bounds{MaxItems: p.MaxItems, MaxBytes: p.MaxBytes}
}
func (p assistantPolicy) clonePolicy() requestPolicy {
	p.Resources = append([]string(nil), p.Resources...)
	return p
}
func (r Request) bounds() views.Bounds {
	if r.policy == nil {
		return views.Bounds{}
	}
	return r.policy.requestBounds()
}

type Challenge struct {
	ID        string
	Operation Operation
	Bytes     []byte
	BytesB64  string
	ExpiresAt time.Time
}

type Release struct {
	ID        string
	Bytes     []byte
	BytesB64  string
	ExpiresAt time.Time
}

type Engine struct {
	mu          sync.Mutex
	clock       Clock
	random      func([]byte) error
	trust       Trust
	pending     map[string]*pendingChallenge
	admissions  map[string]*admission
	stages      map[string]*stage
	stagedBytes int
}
type pendingChallenge struct {
	wire         []byte
	operation    Operation
	principal    trust.Principal
	request      Request
	keyID        string
	deadline     time.Duration
	deadlineWall time.Time
	state        challengeState
}
type challengeState uint8

const (
	challengePending challengeState = iota
	challengeChecking
	challengeClaimed
)

type stage struct {
	id, admissionID string
	principal       trust.Principal
	request         Request
	result          []byte
	resultDigest    string
	deadline        time.Duration
	deadlineWall    time.Time
	challenge       []byte
	keyID           string
	state           challengeState
}
type admission struct {
	principal trust.Principal
	request   Request
	keyID     string
	deadline  time.Duration
}

func New(opts Options) (*Engine, error) {
	if opts.Trust == nil {
		return nil, ErrUnavailable
	}
	clock := opts.Clock
	if clock == nil {
		clock = systemClock{started: time.Now()}
	}
	random := opts.Random
	if random == nil {
		random = func(b []byte) error { _, err := cryptorand.Read(b); return err }
	}
	return &Engine{clock: clock, random: random, trust: opts.Trust, pending: map[string]*pendingChallenge{}, admissions: map[string]*admission{}, stages: map[string]*stage{}}, nil
}
func (e *Engine) clientCountLocked(client string) int {
	n := 0
	for _, v := range e.pending {
		if v.principal.ClientID() == client {
			n++
		}
	}
	for _, v := range e.admissions {
		if v.principal.ClientID() == client {
			n++
		}
	}
	for _, v := range e.stages {
		if v.principal.ClientID() == client {
			n++
		}
	}
	return n
}
func (e *Engine) IssueAdmission(p trust.Principal, r Request, source SourceFence) (Challenge, error) {
	if validatePrincipal(p) != nil || validateRequest(r) != nil || source == nil {
		return Challenge{}, ErrInvalid
	}
	issuer, err := e.trust.ActiveIssuer(p)
	if err != nil {
		return Challenge{}, ErrDenied
	}
	r = cloneRequest(r)
	challenge, wire, err := e.makeChallenge(OperationAdmission, p, issuer.KeyID, r, "", "")
	if err != nil {
		return Challenge{}, err
	}
	consumed := false
	err = source.WithCurrentSource(p, r.Source, func(s views.SourceSnapshot) error {
		consumed = true
		if !sourceMatches(p, r.Source, s) {
			return ErrDenied
		}
		e.mu.Lock()
		defer e.mu.Unlock()
		e.sweepExpiredLocked()
		if e.clientCountLocked(p.ClientID()) >= MaxPendingPerClient {
			return ErrDenied
		}
		e.pending[challenge.ID] = &pendingChallenge{wire: append([]byte(nil), wire...), operation: OperationAdmission, principal: p, request: r, keyID: issuer.KeyID, deadline: e.clock.Monotonic() + challenge.ExpiresAt.Sub(e.clock.Now()), deadlineWall: challenge.ExpiresAt}
		return nil
	})
	if err != nil || !consumed {
		return Challenge{}, ErrDenied
	}
	return challenge, nil
}
func (e *Engine) ClaimAdmission(p trust.Principal, viewID views.ID, proof trust.Proof, source SourceFence) (string, Request, error) {
	if source == nil || !p.Valid() {
		return "", Request{}, ErrDenied
	}
	e.mu.Lock()
	e.sweepExpiredLocked()
	c, ok := e.pending[proof.ChallengeID]
	if !ok || c.state != challengePending || !p.Same(c.principal) || c.request.Source.Descriptor.ID != string(viewID) {
		e.mu.Unlock()
		return "", Request{}, ErrReplay
	}
	c.state = challengeChecking
	copy := *c
	copy.wire = append([]byte(nil), c.wire...)
	e.mu.Unlock()
	verified := false
	err := e.trust.WithActiveIssuer(p, copy.keyID, func(issuer trust.IssuerSnapshot) error {
		verified = true
		return trust.VerifyProof(proof, proof.ChallengeID, copy.keyID, copy.wire, issuer.PublicKey)
	})
	if err != nil || !verified {
		e.CancelAdmission(proof.ChallengeID)
		return "", Request{}, ErrDenied
	}
	consumed := false
	err = source.WithCurrentSource(p, copy.request.Source, func(s views.SourceSnapshot) error {
		consumed = true
		e.mu.Lock()
		defer e.mu.Unlock()
		live, ok := e.pending[proof.ChallengeID]
		if !ok || live != c || live.state != challengeChecking {
			return ErrReplay
		}
		delete(e.pending, proof.ChallengeID)
		if e.expired(live.deadline) || !sourceMatches(p, copy.request.Source, s) {
			return ErrDenied
		}
		e.admissions[proof.ChallengeID] = &admission{p, cloneRequest(copy.request), copy.keyID, copy.deadline}
		return nil
	})
	if err != nil || !consumed {
		e.CancelAdmission(proof.ChallengeID)
		if err == nil {
			err = ErrDenied
		}
		return "", Request{}, err
	}
	return proof.ChallengeID, cloneRequest(copy.request), nil
}
func (e *Engine) StageResult(id string, p trust.Principal, r Request, result []byte, count uint32, source SourceFence) (Release, error) {
	if source == nil || !p.Valid() || validateRequest(r) != nil || len(result) > MaxStageBytesPerResult || len(result) > int(r.bounds().MaxBytes) || count > r.bounds().MaxItems {
		return Release{}, ErrDenied
	}
	issuer, err := e.trust.ActiveIssuer(p)
	if err != nil {
		return Release{}, ErrDenied
	}
	hash := sha256.Sum256(result)
	resultDigest := hex.EncodeToString(hash[:])
	challenge, encoded, err := e.makeChallenge(OperationRelease, p, issuer.KeyID, r, id, resultDigest)
	if err != nil {
		return Release{}, err
	}
	var out Release
	err = source.WithCurrentSource(p, r.Source, func(current views.SourceSnapshot) error {
		if !sourceMatches(p, r.Source, current) {
			return ErrDenied
		}
		e.mu.Lock()
		defer e.mu.Unlock()
		e.sweepExpiredLocked()
		a, ok := e.admissions[id]
		if !ok || !p.Same(a.principal) || a.keyID != issuer.KeyID || !requestsEqual(a.request, r) {
			return ErrDenied
		}
		if e.stagedBytes+len(result) > MaxStageBytesGlobal {
			return ErrDenied
		}
		e.stages[challenge.ID] = &stage{id: challenge.ID, admissionID: id, principal: p, request: cloneRequest(r), result: append([]byte(nil), result...), resultDigest: resultDigest, deadline: e.clock.Monotonic() + challenge.ExpiresAt.Sub(e.clock.Now()), deadlineWall: challenge.ExpiresAt, challenge: append([]byte(nil), encoded...), keyID: issuer.KeyID}
		e.stagedBytes += len(result)
		delete(e.admissions, id)
		out = Release{challenge.ID, append([]byte(nil), encoded...), encodeB64(encoded), challenge.ExpiresAt}
		return nil
	})
	return out, err
}
func (e *Engine) ClaimRelease(ctx context.Context, p trust.Principal, viewID views.ID, proof trust.Proof, resolver SourceResolver, source SourceFence) ([]byte, error) {
	if resolver == nil || source == nil || !p.Valid() {
		return nil, ErrDenied
	}
	e.mu.Lock()
	e.sweepExpiredLocked()
	s, ok := e.stages[proof.ChallengeID]
	if !ok || s.state != challengePending || !p.Same(s.principal) {
		e.mu.Unlock()
		return nil, ErrReplay
	}
	s.state = challengeChecking
	copy := *s
	copy.challenge = append([]byte(nil), s.challenge...)
	e.mu.Unlock()
	verified := false
	err := e.trust.WithActiveIssuer(p, copy.keyID, func(issuer trust.IssuerSnapshot) error {
		verified = true
		return trust.VerifyProof(proof, proof.ChallengeID, copy.keyID, copy.challenge, issuer.PublicKey)
	})
	if err != nil || !verified {
		e.CancelRelease(proof.ChallengeID)
		return nil, ErrDenied
	}
	if copy.request.Source.Descriptor.ID != string(viewID) || resolver.PreflightSource(ctx, p, copy.request.Source) != nil {
		e.CancelRelease(proof.ChallengeID)
		return nil, ErrDenied
	}
	var out []byte
	consumed := false
	err = source.WithCurrentSource(p, copy.request.Source, func(current views.SourceSnapshot) error {
		consumed = true
		e.mu.Lock()
		defer e.mu.Unlock()
		live, ok := e.stages[proof.ChallengeID]
		if !ok || live != s || live.state != challengeChecking {
			return ErrReplay
		}
		defer e.dropStageLocked(live)
		if e.expired(live.deadline) || !sourceMatches(p, live.request.Source, current) {
			return ErrDenied
		}
		out = append([]byte(nil), live.result...)
		return nil
	})
	if err != nil || !consumed {
		e.CancelRelease(proof.ChallengeID)
		if err == nil {
			err = ErrDenied
		}
		return nil, err
	}
	return out, nil
}
func (e *Engine) CancelRelease(id string) {
	e.mu.Lock()
	defer e.mu.Unlock()
	if s := e.stages[id]; s != nil {
		e.dropStageLocked(s)
	}
}
func (e *Engine) CancelAdmission(id string) {
	e.mu.Lock()
	defer e.mu.Unlock()
	delete(e.pending, id)
	delete(e.admissions, id)
}
func (e *Engine) expired(deadline time.Duration) bool { return e.clock.Monotonic() >= deadline }
func (e *Engine) dropStageLocked(s *stage)            { delete(e.stages, s.id); e.stagedBytes -= len(s.result) }
func (e *Engine) sweepExpiredLocked() {
	for id, c := range e.pending {
		if e.expired(c.deadline) {
			delete(e.pending, id)
		}
	}
	for id, a := range e.admissions {
		if e.expired(a.deadline) {
			delete(e.admissions, id)
		}
	}
	for _, s := range e.stages {
		if e.expired(s.deadline) {
			e.dropStageLocked(s)
		}
	}
}
func (engine *Engine) makeChallenge(operation Operation, principal trust.Principal, keyID string, request Request, admissionID, resultDigest string) (Challenge, []byte, error) {
	if validatePrincipal(principal) != nil || validateRequest(request) != nil || operation != OperationAdmission && operation != OperationRelease {
		return Challenge{}, nil, ErrInvalid
	}
	id, err := engine.randomUUID()
	if err != nil {
		return Challenge{}, nil, err
	}
	nonce := make([]byte, 32)
	if err = engine.random(nonce); err != nil {
		return Challenge{}, nil, err
	}
	now := engine.clock.Now()
	expires := now.Add(ChallengeTTL)
	var payload any
	switch policy := request.policy.(type) {
	case assistantPolicy:
		source := request.Source
		wire := challengeWire{SchemaVersion: SchemaVersion, Operation: string(operation), ChallengeID: id, Nonce: encodeB64(nonce), KeyID: keyID, PersonID: principal.PersonID(), ClientID: principal.ClientID(), DeviceID: principal.DeviceID(), Audience: policy.Audience, Purpose: policy.Purpose, Consumer: policy.Consumer, IssuedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: expires.UnixMilli(), Source: &sourceWire{source.ConnectorID, source.ConnectionID, source.ExecutionOwner, source.Incarnation, source.Epoch}, Grant: &grantWire{policy.Grant.ID, policy.Grant.Incarnation, policy.Grant.Epoch}, Resources: append([]string(nil), policy.Resources...), QueryDigest: hex.EncodeToString(policy.QueryDigest[:]), MaxItems: policy.MaxItems, MaxBytes: policy.MaxBytes, AdmissionID: admissionID, ResultDigest: resultDigest}
		if validateWire(wire) != nil {
			return Challenge{}, nil, ErrInvalid
		}
		payload = wire
	case productCalendarPolicy:
		if policy.expires.Before(expires) {
			expires = policy.expires
		}
		if !expires.After(now) {
			return Challenge{}, nil, ErrExpired
		}
		wire := ProductCalendarChallenge{Version: 1, Operation: "day_calendar_admission", ChallengeID: id, Nonce: encodeB64(nonce), KeyID: keyID, Claims: policy.claims, IssuedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: expires.UnixMilli()}
		if operation == OperationRelease {
			wire.Operation = "day_calendar_release"
			wire.AdmissionID = admissionID
			wire.ResultSHA256 = resultDigest
		}
		payload = wire
	default:
		return Challenge{}, nil, ErrInvalid
	}
	data, err := json.Marshal(payload)
	if err != nil || len(data) > MaxChallengeBytes {
		return Challenge{}, nil, ErrInvalid
	}
	return Challenge{ID: id, Operation: operation, Bytes: data, BytesB64: encodeB64(data), ExpiresAt: expires}, data, nil
}

func (engine *Engine) randomUUID() (string, error) {
	randomBytes := make([]byte, 16)
	if err := engine.random(randomBytes); err != nil {
		return "", err
	}
	randomBytes[6] = (randomBytes[6] & 0x0f) | 0x40
	randomBytes[8] = (randomBytes[8] & 0x3f) | 0x80
	return formatUUID(randomBytes), nil
}

type challengeWire struct {
	SchemaVersion   int         `json:"v"`
	Operation       string      `json:"operation"`
	ChallengeID     string      `json:"challenge_id"`
	Nonce           string      `json:"nonce"`
	KeyID           string      `json:"key_id"`
	PersonID        string      `json:"person_id"`
	ClientID        string      `json:"client_id"`
	DeviceID        string      `json:"device_id"`
	Audience        string      `json:"audience"`
	Purpose         string      `json:"purpose"`
	Consumer        string      `json:"consumer"`
	Source          *sourceWire `json:"source,omitempty"`
	Grant           *grantWire  `json:"grant,omitempty"`
	Resources       []string    `json:"resources,omitempty"`
	QueryDigest     string      `json:"query_sha256,omitempty"`
	MaxItems        uint32      `json:"max_items,omitempty"`
	MaxBytes        uint32      `json:"max_bytes,omitempty"`
	ResultDigest    string      `json:"result_sha256,omitempty"`
	AdmissionID     string      `json:"admission_id,omitempty"`
	IssuedAtUnixMS  int64       `json:"issued_at_unix_ms"`
	ExpiresAtUnixMS int64       `json:"expires_at_unix_ms"`
}
type sourceWire struct {
	ConnectorID    string `json:"connector_id"`
	ConnectionID   string `json:"connection_id"`
	ExecutionOwner string `json:"execution_owner"`
	Incarnation    string `json:"incarnation"`
	Epoch          uint64 `json:"epoch"`
}
type grantWire struct {
	ID          string `json:"id"`
	Incarnation string `json:"incarnation"`
	Epoch       uint64 `json:"epoch"`
}

func parseChallengeBytes(data []byte) (challengeWire, error) {
	if len(data) > MaxChallengeBytes {
		return challengeWire{}, fmt.Errorf("%w: challenge too large", ErrInvalid)
	}
	if err := rejectDuplicateJSON(data); err != nil {
		return challengeWire{}, err
	}
	wire, err := decodeChallengeWire(data)
	if err != nil {
		return challengeWire{}, err
	}
	if err := validateWire(wire); err != nil {
		return challengeWire{}, err
	}
	return wire, nil
}

func decodeChallengeWire(data []byte) (challengeWire, error) {
	var wire challengeWire
	if trust.DecodeStrict(data, &wire, MaxChallengeBytes, MaxJSONDepth) != nil {
		return challengeWire{}, ErrInvalid
	}
	return wire, nil
}

func ParseChallengeBytes(data []byte) error { _, err := parseChallengeBytes(data); return err }

func validateWire(wire challengeWire) error {
	if wire.SchemaVersion != SchemaVersion {
		return fmt.Errorf("%w: version", ErrInvalid)
	}
	if wire.Operation != string(OperationEnrollment) && wire.Operation != string(OperationAdmission) && wire.Operation != string(OperationRelease) {
		return fmt.Errorf("%w: operation", ErrInvalid)
	}
	if err := validateUUID(wire.ChallengeID); err != nil {
		return fmt.Errorf("%w: challenge id", ErrInvalid)
	}
	if _, err := trust.DecodeBase64(wire.Nonce, 32); err != nil {
		return fmt.Errorf("%w: nonce", ErrInvalid)
	}
	if err := validateUUID(wire.KeyID); err != nil {
		return fmt.Errorf("%w: key id", ErrInvalid)
	}
	if err := validateUUID(wire.PersonID); err != nil {
		return fmt.Errorf("%w: person id", ErrInvalid)
	}
	if err := validateBoundString(wire.ClientID, 128); err != nil {
		return fmt.Errorf("%w: client id", ErrInvalid)
	}
	if err := validateBoundString(wire.DeviceID, 128); err != nil {
		return fmt.Errorf("%w: device id", ErrInvalid)
	}
	if err := validateBoundString(wire.Audience, MaxAudienceBytes); err != nil {
		return fmt.Errorf("%w: audience", ErrInvalid)
	}
	if err := validateBoundString(wire.Purpose, MaxPurposeBytes); err != nil {
		return fmt.Errorf("%w: purpose", ErrInvalid)
	}
	if wire.Operation == string(OperationEnrollment) && wire.Purpose != "owner_enrollment" {
		return fmt.Errorf("%w: purpose", ErrInvalid)
	}
	if wire.Operation != string(OperationEnrollment) && !validPurpose(wire.Purpose) {
		return fmt.Errorf("%w: purpose", ErrInvalid)
	}
	if err := validateBoundString(wire.Consumer, MaxConsumerBytes); err != nil {
		return fmt.Errorf("%w: consumer", ErrInvalid)
	}
	if wire.IssuedAtUnixMS <= 0 || wire.ExpiresAtUnixMS <= wire.IssuedAtUnixMS {
		return fmt.Errorf("%w: timestamps", ErrInvalid)
	}
	if wire.Operation == string(OperationEnrollment) {
		if wire.Source != nil || wire.Grant != nil || len(wire.Resources) != 0 || wire.QueryDigest != "" || wire.MaxItems != 0 || wire.MaxBytes != 0 || wire.ResultDigest != "" || wire.AdmissionID != "" {
			return fmt.Errorf("%w: enrollment fields", ErrInvalid)
		}
		return nil
	}
	if wire.Source == nil || wire.Grant == nil {
		return fmt.Errorf("%w: missing authority references", ErrInvalid)
	}
	if err := validateSource(*wire.Source); err != nil {
		return err
	}
	if err := validateGrant(*wire.Grant); err != nil {
		return err
	}
	if len(wire.Resources) == 0 || len(wire.Resources) > MaxResources {
		return fmt.Errorf("%w: resources", ErrInvalid)
	}
	previous := ""
	for _, resource := range wire.Resources {
		if err := validateBoundString(resource, MaxResourceBytes); err != nil {
			return fmt.Errorf("%w: resource", ErrInvalid)
		}
		if previous != "" && resource <= previous {
			return fmt.Errorf("%w: resources not sorted", ErrInvalid)
		}
		previous = resource
	}
	if _, err := decodeHexDigest(wire.QueryDigest); err != nil {
		return fmt.Errorf("%w: query digest", ErrInvalid)
	}
	if wire.MaxItems == 0 || wire.MaxItems > MaxResources*2 || wire.MaxBytes == 0 || wire.MaxBytes > MaxStageBytesPerResult {
		return fmt.Errorf("%w: budget", ErrInvalid)
	}
	if wire.Operation == string(OperationRelease) {
		if err := validateUUID(wire.AdmissionID); err != nil {
			return fmt.Errorf("%w: admission id", ErrInvalid)
		}
		if _, err := decodeHexDigest(wire.ResultDigest); err != nil {
			return fmt.Errorf("%w: result digest", ErrInvalid)
		}
	} else if wire.ResultDigest != "" || wire.AdmissionID != "" {
		return fmt.Errorf("%w: result digest", ErrInvalid)
	}
	return nil
}

func requestsEqual(a, b Request) bool { return reflect.DeepEqual(a, b) }
func cloneRequest(r Request) Request {
	r.Source = views.CloneSource(r.Source)
	if r.policy != nil {
		r.policy = r.policy.clonePolicy()
	}
	return r
}
func validateRequest(r Request) error {
	if !r.Source.Active || r.Source.ConnectionRevision == 0 || r.Source.PersonID == "" || r.Source.ProviderIdentity == "" || r.Source.IdentityGeneration == 0 || r.Source.Descriptor.ID == "" {
		return ErrInvalid
	}
	if err := validateSource(sourceWire{r.Source.ConnectorID, r.Source.ConnectionID, r.Source.ExecutionOwner, r.Source.Incarnation, r.Source.Epoch}); err != nil {
		return err
	}
	switch policy := r.policy.(type) {
	case assistantPolicy:
		if r.Source.Descriptor.ID == string(views.CalendarMirror) || validateBoundString(policy.Audience, MaxAudienceBytes) != nil || !validPurpose(policy.Purpose) || validateBoundString(policy.Consumer, MaxConsumerBytes) != nil || validateGrant(grantWire{policy.Grant.ID, policy.Grant.Incarnation, policy.Grant.Epoch}) != nil || len(policy.Resources) == 0 || len(policy.Resources) > MaxResources {
			return ErrInvalid
		}
		previous := ""
		for _, resource := range policy.Resources {
			if validateBoundString(resource, MaxResourceBytes) != nil || previous != "" && resource <= previous {
				return ErrInvalid
			}
			previous = resource
		}
	case productCalendarPolicy:
		if r.Source.Descriptor.ID != string(views.CalendarMirror) || policy.validateSource(r.Source) != nil {
			return ErrInvalid
		}
	default:
		return ErrInvalid
	}
	bounds := r.bounds()
	if bounds.MaxItems == 0 || bounds.MaxItems > 128 || bounds.MaxBytes == 0 || bounds.MaxBytes > MaxStageBytesPerResult {
		return ErrInvalid
	}
	return nil
}

func validPurpose(purpose string) bool {
	switch purpose {
	case "quick_response", "everyday_assistance", "deep_work":
		return true
	default:
		return false
	}
}
func validateSource(source sourceWire) error {
	for _, field := range []struct {
		value     string
		maxLength int
	}{{source.ConnectorID, 128}, {source.ConnectionID, 128}, {source.ExecutionOwner, 128}} {
		if err := validateBoundString(field.value, field.maxLength); err != nil {
			return fmt.Errorf("%w: source", ErrInvalid)
		}
	}
	if err := validateUUID(source.Incarnation); err != nil {
		return fmt.Errorf("%w: source incarnation", ErrInvalid)
	}
	if source.Epoch == 0 {
		return fmt.Errorf("%w: source epoch", ErrInvalid)
	}
	return nil
}
func validateGrant(g grantWire) error {
	if err := validateUUID(g.ID); err != nil {
		return fmt.Errorf("%w: grant", ErrInvalid)
	}
	if err := validateUUID(g.Incarnation); err != nil {
		return fmt.Errorf("%w: grant incarnation", ErrInvalid)
	}
	if g.Epoch == 0 {
		return fmt.Errorf("%w: grant epoch", ErrInvalid)
	}
	return nil
}
func sourceMatches(p trust.Principal, want, got views.SourceSnapshot) bool {
	return got.Active && got.PersonID == p.PersonID() && (got.DeviceID == "" || got.DeviceID == p.DeviceID()) && got.ConnectionRevision > 0 && got.ProviderIdentity != "" && got.IdentityGeneration > 0 && reflect.DeepEqual(want, got)
}
func validatePrincipal(p trust.Principal) error {
	if !p.Valid() {
		return fmt.Errorf("%w: unauthenticated principal", ErrDenied)
	}
	if err := validateBoundString(p.ClientID(), 128); err != nil {
		return err
	}
	if err := validateUUID(p.PersonID()); err != nil {
		return err
	}
	if err := validateBoundString(p.DeviceID(), 128); err != nil {
		return err
	}
	return nil
}
func samePrincipal(a, b trust.Principal) bool {
	return a.Same(b)
}
func challengeIDFromBytes(b []byte) string {
	var wire challengeWire
	if json.Unmarshal(b, &wire) == nil {
		return wire.ChallengeID
	}
	return ""
}
func encodeB64(b []byte) string { return base64.RawURLEncoding.EncodeToString(b) }
func decodeHexDigest(s string) ([]byte, error) {
	if len(s) != 64 {
		return nil, ErrInvalid
	}
	b, err := hex.DecodeString(s)
	if err != nil || hex.EncodeToString(b) != s {
		return nil, ErrInvalid
	}
	return b, nil
}
func validateBoundString(value string, maxLength int) error {
	if value == "" || len(value) > maxLength || strings.IndexByte(value, 0) >= 0 {
		return ErrInvalid
	}
	for _, character := range value {
		if character < 0x21 || character > 0x7e {
			return ErrInvalid
		}
	}
	return nil
}
func validateUUID(s string) error {
	if len(s) != 36 || s[8] != '-' || s[13] != '-' || s[18] != '-' || s[23] != '-' || strings.ToLower(s) != s {
		return ErrInvalid
	}
	raw := strings.ReplaceAll(s, "-", "")
	if len(raw) != 32 {
		return ErrInvalid
	}
	b, err := hex.DecodeString(raw)
	if err != nil {
		return ErrInvalid
	}
	var zero [16]byte
	if bytes.Equal(b, zero[:]) {
		return ErrInvalid
	}
	return nil
}
func formatUUID(b []byte) string {
	return fmt.Sprintf("%s-%s-%s-%s-%s", hex.EncodeToString(b[:4]), hex.EncodeToString(b[4:6]), hex.EncodeToString(b[6:8]), hex.EncodeToString(b[8:10]), hex.EncodeToString(b[10:]))
}

func rejectDuplicateJSON(data []byte) error {
	return trust.StrictJSON(data, MaxChallengeBytes, MaxJSONDepth)
}
