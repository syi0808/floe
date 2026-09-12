// Package authorization contains the transport-independent owner authorization
// protocol. It deliberately does not authenticate HTTP bearers or store private
// keys; those responsibilities remain with the console and the vault.
package authorization

import (
	"bytes"
	"crypto/ed25519"
	cryptorand "crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"
	"sync"
	"time"
)

const (
	SchemaVersion   = 1
	SignatureDomain = "floe.remote.authorization.v1\x00"

	ChallengeTTL           = 30 * time.Second
	MaxPendingPerClient    = 128
	MaxStageBytesPerResult = 1 << 20
	MaxStageBytesGlobal    = 16 << 20
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

type Options struct {
	Clock  Clock
	Random func([]byte) error
	Store  TrustStore
}

// Principal is trusted only when Authenticated is true and the caller has
// obtained it from the console's bearer/pairing verifier. This package never
// turns a bearer, or a client-provided identity, into an authenticated one.
type Principal struct {
	ClientID      string `json:"client_id"`
	PersonID      string `json:"person_id"`
	DeviceID      string `json:"device_id"`
	Authenticated bool   `json:"-"`
}

type SourceReference struct {
	ConnectorID    string
	ConnectionID   string
	ExecutionOwner string
	Incarnation    string
	Epoch          uint64
}

type SourceSnapshot struct {
	SourceReference
	PersonID string
	Active   bool
}

type PolicyReference struct {
	Incarnation string
	Epoch       uint64
}
type GrantReference struct {
	ID          string
	Incarnation string
	Epoch       uint64
}

type Request struct {
	Audience    string
	Purpose     string
	Consumer    string
	Policy      PolicyReference
	Source      SourceReference
	Grant       GrantReference
	Resources   []string
	QueryDigest [32]byte
	MaxItems    uint32
	MaxBytes    uint32
}

type IssuerRecord struct {
	KeyID     string
	Principal Principal
	PublicKey ed25519.PublicKey
}

// TrustStore is a durable transaction boundary. Implementations must commit
// activation/revocation atomically, including any tombstone needed to prevent
// key resurrection. CommitIssuerActivation must reject a key ID present in a
// durable revocation tombstone, even if IsIssuerKeyRevoked was previously false,
// and return only after the commit is durable. Engine state changes only after
// these methods return nil. Private keys never enter this API.
type TrustStore interface {
	LoadIssuers() ([]IssuerRecord, error)
	IsIssuerKeyRevoked(string) (bool, error)
	CommitIssuerActivation(IssuerRecord) error
	CommitIssuerRevocation(IssuerRecord) error
}

// SourceAuthority resolves the current source under the console owner lock.
// It must reject unknown/inactive pairings, keep the owner lock held while
// invoking consume, and return the current incarnation and epoch. Engine
// never calls it while holding its own mutex.
type SourceAuthority interface {
	WithCurrentSource(Principal, SourceReference, func(SourceSnapshot) error) error
}

type Challenge struct {
	ID        string
	Operation Operation
	Bytes     []byte
	BytesB64  string
	ExpiresAt time.Time
}

type Proof struct {
	ChallengeID string `json:"challenge_id"`
	KeyID       string `json:"key_id"`
	Signature   string `json:"signature"`
}

// ParseProofJSON rejects duplicate/unknown fields, trailing bytes, and
// non-canonical base64 before a proof reaches the engine.
func ParseProofJSON(data []byte) (Proof, error) {
	if err := rejectDuplicateJSON(data); err != nil {
		return Proof{}, err
	}
	var proof Proof
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&proof); err != nil {
		return Proof{}, fmt.Errorf("%w: proof: %v", ErrInvalid, err)
	}
	var extra any
	if err := decoder.Decode(&extra); err != io.EOF {
		return Proof{}, fmt.Errorf("%w: proof trailing bytes", ErrInvalid)
	}
	if err := validateUUID(proof.ChallengeID); err != nil {
		return Proof{}, fmt.Errorf("%w: proof challenge", ErrInvalid)
	}
	if err := validateUUID(proof.KeyID); err != nil {
		return Proof{}, fmt.Errorf("%w: proof key", ErrInvalid)
	}
	if _, err := decodeB64(proof.Signature, ed25519.SignatureSize); err != nil {
		return Proof{}, fmt.Errorf("%w: proof signature", ErrInvalid)
	}
	return proof, nil
}

type Enrollment struct {
	ID          string
	Fingerprint string
	Principal   Principal
	KeyID       string
}

type Release struct {
	ID        string
	Bytes     []byte
	BytesB64  string
	ExpiresAt time.Time
}

type Engine struct {
	mu                  sync.Mutex
	clock               Clock
	random              func([]byte) error
	store               TrustStore
	transitionMu        sync.Mutex
	issuers             map[string]IssuerRecord
	pending             map[string]*pendingChallenge
	pendingByClient     map[string]int
	enrollmentsByClient map[string]int
	admissionsByClient  map[string]int
	enrollments         map[string]*pendingEnrollment
	admissions          map[string]*admission
	stages              map[string]*stage
	stagedBytes         int
}

type pendingChallenge struct {
	wire         []byte
	operation    Operation
	principal    Principal
	request      Request
	keyID        string
	deadline     time.Duration
	deadlineWall time.Time
	state        challengeState
	enrollmentID string
}
type challengeState uint8

const (
	challengePending challengeState = iota
	challengeChecking
	challengeClaimed
)

type pendingEnrollment struct {
	id             string
	challengeID    string
	principal      Principal
	keyID          string
	publicKey      ed25519.PublicKey
	fingerprint    string
	adminApproved  bool
	localConfirmed bool
	committing     bool
	deadline       time.Duration
}

