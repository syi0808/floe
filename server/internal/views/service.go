package views

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"reflect"
	"sync"
	"time"

	sourcecontract "floe/server/internal/contracts/source"
	"floe/server/internal/trust"
)

const (
	challengeTTL             = 30 * time.Second
	viewStateSweepInterval   = time.Second
	maxViewPreviewProofBytes = 16 << 10
	maxViewResultBytes       = 1 << 20
	maxViewItems             = 128
	maxViewResourceBytes     = 256
)

type ErrorCategory string

const (
	ErrorInvalid     ErrorCategory = "invalid"
	ErrorDenied      ErrorCategory = "denied"
	ErrorConflict    ErrorCategory = "conflict"
	ErrorLimited     ErrorCategory = "limited"
	ErrorUnavailable ErrorCategory = "unavailable"
	ErrorUpstream    ErrorCategory = "upstream"
	ErrorInternal    ErrorCategory = "internal"
)

type Error struct {
	Category ErrorCategory
	Code     string
}

func (failure Error) Error() string { return failure.Code }

func fail(category ErrorCategory, code string) error {
	return Error{Category: category, Code: code}
}

type EnforcementFailure interface {
	error
	Category() string
	Code() string
}

// Enforcement is the Views-owned inward port implemented by Authority. It
// accepts an admission intent and returns only verified source/query/bounds
// bindings; Authority Request and policy types never cross this boundary.
type Enforcement interface {
	IssueViewAdmission(
		trust.Principal,
		sourcecontract.Snapshot,
		string, string, string, string, uint64,
		[]string, [32]byte, sourcecontract.Bounds,
	) (string, []byte, time.Time, error)
	CancelViewAdmission(string)
	ClaimViewAdmission(trust.Principal, sourcecontract.ID, trust.Proof) (string, sourcecontract.Snapshot, [32]byte, sourcecontract.Bounds, error)
	StageViewResult(string, trust.Principal, []byte, uint32) (string, []byte, time.Time, error)
	CancelViewRelease(string)
	ClaimViewRelease(context.Context, trust.Principal, sourcecontract.ID, trust.Proof) ([]byte, error)
}

type Clock interface{ Now() time.Time }

type wallClock struct{}

func (wallClock) Now() time.Time { return time.Now() }

type ProducerTrust interface {
	ProducerMetadata() (trust.ProducerMetadata, error)
	SignProducerChallenge([]byte) ([]byte, error)
}

type PreviewRequest struct {
	ConnectorID  string
	ConnectionID string
	Resource     string
}

type GrantReference struct {
	ID, Incarnation string
	Epoch           uint64
}

type AdmissionRequest struct {
	SchemaVersion      int
	ConnectorID        string
	ConnectionID       string
	ConnectionRevision uint64
	Resources          []string
	Grant              GrantReference
	Purpose            string
	Consumer           string
	MaxItems           uint32
	MaxBytes           uint32
	Query              []byte
}

type SourcePreviewResult struct {
	Producer           trust.ProducerMetadata
	Descriptor         string
	Signature          string
	ExpiresAtUnixMS    int64
	ConnectionRevision uint64
	SourceResources    []string
}

type ChallengeResult struct {
	SchemaVersion int
	Operation     string
	ChallengeID   string
	Challenge     string
	Signature     string
	Producer      trust.ProducerMetadata
	Expires       time.Time
}

type ReleaseResult struct {
	SchemaVersion int
	View          Result
}

type admissionState struct {
	viewID     ID
	query      ParsedQuery
	principal  trust.Principal
	source     SourceSnapshot
	reader     Reader
	bounds     Bounds
	expires    time.Time
	claiming   bool
	claimToken *claimToken
}

type releaseState struct{ admissionState }

type claimToken struct{}

type Service struct {
	enforcement Enforcement
	trust       ProducerTrust
	resolver    Resolver
	clock       Clock
	mu          sync.Mutex
	admissions  map[string]admissionState
	releases    map[string]releaseState
	lifetime    context.Context
	sweepCancel context.CancelFunc
	sweepDone   chan struct{}
	active      sync.WaitGroup
	closed      bool
	closeOnce   sync.Once
}

func NewService(enforcement Enforcement, producer ProducerTrust, resolver Resolver) (*Service, error) {
	return NewServiceWithClock(enforcement, producer, resolver, wallClock{})
}