type stage struct {
	id           string
	admissionID  string
	principal    Principal
	request      Request
	result       []byte
	resultDigest string
	deadline     time.Duration
	deadlineWall time.Time
	challenge    []byte
	keyID        string
	state        challengeState
}

type admission struct {
	principal Principal
	request   Request
	keyID     string
	deadline  time.Duration
}

func New(opts Options) (*Engine, error) {
	clock := opts.Clock
	if clock == nil {
		clock = systemClock{started: time.Now()}
	}
	random := opts.Random
	if random == nil {
		random = func(b []byte) error { _, err := cryptorand.Read(b); return err }
	}
	if opts.Store == nil {
		return nil, fmt.Errorf("%w: trust store is required", ErrUnavailable)
	}
	loaded, err := opts.Store.LoadIssuers()
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrUnavailable, err)
	}
	engine := &Engine{clock: clock, random: random, store: opts.Store, issuers: make(map[string]IssuerRecord), pending: make(map[string]*pendingChallenge), pendingByClient: make(map[string]int), enrollmentsByClient: make(map[string]int), admissionsByClient: make(map[string]int), enrollments: make(map[string]*pendingEnrollment), admissions: make(map[string]*admission), stages: make(map[string]*stage)}
	for _, record := range loaded {
		if err := validateIssuer(record); err != nil {
			return nil, fmt.Errorf("%w: invalid stored issuer: %v", ErrUnavailable, err)
		}
		if _, ok := engine.issuers[record.KeyID]; ok {
			return nil, fmt.Errorf("%w: duplicate issuer", ErrUnavailable)
		}
		for _, existing := range engine.issuers {
			if samePrincipal(existing.Principal, record.Principal) {
				return nil, fmt.Errorf("%w: multiple active issuers", ErrUnavailable)
			}
		}
		engine.issuers[record.KeyID] = cloneIssuer(record)
	}
	return engine, nil
}

// BeginEnrollment creates a one-use proof-of-possession challenge. The
// principal must already be authenticated by the console, and the public key
// is only a claim until CompleteEnrollment verifies its signature. V1 permits
// one active issuer binding for each exact client/person/device principal.
func (engine *Engine) BeginEnrollment(principal Principal, keyID string, publicKey ed25519.PublicKey, audience string) (Enrollment, Challenge, error) {
	if err := validatePrincipal(principal); err != nil {
		return Enrollment{}, Challenge{}, err
	}
	if err := validateUUID(keyID); err != nil {
		return Enrollment{}, Challenge{}, fmt.Errorf("%w: key id", err)
	}
	revoked, err := engine.store.IsIssuerKeyRevoked(keyID)
	if err != nil {
		return Enrollment{}, Challenge{}, fmt.Errorf("%w: trust lookup: %v", ErrUnavailable, err)
	}
	if revoked {
		return Enrollment{}, Challenge{}, fmt.Errorf("%w: revoked key", ErrDenied)
	}
	if len(publicKey) != ed25519.PublicKeySize {
		return Enrollment{}, Challenge{}, fmt.Errorf("%w: public key", ErrInvalid)
	}
	if err := validateBoundString(audience, MaxAudienceBytes); err != nil {
		return Enrollment{}, Challenge{}, fmt.Errorf("%w: audience", err)
	}
	engine.mu.Lock()
	engine.sweepExpiredLocked()
	if _, exists := engine.issuers[keyID]; exists {
		engine.mu.Unlock()
		return Enrollment{}, Challenge{}, fmt.Errorf("%w: key already known", ErrConflict)
	}
	if engine.pendingByClient[principal.ClientID] >= MaxPendingPerClient {
		engine.mu.Unlock()
		return Enrollment{}, Challenge{}, fmt.Errorf("%w: pending challenge cap", ErrDenied)
	}
	if engine.enrollmentsByClient[principal.ClientID] >= MaxPendingPerClient {
		engine.mu.Unlock()
		return Enrollment{}, Challenge{}, fmt.Errorf("%w: enrollment cap", ErrDenied)
	}
	engine.mu.Unlock()

	challenge, wire, err := engine.makeChallenge(OperationEnrollment, principal, keyID, Request{Audience: audience, Purpose: "owner_enrollment", Consumer: "owner"}, nil)
	if err != nil {
		return Enrollment{}, Challenge{}, err
	}
	fingerprint := fingerprintPublicKey(publicKey)
	engine.mu.Lock()
	defer engine.mu.Unlock()
	engine.sweepExpiredLocked()
	if _, exists := engine.issuers[keyID]; exists {
		return Enrollment{}, Challenge{}, fmt.Errorf("%w: key already known", ErrConflict)
	}
	if engine.pendingByClient[principal.ClientID] >= MaxPendingPerClient {
		return Enrollment{}, Challenge{}, fmt.Errorf("%w: pending challenge cap", ErrDenied)
	}
	if engine.enrollmentsByClient[principal.ClientID] >= MaxPendingPerClient {
		return Enrollment{}, Challenge{}, fmt.Errorf("%w: enrollment cap", ErrDenied)
	}
	id := challenge.ID
	engine.pending[id] = &pendingChallenge{wire: append([]byte(nil), wire...), operation: OperationEnrollment, principal: principal, keyID: keyID, deadline: engine.clock.Monotonic() + ChallengeTTL, deadlineWall: challenge.ExpiresAt}
	challenge.Bytes = append([]byte(nil), challenge.Bytes...)
	engine.pendingByClient[principal.ClientID]++
	deadline := engine.clock.Monotonic() + ChallengeTTL
	engine.enrollments[id] = &pendingEnrollment{id: id, challengeID: id, principal: principal, keyID: keyID, publicKey: append(ed25519.PublicKey(nil), publicKey...), fingerprint: fingerprint, deadline: deadline}
	engine.enrollmentsByClient[principal.ClientID]++
	return Enrollment{ID: id, Fingerprint: fingerprint, Principal: principal, KeyID: keyID}, challenge, nil
}

// CompleteEnrollment verifies the local key's signature. It is the signed
// local confirmation. Admin approval remains a separate explicit operation.
func (engine *Engine) CompleteEnrollment(enrollmentID string, principal Principal, proof Proof) error {
	if err := validatePrincipal(principal); err != nil {
		return err
	}
	engine.mu.Lock()
	enrollment, ok := engine.enrollments[enrollmentID]
	if !ok || enrollment.committing {
		engine.mu.Unlock()
		return ErrDenied
	}
	if !samePrincipal(principal, enrollment.principal) {
		engine.mu.Unlock()
		return ErrDenied
	}
	if engine.expired(enrollment.deadline) {
		engine.dropEnrollmentLocked(enrollmentID)
		engine.mu.Unlock()
		return ErrExpired
	}
	challenge := engine.pending[enrollment.challengeID]
	if challenge == nil {
		engine.mu.Unlock()
		return ErrDenied
	}
	if engine.expired(challenge.deadline) {
		engine.dropEnrollmentLocked(enrollmentID)
		engine.mu.Unlock()
		return ErrExpired
	}
	if err := verifyProof(proof, challenge, enrollment.publicKey); err != nil {
		engine.mu.Unlock()
		return err
	}
	delete(engine.pending, enrollment.challengeID)
	if engine.pendingByClient[principal.ClientID] > 0 {
		engine.pendingByClient[principal.ClientID]--
	}
	enrollment.localConfirmed = true
	activate := enrollment.adminApproved
	if activate {
		enrollment.committing = true
	}
	record := IssuerRecord{KeyID: enrollment.keyID, Principal: enrollment.principal, PublicKey: append(ed25519.PublicKey(nil), enrollment.publicKey...)}
	engine.mu.Unlock()
	if !activate {
		return nil
	}
	return engine.commitActivation(enrollmentID, record)
}

// ApproveEnrollment records the explicit admin decision. The caller must
// perform admin authentication and exact fingerprint confirmation before this
// method; this package intentionally has no bearer/admin authority.
func (engine *Engine) ApproveEnrollment(enrollmentID, fingerprint string, approved bool) error {
	if !approved {
		engine.transitionMu.Lock()
		defer engine.transitionMu.Unlock()
		engine.mu.Lock()
		enrollment, exists := engine.enrollments[enrollmentID]
		if !exists || enrollment.committing {
			engine.mu.Unlock()
			return ErrConflict
		}
		engine.dropEnrollmentLocked(enrollmentID)
		engine.mu.Unlock()
		return fmt.Errorf("%w: enrollment not approved", ErrDenied)
	}
	engine.mu.Lock()
	enrollment, ok := engine.enrollments[enrollmentID]
	if !ok || enrollment.committing {
		engine.mu.Unlock()
		return ErrDenied
	}
	if engine.expired(enrollment.deadline) {
		engine.dropEnrollmentLocked(enrollmentID)
		engine.mu.Unlock()
		return ErrExpired
	}
	if fingerprint != enrollment.fingerprint {
		engine.mu.Unlock()
		return fmt.Errorf("%w: fingerprint mismatch", ErrDenied)
	}
	enrollment.adminApproved = true
	activate := enrollment.localConfirmed
	if activate {
		enrollment.committing = true
	}
	record := IssuerRecord{KeyID: enrollment.keyID, Principal: enrollment.principal, PublicKey: append(ed25519.PublicKey(nil), enrollment.publicKey...)}
	engine.mu.Unlock()
	if !activate {
		return nil
	}
	return engine.commitActivation(enrollmentID, record)
}

func (engine *Engine) commitActivation(enrollmentID string, record IssuerRecord) error {
	engine.transitionMu.Lock()
	defer engine.transitionMu.Unlock()
	engine.mu.Lock()
	enrollment := engine.enrollments[enrollmentID]
	if enrollment == nil || !enrollment.committing {
		engine.mu.Unlock()
		return ErrConflict
	}
	if engine.expired(enrollment.deadline) {
		engine.dropEnrollmentLocked(enrollmentID)
		engine.mu.Unlock()
		return ErrExpired
	}
	if _, exists := engine.issuers[record.KeyID]; exists {
		enrollment.committing = false
		engine.mu.Unlock()
		return ErrConflict
	}
	for _, existing := range engine.issuers {
		if samePrincipal(existing.Principal, record.Principal) {
			enrollment.committing = false
			engine.mu.Unlock()
			return ErrConflict
		}
	}
	engine.mu.Unlock()
	if err := engine.store.CommitIssuerActivation(record); err != nil {
		engine.mu.Lock()
		if enrollment := engine.enrollments[enrollmentID]; enrollment != nil {
			enrollment.committing = false
		}
		engine.mu.Unlock()
		return fmt.Errorf("%w: activation: %v", ErrUnavailable, err)
	}
	engine.mu.Lock()
	defer engine.mu.Unlock()
	enrollment = engine.enrollments[enrollmentID]
	if enrollment == nil {
		return ErrConflict
	}
	if _, exists := engine.issuers[record.KeyID]; exists {
		enrollment.committing = false
		return ErrConflict
	}
	for _, existing := range engine.issuers {
		if samePrincipal(existing.Principal, record.Principal) {
			enrollment.committing = false
			return ErrConflict
		}
	}
	engine.issuers[record.KeyID] = cloneIssuer(record)
	delete(engine.enrollments, enrollmentID)
	if engine.enrollmentsByClient[record.Principal.ClientID] > 0 {
		engine.enrollmentsByClient[record.Principal.ClientID]--
	}
	return nil
}