func NewServiceWithClock(enforcement Enforcement, producer ProducerTrust, resolver Resolver, clock Clock) (*Service, error) {
	if enforcement == nil || producer == nil || resolver == nil || clock == nil {
		return nil, errors.New("view service unavailable")
	}
	ctx, cancel := context.WithCancel(context.Background())
	service := &Service{enforcement: enforcement, trust: producer, resolver: resolver, clock: clock, admissions: map[string]admissionState{}, releases: map[string]releaseState{}, lifetime: ctx, sweepCancel: cancel, sweepDone: make(chan struct{})}
	go service.sweepLoop(ctx)
	return service, nil
}

// Close cancels operation contexts and drains admitted calls before clearing
// state. A Reader that ignores its context can delay this drain.
func (service *Service) Close() {
	service.closeOnce.Do(func() {
		service.mu.Lock()
		service.closed = true
		service.mu.Unlock()
		service.sweepCancel()
		<-service.sweepDone
		service.active.Wait()
		var admissions, releases []string
		service.mu.Lock()
		for id := range service.admissions {
			admissions = append(admissions, id)
			delete(service.admissions, id)
		}
		for id := range service.releases {
			releases = append(releases, id)
			delete(service.releases, id)
		}
		service.mu.Unlock()
		for _, id := range admissions {
			service.enforcement.CancelViewAdmission(id)
		}
		for _, id := range releases {
			service.enforcement.CancelViewRelease(id)
		}
	})
}

func (service *Service) beginOperation(parent context.Context) (context.Context, func(), bool) {
	service.mu.Lock()
	if service.closed {
		service.mu.Unlock()
		return nil, nil, false
	}
	service.active.Add(1)
	lifetime := service.lifetime
	service.mu.Unlock()
	ctx, cancel := context.WithCancel(parent)
	stop := context.AfterFunc(lifetime, cancel)
	if lifetime.Err() != nil {
		cancel()
	}
	return ctx, func() {
		stop()
		cancel()
		service.active.Done()
	}, true
}

func (service *Service) sweepLoop(ctx context.Context) {
	defer close(service.sweepDone)
	ticker := time.NewTicker(viewStateSweepInterval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			service.sweep(service.clock.Now())
		}
	}
}