// RevokeIssuer serializes local revocation with admission/release checks and
// reports success only after the durable store commits the tombstone.
func (engine *Engine) RevokeIssuer(keyID string) error {
	if err := validateUUID(keyID); err != nil {
		return err
	}
	engine.transitionMu.Lock()
	defer engine.transitionMu.Unlock()
	engine.mu.Lock()
	record, ok := engine.issuers[keyID]
	if !ok {
		engine.mu.Unlock()
		return ErrDenied
	}
	delete(engine.issuers, keyID)
	engine.mu.Unlock()
	if err := engine.store.CommitIssuerRevocation(record); err != nil {
		engine.mu.Lock()
		engine.issuers[keyID] = record
		engine.mu.Unlock()
		return fmt.Errorf("%w: revocation: %v", ErrUnavailable, err)
	}
	return nil
}

// IssueAdmission creates the exact bytes Core must sign. It first resolves
// source authority outside the engine lock and compares that result to the
// requested reference; request fields are never authority by themselves.
func (engine *Engine) IssueAdmission(principal Principal, request Request, authority SourceAuthority) (Challenge, error) {
	if err := validatePrincipal(principal); err != nil {
		return Challenge{}, err
	}
	if authority == nil {
		return Challenge{}, fmt.Errorf("%w: source authority required", ErrDenied)
	}
	if err := validateRequest(request); err != nil {
		return Challenge{}, err
	}
	request = cloneRequest(request)
	var snapshot SourceSnapshot
	didSnapshot := false
	err := authority.WithCurrentSource(principal, request.Source, func(current SourceSnapshot) error { snapshot = current; didSnapshot = true; return nil })
	if err != nil || !didSnapshot || !sourceMatches(principal, request.Source, snapshot) {
		return Challenge{}, ErrDenied
	}
	engine.mu.Lock()
	engine.sweepExpiredLocked()
	issuer, ok := engine.issuerForPrincipalLocked(principal)
	if !ok {
		engine.mu.Unlock()
		return Challenge{}, ErrDenied
	}
	if engine.pendingByClient[principal.ClientID] >= MaxPendingPerClient {
		engine.mu.Unlock()
		return Challenge{}, fmt.Errorf("%w: pending challenge cap", ErrDenied)
	}
	engine.mu.Unlock()
	challenge, wire, err := engine.makeChallenge(OperationAdmission, principal, issuer.KeyID, request, &request.Source)
	if err != nil {
		return Challenge{}, err
	}
	engine.mu.Lock()
	defer engine.mu.Unlock()
	if _, ok := engine.issuerForPrincipalLocked(principal); !ok {
		return Challenge{}, ErrDenied
	}
	if engine.pendingByClient[principal.ClientID] >= MaxPendingPerClient {
		return Challenge{}, fmt.Errorf("%w: pending challenge cap", ErrDenied)
	}
	engine.pending[challenge.ID] = &pendingChallenge{wire: append([]byte(nil), wire...), operation: OperationAdmission, principal: principal, request: request, keyID: issuer.KeyID, deadline: engine.clock.Monotonic() + ChallengeTTL, deadlineWall: challenge.ExpiresAt}
	challenge.Bytes = append([]byte(nil), challenge.Bytes...)
	engine.pendingByClient[principal.ClientID]++
	return challenge, nil
}

// ClaimAdmission verifies one signature and rechecks source pairing/epoch at
// the admission linearization point. It returns no source content.
func (engine *Engine) ClaimAdmission(principal Principal, proof Proof, authority SourceAuthority) (string, Request, error) {
	if err := validatePrincipal(principal); err != nil {
		return "", Request{}, err
	}
	if authority == nil {
		return "", Request{}, fmt.Errorf("%w: source authority required", ErrDenied)
	}
	engine.mu.Lock()
	challenge, ok := engine.pending[proof.ChallengeID]
	if !ok {
		engine.mu.Unlock()
		return "", Request{}, ErrReplay
	}
	if challenge.operation != OperationAdmission || !samePrincipal(principal, challenge.principal) {
		engine.mu.Unlock()
		return "", Request{}, ErrDenied
	}
	if challenge.state != challengePending {
		engine.mu.Unlock()
		return "", Request{}, ErrReplay
	}
	if engine.expired(challenge.deadline) {
		engine.dropChallengeLocked(challenge)
		engine.mu.Unlock()
		return "", Request{}, ErrExpired
	}
	issuer, ok := engine.issuers[challenge.keyID]
	if !ok {
		engine.dropChallengeLocked(challenge)
		engine.mu.Unlock()
		return "", Request{}, ErrDenied
	}
	if err := verifyProof(proof, challenge, issuer.PublicKey); err != nil {
		engine.mu.Unlock()
		return "", Request{}, err
	}
	challenge.state = challengeChecking
	request := challenge.request
	engine.mu.Unlock()
	var claimErr error
	didConsume := false
	err := authority.WithCurrentSource(principal, request.Source, func(snapshot SourceSnapshot) error {
		didConsume = true
		engine.mu.Lock()
		defer engine.mu.Unlock()
		if err := sourceCheckAndAdmissionLocked(engine, challenge, proof.ChallengeID, principal, request, snapshot); err != nil {
			claimErr = err
			return err
		}
		return nil
	})
	if err != nil || claimErr != nil {
		if err != nil && claimErr == nil {
			engine.mu.Lock()
			if live, ok := engine.pending[proof.ChallengeID]; ok && live == challenge && live.state == challengeChecking {
				engine.dropChallengeLocked(live)
			}
			engine.mu.Unlock()
		}
		if err != nil {
			return "", Request{}, err
		}
		return "", Request{}, claimErr
	}
	if !didConsume {
		engine.mu.Lock()
		if live, ok := engine.pending[proof.ChallengeID]; ok && live == challenge && live.state == challengeChecking {
			engine.dropChallengeLocked(live)
		}
		engine.mu.Unlock()
		return "", Request{}, ErrDenied
	}
	return proof.ChallengeID, cloneRequest(request), nil
}

func sourceCheckAndAdmissionLocked(engine *Engine, challenge *pendingChallenge, challengeID string, principal Principal, request Request, snapshot SourceSnapshot) error {
	if !sourceMatches(principal, request.Source, snapshot) {
		engine.dropChallengeLocked(challenge)
		return ErrDenied
	}
	if engine.expired(challenge.deadline) {
		engine.dropChallengeLocked(challenge)
		return ErrExpired
	}
	current, ok := engine.issuers[challenge.keyID]
	if !ok || !samePrincipal(current.Principal, principal) {
		engine.dropChallengeLocked(challenge)
		return ErrDenied
	}
	if engine.admissionsByClient[principal.ClientID] >= MaxPendingPerClient {
		engine.dropChallengeLocked(challenge)
		return ErrDenied
	}
	delete(engine.pending, challengeID)
	if engine.pendingByClient[principal.ClientID] > 0 {
		engine.pendingByClient[principal.ClientID]--
	}
	challenge.state = challengeClaimed
	engine.admissions[challengeID] = &admission{principal: principal, request: cloneRequest(request), keyID: challenge.keyID, deadline: engine.clock.Monotonic() + ChallengeTTL}
	engine.admissionsByClient[principal.ClientID]++
	return nil
}

// StageResult copies bounded output and returns a separate release challenge.
// It does not release output and provider work must happen before this call.
func (engine *Engine) StageResult(admissionID string, principal Principal, request Request, result []byte, resultItems uint32) (Release, error) {
	if err := validatePrincipal(principal); err != nil {
		return Release{}, err
	}
	if err := validateRequest(request); err != nil {
		return Release{}, err
	}
	request = cloneRequest(request)
	if len(result) > MaxStageBytesPerResult {
		return Release{}, fmt.Errorf("%w: result too large", ErrDenied)
	}
	if len(result) > int(request.MaxBytes) || resultItems > request.MaxItems {
		return Release{}, fmt.Errorf("%w: result budget exceeded", ErrDenied)
	}
	engine.mu.Lock()
	engine.sweepExpiredLocked()
	claimed, ok := engine.admissions[admissionID]
	if !ok || !samePrincipal(claimed.principal, principal) || !requestsEqual(claimed.request, request) {
		engine.mu.Unlock()
		return Release{}, ErrDenied
	}
	issuer, ok := engine.issuers[claimed.keyID]
	if !ok {
		engine.mu.Unlock()
		return Release{}, ErrDenied
	}
	if engine.stagedBytes+len(result) > MaxStageBytesGlobal {
		engine.mu.Unlock()
		return Release{}, fmt.Errorf("%w: staged output cap", ErrDenied)
	}
	for _, staged := range engine.stages {
		if staged.admissionID == admissionID {
			engine.mu.Unlock()
			return Release{}, fmt.Errorf("%w: admission already staged", ErrConflict)
		}
	}
	engine.mu.Unlock()

	resultCopy := append([]byte(nil), result...)
	digest := sha256.Sum256(resultCopy)
	challenge, wire, err := engine.makeChallenge(OperationRelease, principal, issuer.KeyID, request, &request.Source)
	if err != nil {
		return Release{}, err
	}
	if err := rejectDuplicateJSON(wire); err != nil {
		return Release{}, err
	}
	parsed, err := decodeChallengeWire(wire)
	if err != nil {
		return Release{}, err
	}
	parsed.ResultDigest = hex.EncodeToString(digest[:])
	parsedBytes, err := json.Marshal(parsed)
	if err != nil {
		return Release{}, err
	}
	if err := validateWire(parsed); err != nil {
		return Release{}, err
	}
	challenge.Bytes, challenge.BytesB64 = parsedBytes, encodeB64(parsedBytes)
	engine.mu.Lock()
	defer engine.mu.Unlock()
	if _, ok := engine.issuers[issuer.KeyID]; !ok {
		return Release{}, ErrDenied
	}
	claimed, ok = engine.admissions[admissionID]
	if !ok || !samePrincipal(claimed.principal, principal) || !requestsEqual(claimed.request, request) {
		return Release{}, ErrDenied
	}
	if claimed.keyID != issuer.KeyID {
		return Release{}, ErrDenied
	}
	for _, staged := range engine.stages {
		if staged.admissionID == admissionID {
			return Release{}, fmt.Errorf("%w: admission already staged", ErrConflict)
		}
	}
	if engine.stagedBytes+len(resultCopy) > MaxStageBytesGlobal {
		return Release{}, fmt.Errorf("%w: staged output cap", ErrDenied)
	}
	deadline := engine.clock.Monotonic() + ChallengeTTL
	engine.stages[challenge.ID] = &stage{id: challenge.ID, admissionID: admissionID, principal: principal, request: request, result: resultCopy, resultDigest: parsed.ResultDigest, deadline: deadline, deadlineWall: challenge.ExpiresAt, challenge: parsedBytes, keyID: issuer.KeyID}
	engine.stagedBytes += len(resultCopy)
	delete(engine.admissions, admissionID)
	if engine.admissionsByClient[principal.ClientID] > 0 {
		engine.admissionsByClient[principal.ClientID]--
	}
	return Release{ID: challenge.ID, Bytes: append([]byte(nil), parsedBytes...), BytesB64: encodeB64(parsedBytes), ExpiresAt: challenge.ExpiresAt}, nil
}