func (service *Service) Preview(ctx context.Context, principal trust.Principal, id string, request PreviewRequest) (SourcePreviewResult, error) {
	var done func()
	var ok bool
	ctx, done, ok = service.beginOperation(ctx)
	if !ok {
		return SourcePreviewResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	defer done()
	service.sweep(service.clock.Now())
	ctx, cancel := context.WithTimeout(ctx, challengeTTL)
	defer cancel()
	if !trust.ValidID(request.ConnectionID) || request.ConnectorID == "" || request.Resource != id+":"+request.ConnectionID || len(request.Resource) > maxViewResourceBytes {
		return SourcePreviewResult{}, fail(ErrorInvalid, "validation")
	}
	resolved, err := service.resolver.ResolveSource(ctx, principal, SourceTarget{ViewID: ID(id), ConnectorID: request.ConnectorID, ConnectionID: request.ConnectionID, ResourceID: request.Resource})
	if err != nil {
		return SourcePreviewResult{}, fail(ErrorConflict, "source_unavailable")
	}
	snapshot := resolved.Snapshot
	if err = service.resolver.PreflightSource(ctx, principal, snapshot); err != nil {
		return SourcePreviewResult{}, fail(ErrorUnavailable, "source_identity_unavailable")
	}
	metadata, err := service.trust.ProducerMetadata()
	if err != nil {
		return SourcePreviewResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	now := service.clock.Now()
	descriptor := sourcePreviewDescriptor{
		Audience: metadata.Audience, ChallengeID: trust.NewID(), Operation: "remote_view_source_preview", Nonce: trust.Token(),
		ViewID: id, PersonID: principal.PersonID(), ClientID: principal.ClientID(), DeviceID: principal.DeviceID(),
		ConnectorID: snapshot.ConnectorID, ConnectionID: snapshot.ConnectionID, ConnectionRevision: snapshot.ConnectionRevision,
		ExecutionOwner: snapshot.ExecutionOwner, Incarnation: snapshot.Incarnation, Epoch: snapshot.Epoch, Resource: request.Resource,
		SourceResources: snapshot.Resources, ProviderIdentity: snapshot.ProviderIdentity, IssuedAtUnixMS: now.UnixMilli(), Version: 1,
	}
	raw, err := json.Marshal(descriptor)
	if err != nil || len(raw) > maxViewPreviewProofBytes {
		return SourcePreviewResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	signature, err := service.trust.SignProducerChallenge(raw)
	if err != nil {
		return SourcePreviewResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	if err = service.resolver.PreflightSource(ctx, principal, snapshot); err != nil {
		return SourcePreviewResult{}, fail(ErrorConflict, "source_changed")
	}
	return SourcePreviewResult{metadata, base64.RawURLEncoding.EncodeToString(raw), base64.RawURLEncoding.EncodeToString(signature), now.Add(challengeTTL).UnixMilli(), snapshot.ConnectionRevision, append([]string(nil), snapshot.Resources...)}, nil
}

type sourcePreviewDescriptor struct {
	Audience           string   `json:"audience"`
	ChallengeID        string   `json:"challenge_id"`
	ClientID           string   `json:"client_id"`
	ConnectionID       string   `json:"connection_id"`
	ConnectionRevision uint64   `json:"connection_revision"`
	ConnectorID        string   `json:"connector_id"`
	DeviceID           string   `json:"device_id"`
	Epoch              uint64   `json:"epoch"`
	ExecutionOwner     string   `json:"execution_owner"`
	Incarnation        string   `json:"incarnation"`
	IssuedAtUnixMS     int64    `json:"issued_at_unix_ms"`
	Nonce              string   `json:"nonce"`
	Operation          string   `json:"operation"`
	PersonID           string   `json:"person_id"`
	ProviderIdentity   string   `json:"provider_identity"`
	Resource           string   `json:"resource"`
	SourceResources    []string `json:"source_resources"`
	Version            int      `json:"v"`
	ViewID             string   `json:"view_id"`
}

func (service *Service) Admit(ctx context.Context, principal trust.Principal, id string, input AdmissionRequest) (ChallengeResult, error) {
	var done func()
	var ok bool
	ctx, done, ok = service.beginOperation(ctx)
	if !ok {
		return ChallengeResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	defer done()
	ctx, cancel := context.WithTimeout(ctx, challengeTTL)
	defer cancel()
	service.sweep(service.clock.Now())
	resource := id + ":" + input.ConnectionID
	if input.SchemaVersion != 1 || !trust.ValidID(input.ConnectionID) || input.ConnectionRevision == 0 || len(input.Resources) != 1 || input.Resources[0] != resource || len(resource) > maxViewResourceBytes || input.MaxItems == 0 || input.MaxItems > maxViewItems || input.MaxBytes == 0 || input.MaxBytes > maxViewResultBytes {
		return ChallengeResult{}, fail(ErrorInvalid, "validation")
	}
	query, err := ParseQuery(ID(id), input.Query)
	if err != nil {
		return ChallengeResult{}, fail(ErrorInvalid, "validation")
	}
	// Preserve the exact Rust-validated bytes for the authority digest/signature.
	query.Canonical = append([]byte(nil), input.Query...)
	query.Digest = sha256.Sum256(query.Canonical)
	resolved, err := service.resolver.ResolveSource(ctx, principal, SourceTarget{ViewID: ID(id), ConnectorID: input.ConnectorID, ConnectionID: input.ConnectionID, ConnectionRevision: input.ConnectionRevision, ResourceID: resource})
	if err != nil {
		return ChallengeResult{}, fail(ErrorConflict, "connection_changed")
	}
	bounds := Bounds{MaxItems: min(input.MaxItems, resolved.Limits.MaxItems), MaxBytes: min(input.MaxBytes, resolved.Limits.MaxBytes)}
	if bounds.MaxItems != input.MaxItems || bounds.MaxBytes != input.MaxBytes || ValidateQueryBounds(query.Query, bounds) != nil {
		return ChallengeResult{}, fail(ErrorInvalid, "query_budget")
	}
	if err = service.resolver.PreflightSource(ctx, principal, resolved.Snapshot); err != nil {
		return ChallengeResult{}, fail(ErrorUnavailable, "source_identity_unavailable")
	}
	idValue, challenge, expires, err := service.enforcement.IssueViewAdmission(principal, resolved.Snapshot, input.Purpose, input.Consumer, input.Grant.ID, input.Grant.Incarnation, input.Grant.Epoch, input.Resources, query.Digest, bounds)
	if err != nil {
		var enforcementFailure EnforcementFailure
		if errors.As(err, &enforcementFailure) {
			return ChallengeResult{}, fail(ErrorCategory(enforcementFailure.Category()), enforcementFailure.Code())
		}
		return ChallengeResult{}, fail(ErrorDenied, "admission_denied")
	}
	if err = ctx.Err(); err != nil {
		service.enforcement.CancelViewAdmission(idValue)
		return ChallengeResult{}, fail(ErrorUnavailable, "cancelled")
	}
	metadata, err := service.trust.ProducerMetadata()
	if err != nil {
		service.enforcement.CancelViewAdmission(idValue)
		return ChallengeResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	signature, err := service.trust.SignProducerChallenge(challenge)
	if err != nil {
		service.enforcement.CancelViewAdmission(idValue)
		return ChallengeResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	if err = ctx.Err(); err != nil {
		service.enforcement.CancelViewAdmission(idValue)
		return ChallengeResult{}, fail(ErrorUnavailable, "cancelled")
	}
	service.sweep(service.clock.Now())
	if !expires.After(service.clock.Now()) {
		service.enforcement.CancelViewAdmission(idValue)
		return ChallengeResult{}, fail(ErrorConflict, "admission_unavailable")
	}
	state := admissionState{viewID: ID(id), query: query, principal: principal, source: CloneSource(resolved.Snapshot), reader: resolved.Reader, bounds: bounds, expires: expires}
	service.mu.Lock()
	closed := service.closed
	if closed || !expires.After(service.clock.Now()) {
		service.mu.Unlock()
		service.enforcement.CancelViewAdmission(idValue)
		if closed {
			return ChallengeResult{}, fail(ErrorUnavailable, "cancelled")
		}
		return ChallengeResult{}, fail(ErrorConflict, "admission_unavailable")
	}
	service.admissions[idValue] = state
	service.mu.Unlock()
	return ChallengeResult{1, "admission", idValue, base64.RawURLEncoding.EncodeToString(challenge), base64.RawURLEncoding.EncodeToString(signature), metadata, expires}, nil
}

func (service *Service) Read(ctx context.Context, principal trust.Principal, id string, proof trust.Proof) (ChallengeResult, error) {
	var done func()
	var ok bool
	ctx, done, ok = service.beginOperation(ctx)
	if !ok {
		return ChallengeResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	defer done()
	if err := ctx.Err(); err != nil {
		return readFailure(err)
	}
	service.sweep(service.clock.Now())
	service.mu.Lock()
	if service.closed {
		service.mu.Unlock()
		return ChallengeResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	state, ok := service.admissions[proof.ChallengeID]
	if !ok || state.viewID != ID(id) || !state.principal.Same(principal) {
		service.mu.Unlock()
		return ChallengeResult{}, fail(ErrorDenied, "admission_denied")
	}
	if state.claiming {
		service.mu.Unlock()
		return ChallengeResult{}, fail(ErrorConflict, "admission_in_progress")
	}
	token := &claimToken{}
	state.claiming, state.claimToken = true, token
	service.admissions[proof.ChallengeID] = state
	service.mu.Unlock()
	if !state.expires.After(service.clock.Now()) {
		service.finishAdmissionClaim(proof.ChallengeID, token, true)
		service.enforcement.CancelViewAdmission(proof.ChallengeID)
		return ChallengeResult{}, fail(ErrorConflict, "admission_unavailable")
	}
	admissionID, authorizedSource, authorizedDigest, authorizedBounds, err := service.enforcement.ClaimViewAdmission(principal, ID(id), proof)
	if err != nil {
		service.finishAdmissionClaim(proof.ChallengeID, token, false)
		if ctx.Err() != nil {
			return readFailure(ctx.Err())
		}
		return ChallengeResult{}, fail(ErrorDenied, "admission_denied")
	}
	service.finishAdmissionClaim(proof.ChallengeID, token, true)
	defer service.enforcement.CancelViewAdmission(admissionID)
	if err = ctx.Err(); err != nil {
		return readFailure(err)
	}
	if !reflect.DeepEqual(state.source, authorizedSource) || state.query.Digest != authorizedDigest || state.bounds != authorizedBounds || state.reader == nil {
		return ChallengeResult{}, fail(ErrorConflict, "admission_unavailable")
	}
	ctx, cancel := context.WithDeadline(ctx, state.expires)
	defer cancel()
	if err = service.resolver.PreflightSource(ctx, principal, state.source); err != nil {
		return ChallengeResult{}, fail(ErrorConflict, "source_changed")
	}
	if err = ctx.Err(); err != nil {
		return readFailure(err)
	}
	parsed, err := ParseQuery(state.viewID, state.query.Canonical)
	if err != nil {
		return ChallengeResult{}, fail(ErrorInvalid, "validation")
	}
	request := ReadRequest{Source: CloneSource(state.source), Query: parsed.Query, Bounds: state.bounds}
	result, err := state.reader.Read(ctx, request)
	if err != nil {
		return readFailure(err)
	}
	if err = ctx.Err(); err != nil {
		return readFailure(err)
	}
	if ValidateResultRequest(result, request) != nil {
		return ChallengeResult{}, fail(ErrorUpstream, "invalid_provider_response")
	}
	raw, count, err := EncodeBounded(result, state.bounds)
	if err != nil {
		return ChallengeResult{}, fail(ErrorUpstream, "invalid_provider_response")
	}
	if err = service.resolver.PreflightSource(ctx, principal, state.source); err != nil {
		return ChallengeResult{}, fail(ErrorDenied, "source_changed")
	}
	if err = ctx.Err(); err != nil {
		return readFailure(err)
	}
	releaseID, release, releaseExpires, err := service.enforcement.StageViewResult(admissionID, principal, raw, count)
	if err != nil {
		return ChallengeResult{}, fail(ErrorDenied, "release_denied")
	}
	cancelRelease := true
	defer func() {
		if cancelRelease {
			service.enforcement.CancelViewRelease(releaseID)
		}
	}()
	metadata, err := service.trust.ProducerMetadata()
	if err != nil {
		return ChallengeResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	signature, err := service.trust.SignProducerChallenge(release)
	if err != nil {
		return ChallengeResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	if err = ctx.Err(); err != nil {
		return ChallengeResult{}, fail(ErrorUnavailable, "cancelled")
	}
	service.sweep(service.clock.Now())
	releaseState := releaseState{admissionState: admissionState{
		viewID: state.viewID, query: state.query, principal: state.principal, source: state.source,
		bounds: state.bounds, expires: releaseExpires,
	}}
	service.mu.Lock()
	closed := service.closed
	if closed || !releaseExpires.After(service.clock.Now()) {
		service.mu.Unlock()
		if closed {
			return ChallengeResult{}, fail(ErrorUnavailable, "cancelled")
		}
		return ChallengeResult{}, fail(ErrorDenied, "release_denied")
	}
	service.releases[releaseID] = releaseState
	service.mu.Unlock()
	cancelRelease = false
	return ChallengeResult{1, "release", releaseID, base64.RawURLEncoding.EncodeToString(release), base64.RawURLEncoding.EncodeToString(signature), metadata, releaseExpires}, nil
}

func (service *Service) Release(ctx context.Context, principal trust.Principal, id string, proof trust.Proof) (ReleaseResult, error) {
	var done func()
	var ok bool
	ctx, done, ok = service.beginOperation(ctx)
	if !ok {
		return ReleaseResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	defer done()
	if err := ctx.Err(); err != nil {
		return ReleaseResult{}, fail(ErrorUnavailable, "cancelled")
	}
	service.sweep(service.clock.Now())
	service.mu.Lock()
	if service.closed {
		service.mu.Unlock()
		return ReleaseResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	state, ok := service.releases[proof.ChallengeID]
	if !ok || state.viewID != ID(id) || !state.principal.Same(principal) {
		service.mu.Unlock()
		return ReleaseResult{}, fail(ErrorDenied, "release_denied")
	}
	if state.claiming {
		service.mu.Unlock()
		return ReleaseResult{}, fail(ErrorConflict, "release_in_progress")
	}
	token := &claimToken{}
	state.claiming, state.claimToken = true, token
	service.releases[proof.ChallengeID] = releaseState{state.admissionState}
	service.mu.Unlock()
	if !state.expires.After(service.clock.Now()) {
		service.finishReleaseClaim(proof.ChallengeID, token, true)
		service.enforcement.CancelViewRelease(proof.ChallengeID)
		return ReleaseResult{}, fail(ErrorDenied, "release_denied")
	}
	ctx, cancel := context.WithDeadline(ctx, state.expires)
	defer cancel()
	if err := service.resolver.PreflightSource(ctx, principal, state.source); err != nil {
		service.finishReleaseClaim(proof.ChallengeID, token, true)
		service.enforcement.CancelViewRelease(proof.ChallengeID)
		return ReleaseResult{}, fail(ErrorDenied, "release_denied")
	}
	raw, err := service.enforcement.ClaimViewRelease(ctx, principal, ID(id), proof)
	if err != nil {
		service.finishReleaseClaim(proof.ChallengeID, token, false)
		return ReleaseResult{}, fail(ErrorDenied, "release_denied")
	}
	service.finishReleaseClaim(proof.ChallengeID, token, true)
	parsed, err := DecodeBounded(raw, ID(id), state.bounds)
	if err != nil || parsed.ViewID != ID(id) {
		return ReleaseResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	request := ReadRequest{Source: CloneSource(state.source), Query: state.query.Query, Bounds: state.bounds}
	if ValidateResultRequest(parsed, request) != nil {
		return ReleaseResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	return ReleaseResult{SchemaVersion: 1, View: parsed}, nil
}

func (service *Service) sweep(now time.Time) {
	var admissions, releases []string
	service.mu.Lock()
	for id, state := range service.admissions {
		if !state.claiming && !state.expires.After(now) {
			delete(service.admissions, id)
			admissions = append(admissions, id)
		}
	}
	for id, state := range service.releases {
		if !state.claiming && !state.expires.After(now) {
			delete(service.releases, id)
			releases = append(releases, id)
		}
	}
	service.mu.Unlock()
	for _, id := range admissions {
		service.enforcement.CancelViewAdmission(id)
	}
	for _, id := range releases {
		service.enforcement.CancelViewRelease(id)
	}
}

func (service *Service) finishAdmissionClaim(id string, token *claimToken, consume bool) {
	service.mu.Lock()
	closed := service.closed
	matched := false
	if state, ok := service.admissions[id]; ok && state.claiming && state.claimToken == token {
		matched = true
		if consume {
			delete(service.admissions, id)
		} else {
			state.claiming, state.claimToken = false, nil
			service.admissions[id] = state
		}
	}
	service.mu.Unlock()
	if matched && !closed && !consume {
		service.sweep(service.clock.Now())
	}
}

func (service *Service) finishReleaseClaim(id string, token *claimToken, consume bool) {
	service.mu.Lock()
	closed := service.closed
	matched := false
	if state, ok := service.releases[id]; ok && state.claiming && state.claimToken == token {
		matched = true
		if consume {
			delete(service.releases, id)
		} else {
			state.claiming, state.claimToken = false, nil
			service.releases[id] = state
		}
	}
	service.mu.Unlock()
	if matched && !closed && !consume {
		service.sweep(service.clock.Now())
	}
}

func readFailure(err error) (ChallengeResult, error) {
	if errors.Is(err, context.Canceled) {
		return ChallengeResult{}, fail(ErrorUnavailable, "cancelled")
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return ChallengeResult{}, fail(ErrorUnavailable, "deadline_exceeded")
	}
	var failure ReadError
	if errors.As(err, &failure) {
		switch failure.Kind {
		case InvalidQuery:
			return ChallengeResult{}, fail(ErrorInvalid, string(failure.Kind))
		case CredentialExpired, PermissionDenied:
			return ChallengeResult{}, fail(ErrorDenied, string(failure.Kind))
		case RateLimited:
			return ChallengeResult{}, fail(ErrorLimited, string(failure.Kind))
		case InvalidProviderResponse:
			return ChallengeResult{}, fail(ErrorUpstream, string(failure.Kind))
		}
	}
	return ChallengeResult{}, fail(ErrorUnavailable, "view_unavailable")
}