// ClaimRelease performs the producer release linearization and returns output
// only after current source and issuer checks succeed. It consumes the stage.
func (engine *Engine) ClaimRelease(principal Principal, proof Proof, authority SourceAuthority) ([]byte, error) {
	if err := validatePrincipal(principal); err != nil {
		return nil, err
	}
	if authority == nil {
		return nil, fmt.Errorf("%w: source authority required", ErrDenied)
	}
	engine.mu.Lock()
	staged, ok := engine.stages[proof.ChallengeID]
	if !ok {
		engine.mu.Unlock()
		return nil, ErrReplay
	}
	if !samePrincipal(principal, staged.principal) {
		engine.mu.Unlock()
		return nil, ErrDenied
	}
	if engine.expired(staged.deadline) {
		engine.dropStageLocked(staged)
		engine.mu.Unlock()
		return nil, ErrExpired
	}
	if staged.state != challengePending {
		engine.mu.Unlock()
		return nil, ErrReplay
	}
	issuer, ok := engine.issuers[staged.keyID]
	if !ok {
		engine.dropStageLocked(staged)
		engine.mu.Unlock()
		return nil, ErrDenied
	}
	challenge := &pendingChallenge{wire: staged.challenge, operation: OperationRelease, principal: staged.principal, keyID: staged.keyID, deadline: staged.deadline}
	if err := verifyProof(proof, challenge, issuer.PublicKey); err != nil {
		engine.mu.Unlock()
		return nil, err
	}
	wire, err := parseChallengeBytes(staged.challenge)
	if err != nil || wire.Operation != string(OperationRelease) {
		engine.mu.Unlock()
		return nil, ErrInvalid
	}
	request := requestFromWire(wire)
	staged.state = challengeChecking
	engine.mu.Unlock()
	var result []byte
	didConsume := false
	err = authority.WithCurrentSource(principal, request.Source, func(snapshot SourceSnapshot) error {
		didConsume = true
		engine.mu.Lock()
		defer engine.mu.Unlock()
		live, ok := engine.stages[staged.id]
		if !ok || live != staged || live.state != challengeChecking {
			return ErrReplay
		}
		if !sourceMatches(principal, request.Source, snapshot) {
			engine.dropStageLocked(live)
			return ErrDenied
		}
		if engine.expired(live.deadline) {
			engine.dropStageLocked(live)
			return ErrExpired
		}
		if _, ok := engine.issuers[live.keyID]; !ok {
			engine.dropStageLocked(live)
			return ErrDenied
		}
		result = append([]byte(nil), live.result...)
		engine.dropStageLocked(live)
		return nil
	})
	if err != nil || !didConsume {
		if err != nil || !didConsume {
			engine.mu.Lock()
			if live, ok := engine.stages[staged.id]; ok && live == staged && live.state == challengeChecking {
				engine.dropStageLocked(live)
			}
			engine.mu.Unlock()
		}
		if err == nil {
			return nil, ErrDenied
		}
		return nil, err
	}
	return result, nil
}

func (engine *Engine) CancelRelease(id string) {
	engine.mu.Lock()
	if staged := engine.stages[id]; staged != nil {
		engine.dropStageLocked(staged)
	}
	engine.mu.Unlock()
}

func (engine *Engine) CancelAdmission(id string) {
	engine.mu.Lock()
	if admission := engine.admissions[id]; admission != nil {
		delete(engine.admissions, id)
		if engine.admissionsByClient[admission.principal.ClientID] > 0 {
			engine.admissionsByClient[admission.principal.ClientID]--
		}
	}
	engine.mu.Unlock()
}

func (engine *Engine) issuerForPrincipalLocked(principal Principal) (IssuerRecord, bool) {
	for _, issuer := range engine.issuers {
		if samePrincipal(issuer.Principal, principal) {
			return issuer, true
		}
	}
	return IssuerRecord{}, false
}
func (engine *Engine) expired(deadline time.Duration) bool {
	return engine.clock.Monotonic() >= deadline
}
func (engine *Engine) dropChallengeLocked(c *pendingChallenge) {
	delete(engine.pending, challengeIDFromBytes(c.wire))
	if engine.pendingByClient[c.principal.ClientID] > 0 {
		engine.pendingByClient[c.principal.ClientID]--
	}
}

func (engine *Engine) dropEnrollmentLocked(enrollmentID string) {
	enrollment := engine.enrollments[enrollmentID]
	if enrollment == nil {
		return
	}
	if challenge := engine.pending[enrollment.challengeID]; challenge != nil {
		engine.dropChallengeLocked(challenge)
	}
	delete(engine.enrollments, enrollmentID)
	if engine.enrollmentsByClient[enrollment.principal.ClientID] > 0 {
		engine.enrollmentsByClient[enrollment.principal.ClientID]--
	}
}
func (engine *Engine) dropStageLocked(s *stage) {
	delete(engine.stages, s.id)
	engine.stagedBytes -= len(s.result)
	if engine.stagedBytes < 0 {
		engine.stagedBytes = 0
	}
}

func (engine *Engine) sweepExpiredLocked() {
	for _, challenge := range engine.pending {
		if engine.expired(challenge.deadline) {
			engine.dropChallengeLocked(challenge)
		}
	}
	for _, staged := range engine.stages {
		if engine.expired(staged.deadline) {
			engine.dropStageLocked(staged)
		}
	}
	for id, admission := range engine.admissions {
		if engine.expired(admission.deadline) {
			delete(engine.admissions, id)
			if engine.admissionsByClient[admission.principal.ClientID] > 0 {
				engine.admissionsByClient[admission.principal.ClientID]--
			}
		}
	}
	for enrollmentID, enrollment := range engine.enrollments {
		if !enrollment.committing && engine.expired(enrollment.deadline) {
			engine.dropEnrollmentLocked(enrollmentID)
		}
	}
}

func (engine *Engine) makeChallenge(operation Operation, principal Principal, keyID string, request Request, source *SourceReference) (Challenge, []byte, error) {
	if err := validatePrincipal(principal); err != nil {
		return Challenge{}, nil, err
	}
	if operation != OperationEnrollment && source == nil {
		return Challenge{}, nil, fmt.Errorf("%w: source", ErrInvalid)
	}
	id, err := engine.randomUUID()
	if err != nil {
		return Challenge{}, nil, err
	}
	nonce := make([]byte, 32)
	if err := engine.random(nonce); err != nil {
		return Challenge{}, nil, err
	}
	now := engine.clock.Now()
	expires := now.Add(ChallengeTTL)
	wire := challengeWire{SchemaVersion: SchemaVersion, Operation: string(operation), ChallengeID: id, Nonce: encodeB64(nonce), KeyID: keyID, PersonID: principal.PersonID, ClientID: principal.ClientID, DeviceID: principal.DeviceID, Audience: request.Audience, Purpose: request.Purpose, Consumer: request.Consumer, IssuedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: expires.UnixMilli()}
	if operation != OperationEnrollment {
		wire.Policy = &policyWire{Incarnation: request.Policy.Incarnation, Epoch: request.Policy.Epoch}
		wire.Source = &sourceWire{ConnectorID: source.ConnectorID, ConnectionID: source.ConnectionID, ExecutionOwner: source.ExecutionOwner, Incarnation: source.Incarnation, Epoch: source.Epoch}
		wire.Grant = &grantWire{ID: request.Grant.ID, Incarnation: request.Grant.Incarnation, Epoch: request.Grant.Epoch}
		wire.Resources = append([]string(nil), request.Resources...)
		wire.QueryDigest = hex.EncodeToString(request.QueryDigest[:])
		wire.MaxItems = request.MaxItems
		wire.MaxBytes = request.MaxBytes
	}
	data, err := json.Marshal(wire)
	if err != nil {
		return Challenge{}, nil, err
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
	Policy          *policyWire `json:"policy,omitempty"`
	Source          *sourceWire `json:"source,omitempty"`
	Grant           *grantWire  `json:"grant,omitempty"`
	Resources       []string    `json:"resources,omitempty"`
	QueryDigest     string      `json:"query_sha256,omitempty"`
	MaxItems        uint32      `json:"max_items,omitempty"`
	MaxBytes        uint32      `json:"max_bytes,omitempty"`
	ResultDigest    string      `json:"result_sha256,omitempty"`
	IssuedAtUnixMS  int64       `json:"issued_at_unix_ms"`
	ExpiresAtUnixMS int64       `json:"expires_at_unix_ms"`
}
type policyWire struct {
	Incarnation string `json:"incarnation"`
	Epoch       uint64 `json:"epoch"`
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
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&wire); err != nil {
		return challengeWire{}, fmt.Errorf("%w: %v", ErrInvalid, err)
	}
	var extra any
	if err := decoder.Decode(&extra); err != io.EOF {
		return challengeWire{}, fmt.Errorf("%w: trailing bytes", ErrInvalid)
	}
	return wire, nil
}

// ParseChallengeBytes exposes the same strict parser Core/fixture consumers
// should use before signing; callers must sign the supplied bytes unchanged.
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
	if _, err := decodeB64(wire.Nonce, 32); err != nil {
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
		if wire.Policy != nil || wire.Source != nil || wire.Grant != nil || len(wire.Resources) != 0 || wire.QueryDigest != "" || wire.MaxItems != 0 || wire.MaxBytes != 0 || wire.ResultDigest != "" {
			return fmt.Errorf("%w: enrollment fields", ErrInvalid)
		}
		return nil
	}
	if wire.Policy == nil || wire.Source == nil || wire.Grant == nil {
		return fmt.Errorf("%w: missing authority references", ErrInvalid)
	}
	if err := validatePolicy(*wire.Policy); err != nil {
		return err
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
		if _, err := decodeHexDigest(wire.ResultDigest); err != nil {
			return fmt.Errorf("%w: result digest", ErrInvalid)
		}
	} else if wire.ResultDigest != "" {
		return fmt.Errorf("%w: result digest", ErrInvalid)
	}
	return nil
}

func requestFromWire(wire challengeWire) Request {
	var digest [32]byte
	digestBytes, _ := decodeHexDigest(wire.QueryDigest)
	copy(digest[:], digestBytes)
	return Request{Audience: wire.Audience, Purpose: wire.Purpose, Consumer: wire.Consumer, Policy: PolicyReference{wire.Policy.Incarnation, wire.Policy.Epoch}, Source: SourceReference{wire.Source.ConnectorID, wire.Source.ConnectionID, wire.Source.ExecutionOwner, wire.Source.Incarnation, wire.Source.Epoch}, Grant: GrantReference{wire.Grant.ID, wire.Grant.Incarnation, wire.Grant.Epoch}, Resources: append([]string(nil), wire.Resources...), QueryDigest: digest, MaxItems: wire.MaxItems, MaxBytes: wire.MaxBytes}
}

func requestsEqual(a, b Request) bool {
	return a.Audience == b.Audience && a.Purpose == b.Purpose && a.Consumer == b.Consumer &&
		a.Policy == b.Policy && a.Source == b.Source && a.Grant == b.Grant &&
		a.MaxItems == b.MaxItems && a.MaxBytes == b.MaxBytes &&
		bytes.Equal(a.QueryDigest[:], b.QueryDigest[:]) && slicesEqual(a.Resources, b.Resources)
}

func cloneRequest(request Request) Request {
	request.Resources = append([]string(nil), request.Resources...)
	return request
}

func slicesEqual(first, second []string) bool {
	if len(first) != len(second) {
		return false
	}
	for index := range first {
		if first[index] != second[index] {
			return false
		}
	}
	return true
}

func validateRequest(r Request) error {
	if err := validateBoundString(r.Audience, MaxAudienceBytes); err != nil {
		return err
	}
	if !validPurpose(r.Purpose) {
		return fmt.Errorf("%w: purpose", ErrInvalid)
	}
	if err := validateBoundString(r.Purpose, MaxPurposeBytes); err != nil {
		return err
	}
	if err := validateBoundString(r.Consumer, MaxConsumerBytes); err != nil {
		return err
	}
	if err := validatePolicy(policyWire{r.Policy.Incarnation, r.Policy.Epoch}); err != nil {
		return err
	}
	if err := validateSource(sourceWire{r.Source.ConnectorID, r.Source.ConnectionID, r.Source.ExecutionOwner, r.Source.Incarnation, r.Source.Epoch}); err != nil {
		return err
	}
	if err := validateGrant(grantWire{r.Grant.ID, r.Grant.Incarnation, r.Grant.Epoch}); err != nil {
		return err
	}
	if len(r.Resources) == 0 || len(r.Resources) > MaxResources {
		return fmt.Errorf("%w: resources", ErrInvalid)
	}
	previous := ""
	for _, resource := range r.Resources {
		if err := validateBoundString(resource, MaxResourceBytes); err != nil {
			return err
		}
		if previous != "" && resource <= previous {
			return fmt.Errorf("%w: resources not sorted", ErrInvalid)
		}
		previous = resource
	}
	if r.MaxItems == 0 || r.MaxItems > MaxResources*2 || r.MaxBytes == 0 || r.MaxBytes > MaxStageBytesPerResult {
		return fmt.Errorf("%w: budget", ErrInvalid)
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
func validatePolicy(p policyWire) error {
	if err := validateUUID(p.Incarnation); err != nil {
		return fmt.Errorf("%w: policy", ErrInvalid)
	}
	if p.Epoch == 0 {
		return fmt.Errorf("%w: policy epoch", ErrInvalid)
	}
	return nil
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
func sourceMatches(p Principal, want SourceReference, got SourceSnapshot) bool {
	return got.Active && got.PersonID == p.PersonID && got.ConnectorID == want.ConnectorID && got.ConnectionID == want.ConnectionID && got.ExecutionOwner == want.ExecutionOwner && got.Incarnation == want.Incarnation && got.Epoch == want.Epoch
}
func validateIssuer(r IssuerRecord) error {
	if err := validateUUID(r.KeyID); err != nil {
		return err
	}
	if err := validatePrincipal(r.Principal); err != nil {
		return err
	}
	if len(r.PublicKey) != ed25519.PublicKeySize {
		return fmt.Errorf("%w: public key", ErrInvalid)
	}
	return nil
}
func validatePrincipal(p Principal) error {
	if !p.Authenticated {
		return fmt.Errorf("%w: unauthenticated principal", ErrDenied)
	}
	if err := validateBoundString(p.ClientID, 128); err != nil {
		return err
	}
	if err := validateUUID(p.PersonID); err != nil {
		return err
	}
	if err := validateBoundString(p.DeviceID, 128); err != nil {
		return err
	}
	return nil
}
func samePrincipal(a, b Principal) bool {
	return a.ClientID == b.ClientID && a.PersonID == b.PersonID && a.DeviceID == b.DeviceID
}
func cloneIssuer(r IssuerRecord) IssuerRecord {
	r.PublicKey = append(ed25519.PublicKey(nil), r.PublicKey...)
	return r
}
func fingerprintPublicKey(publicKey ed25519.PublicKey) string {
	digest := sha256.Sum256(publicKey)
	return hex.EncodeToString(digest[:])
}
func verifyProof(proof Proof, c *pendingChallenge, key ed25519.PublicKey) error {
	if proof.ChallengeID == "" || proof.ChallengeID != challengeIDFromBytes(c.wire) || proof.KeyID != c.keyID {
		return fmt.Errorf("%w: proof binding", ErrDenied)
	}
	sig, err := decodeB64(proof.Signature, ed25519.SignatureSize)
	if err != nil {
		return fmt.Errorf("%w: signature", ErrInvalid)
	}
	signed := append([]byte(SignatureDomain), c.wire...)
	if !ed25519.Verify(key, signed, sig) {
		return fmt.Errorf("%w: signature", ErrDenied)
	}
	return nil
}
func challengeIDFromBytes(b []byte) string {
	var wire challengeWire
	if json.Unmarshal(b, &wire) == nil {
		return wire.ChallengeID
	}
	return ""
}
func encodeB64(b []byte) string { return base64.RawURLEncoding.EncodeToString(b) }
func decodeB64(value string, expectedLength int) ([]byte, error) {
	if value == "" || strings.Contains(value, "=") {
		return nil, ErrInvalid
	}
	decoded, err := base64.RawURLEncoding.DecodeString(value)
	if err != nil || len(decoded) != expectedLength || base64.RawURLEncoding.EncodeToString(decoded) != value {
		return nil, ErrInvalid
	}
	return decoded, nil
}
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
	decoder := json.NewDecoder(bytes.NewReader(data))
	if err := walkJSON(decoder); err != nil {
		return fmt.Errorf("%w: %v", ErrInvalid, err)
	}
	var extra any
	if err := decoder.Decode(&extra); err != io.EOF {
		return fmt.Errorf("%w: trailing bytes", ErrInvalid)
	}
	return nil
}
func walkJSON(decoder *json.Decoder) error {
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	switch delimiter := token.(type) {
	case json.Delim:
		if delimiter == '{' {
			seen := map[string]bool{}
			for decoder.More() {
				key, err := decoder.Token()
				if err != nil {
					return err
				}
				ks, ok := key.(string)
				if !ok || seen[ks] {
					return ErrInvalid
				}
				seen[ks] = true
				if err := walkJSON(decoder); err != nil {
					return err
				}
			}
			_, err = decoder.Token()
			return err
		}
		if delimiter == '[' {
			for decoder.More() {
				if err := walkJSON(decoder); err != nil {
					return err
				}
			}
			_, err = decoder.Token()
			return err
		}
	}
	return nil
}
