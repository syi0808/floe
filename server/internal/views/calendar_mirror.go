package views

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"reflect"
	"sync"
	"time"

	sourcecontract "floe/server/internal/contracts/source"
	"floe/server/internal/trust"
	viewcontracts "floe/server/internal/views/contracts"
)

const (
	maxMirrorReads      = 16
	maxMirrorPages      = 512
	maxMirrorChallenge  = 64 << 10
	calendarMirrorTTL   = 30 * time.Second
	calendarMirrorSweep = time.Second
)

type CalendarMirrorPreviewResult struct {
	SchemaVersion int
	Descriptor    string
	Signature     string
	Producer      trust.ProducerMetadata
}

type CalendarMirrorChallengeResult struct {
	SchemaVersion int
	Challenge     string
	Signature     string
	Producer      trust.ProducerMetadata
}

type CalendarMirrorReleaseResult struct {
	SchemaVersion int
	Page          json.RawMessage
}

type mirrorRead struct {
	principal            trust.Principal
	binding              [32]byte
	rangeStart, rangeEnd int64
	expires              time.Time
	pages                map[string]bool
	resources            map[string]*mirrorResource
	records, bytes       uint32
	failed               bool
}

type mirrorResource struct {
	started, pending, terminal bool
	next, providerNext         string
	seen                       map[string][32]byte
	seenCursors                map[string]bool
}

type mirrorPending struct {
	principal      trust.Principal
	source         sourcecontract.Snapshot
	claims         viewcontracts.ProductCalendarClaims
	reader         Reader
	query          viewcontracts.CalendarMirrorQuery
	providerCursor string
	readKey        string
	expires        time.Time
	claiming       bool
	claimToken     *claimToken
}

type mirrorRelease struct {
	principal  trust.Principal
	source     sourcecontract.Snapshot
	expires    time.Time
	claiming   bool
	claimToken *claimToken
}

// CalendarMirrorService owns the product Calendar query, reader and page
// workflow. Signed enforcement is delegated through the Views-owned port.
type CalendarMirrorService struct {
	enforcement viewcontracts.CalendarMirrorEnforcement
	trust       ProducerTrust
	resolver    Resolver
	clock       Clock
	mu          sync.Mutex
	reads       map[string]*mirrorRead
	pending     map[string]mirrorPending
	releases    map[string]mirrorRelease
	lifetime    context.Context
	sweepCancel context.CancelFunc
	sweepDone   chan struct{}
	active      sync.WaitGroup
	closed      bool
	closeOnce   sync.Once
}

func NewCalendarMirrorService(enforcement viewcontracts.CalendarMirrorEnforcement, producer ProducerTrust, resolver Resolver) (*CalendarMirrorService, error) {
	return NewCalendarMirrorServiceWithClock(enforcement, producer, resolver, wallClock{})
}

func NewCalendarMirrorServiceWithClock(enforcement viewcontracts.CalendarMirrorEnforcement, producer ProducerTrust, resolver Resolver, clock Clock) (*CalendarMirrorService, error) {
	if enforcement == nil || producer == nil || resolver == nil || clock == nil {
		return nil, errors.New("calendar mirror service unavailable")
	}
	ctx, cancel := context.WithCancel(context.Background())
	service := &CalendarMirrorService{enforcement: enforcement, trust: producer, resolver: resolver, clock: clock, reads: map[string]*mirrorRead{}, pending: map[string]mirrorPending{}, releases: map[string]mirrorRelease{}, lifetime: ctx, sweepCancel: cancel, sweepDone: make(chan struct{})}
	go service.sweepLoop(ctx)
	return service, nil
}

// Close cancels operation contexts and drains admitted calls before clearing
// state. A Reader that ignores its context can delay this drain.
func (service *CalendarMirrorService) Close() {
	service.closeOnce.Do(func() {
		service.mu.Lock()
		service.closed = true
		service.mu.Unlock()
		service.sweepCancel()
		<-service.sweepDone
		service.active.Wait()
		service.mu.Lock()
		var admissions, releases []string
		for id := range service.pending {
			admissions = append(admissions, id)
		}
		for id := range service.releases {
			releases = append(releases, id)
		}
		service.pending = map[string]mirrorPending{}
		service.releases = map[string]mirrorRelease{}
		service.reads = map[string]*mirrorRead{}
		service.mu.Unlock()
		for _, id := range admissions {
			service.enforcement.CancelCalendarMirrorAdmission(id)
		}
		for _, id := range releases {
			service.enforcement.CancelCalendarMirrorRelease(id)
		}
	})
}

func (service *CalendarMirrorService) beginOperation(parent context.Context) (context.Context, func(), bool) {
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

func (service *CalendarMirrorService) sweepLoop(ctx context.Context) {
	defer close(service.sweepDone)
	ticker := time.NewTicker(calendarMirrorSweep)
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

func (service *CalendarMirrorService) Preview(ctx context.Context, principal trust.Principal, input viewcontracts.ProductCalendarPreviewRequest) (CalendarMirrorPreviewResult, error) {
	var done func()
	var ok bool
	ctx, done, ok = service.beginOperation(ctx)
	if !ok {
		return CalendarMirrorPreviewResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	defer done()
	service.sweep(service.clock.Now())
	ctx, cancel := context.WithTimeout(ctx, calendarMirrorTTL)
	defer cancel()
	if input.SchemaVersion != 1 || !trust.ValidID(input.ConnectionID) || input.LocalRevision == 0 || input.ConnectorID == "" {
		return CalendarMirrorPreviewResult{}, fail(ErrorInvalid, "validation")
	}
	resolved, err := service.resolver.ResolveSource(ctx, principal, SourceTarget{ViewID: CalendarMirror, ConnectorID: input.ConnectorID, ConnectionID: input.ConnectionID, ResourceID: string(CalendarMirror) + ":" + input.ConnectionID})
	if err != nil {
		return CalendarMirrorPreviewResult{}, fail(ErrorConflict, "source_unavailable")
	}
	if len(resolved.Snapshot.Resources) == 0 || len(resolved.Snapshot.Resources) > 256 {
		return CalendarMirrorPreviewResult{}, fail(ErrorLimited, "source_limit")
	}
	if err = service.resolver.PreflightSource(ctx, principal, resolved.Snapshot); err != nil {
		return CalendarMirrorPreviewResult{}, fail(ErrorUnavailable, "source_identity_unavailable")
	}
	metadata, err := service.trust.ProducerMetadata()
	if err != nil {
		return CalendarMirrorPreviewResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	nonce := make([]byte, 32)
	if _, err = rand.Read(nonce); err != nil {
		return CalendarMirrorPreviewResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	now := service.clock.Now()
	preview := viewcontracts.ProductCalendarSourcePreview{Version: 1, Operation: "day_calendar_source_preview", ChallengeID: trust.NewID(), Nonce: base64.RawURLEncoding.EncodeToString(nonce), PersonID: principal.PersonID(), ClientID: principal.ClientID(), DeviceID: principal.DeviceID(), Audience: metadata.Audience, ProducerInstance: metadata.InstanceID, ProducerKeyFingerprint: metadata.Fingerprint, Source: viewcontracts.CalendarSourceClaims(resolved.Snapshot, input.LocalRevision), Resources: append([]string(nil), resolved.Snapshot.Resources...), IssuedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: now.Add(calendarMirrorTTL).UnixMilli()}
	raw, err := json.Marshal(preview)
	if err != nil || len(raw) > maxViewPreviewProofBytes {
		return CalendarMirrorPreviewResult{}, fail(ErrorLimited, "source_limit")
	}
	signature, err := service.trust.SignProducerChallenge(raw)
	if err != nil {
		return CalendarMirrorPreviewResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	if err = service.resolver.PreflightSource(ctx, principal, resolved.Snapshot); err != nil {
		return CalendarMirrorPreviewResult{}, fail(ErrorConflict, "source_changed")
	}
	return CalendarMirrorPreviewResult{SchemaVersion: 1, Descriptor: base64.RawURLEncoding.EncodeToString(raw), Signature: base64.RawURLEncoding.EncodeToString(signature), Producer: metadata}, nil
}

func (service *CalendarMirrorService) Admit(ctx context.Context, principal trust.Principal, input viewcontracts.ProductCalendarAdmissionRequest) (CalendarMirrorChallengeResult, error) {
	var done func()
	var ok bool
	ctx, done, ok = service.beginOperation(ctx)
	if !ok {
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	defer done()
	service.sweep(service.clock.Now())
	ctx, cancel := context.WithTimeout(ctx, calendarMirrorTTL)
	defer cancel()
	claims := input.Claims
	now := service.clock.Now()
	expires := time.UnixMilli(input.ExpiresAtUnixMS)
	if input.SchemaVersion != 1 || !expires.After(now) || expires.After(now.Add(time.Minute)) || claims.PersonID != principal.PersonID() || claims.ClientID != principal.ClientID() || claims.DeviceID != principal.DeviceID() {
		return CalendarMirrorChallengeResult{}, fail(ErrorInvalid, "validation")
	}
	query, err := viewcontracts.ParseMirrorQuery(claims.Query)
	digest := sha256.Sum256(claims.Query)
	selected := false
	for _, resource := range claims.Resources {
		selected = selected || resource == query.CalendarID
	}
	if err != nil || query.Limit > claims.Limits.MaxPageRecords || hex.EncodeToString(digest[:]) != claims.QuerySHA256 || !selected {
		return CalendarMirrorChallengeResult{}, fail(ErrorInvalid, "validation")
	}
	resolved, err := service.resolver.ResolveSource(ctx, principal, SourceTarget{ViewID: CalendarMirror, ConnectorID: claims.Source.ConnectorID, ConnectionID: claims.Source.ConnectionID, ConnectionRevision: claims.Source.ProviderRevision, ResourceID: string(CalendarMirror) + ":" + claims.Source.ConnectionID})
	if err != nil {
		return CalendarMirrorChallengeResult{}, fail(ErrorConflict, "source_changed")
	}
	if resolved.Reader == nil {
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	if claims.Limits.MaxPageRecords > resolved.Limits.MaxItems || claims.Limits.MaxPageBytes > resolved.Limits.MaxBytes {
		return CalendarMirrorChallengeResult{}, fail(ErrorInvalid, "validation")
	}
	if err = service.resolver.PreflightSource(ctx, principal, resolved.Snapshot); err != nil {
		return CalendarMirrorChallengeResult{}, fail(ErrorConflict, "source_changed")
	}
	key := principal.ClientID() + "/" + claims.ReadOperationID
	service.mu.Lock()
	if service.closed {
		service.mu.Unlock()
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	read := service.reads[key]
	if read == nil {
		if len(service.reads) >= maxMirrorReads || query.Cursor != "" {
			service.mu.Unlock()
			return CalendarMirrorChallengeResult{}, fail(ErrorLimited, "read_capacity")
		}
		read = &mirrorRead{principal: principal, binding: productClaimsBinding(claims), rangeStart: query.RangeStartUnixMS, rangeEnd: query.RangeEndUnixMS, expires: expires, pages: map[string]bool{}, resources: map[string]*mirrorResource{}}
		for _, resource := range claims.Resources {
			read.resources[resource] = &mirrorResource{seen: map[string][32]byte{}, seenCursors: map[string]bool{}}
		}
		service.reads[key] = read
	}
	resource := read.resources[query.CalendarID]
	if read.failed || !read.principal.Same(principal) || read.binding != productClaimsBinding(claims) || read.rangeStart != query.RangeStartUnixMS || read.rangeEnd != query.RangeEndUnixMS || !read.expires.Equal(expires) || read.pages[claims.PageID] || len(read.pages) >= maxMirrorPages || resource == nil || resource.pending || resource.terminal || !resource.started && query.Cursor != "" || resource.started && (resource.next == "" || resource.next != query.Cursor) {
		service.mu.Unlock()
		return CalendarMirrorChallengeResult{}, fail(ErrorConflict, "read_changed")
	}
	providerCursor := resource.providerNext
	resource.started, resource.pending, resource.next, resource.providerNext = true, true, "", ""
	read.pages[claims.PageID] = true
	service.mu.Unlock()

	admissionID, challenge, challengeExpires, err := service.enforcement.IssueCalendarMirrorAdmission(principal, resolved.Snapshot, claims, expires)
	if err != nil {
		service.failRead(key)
		return CalendarMirrorChallengeResult{}, mirrorEnforcementFailure(err)
	}
	cleanup := true
	defer func() {
		if cleanup {
			service.enforcement.CancelCalendarMirrorAdmission(admissionID)
		}
	}()
	if err = ctx.Err(); err != nil || !challengeExpires.After(service.clock.Now()) {
		service.failRead(key)
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "cancelled")
	}
	metadata, err := service.trust.ProducerMetadata()
	if err != nil {
		service.failRead(key)
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	signature, err := service.trust.SignProducerChallenge(challenge)
	if err != nil {
		service.failRead(key)
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	if err = ctx.Err(); err != nil || !challengeExpires.After(service.clock.Now()) || len(challenge) > maxMirrorChallenge {
		service.failRead(key)
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "cancelled")
	}
	service.mu.Lock()
	current := service.reads[key]
	closed := service.closed
	if closed || current == nil || current.failed || !current.expires.After(service.clock.Now()) {
		service.mu.Unlock()
		service.failRead(key)
		if closed {
			return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "cancelled")
		}
		return CalendarMirrorChallengeResult{}, fail(ErrorConflict, "read_changed")
	}
	service.pending[admissionID] = mirrorPending{principal: principal, source: sourcecontract.Clone(resolved.Snapshot), claims: viewcontracts.CloneProductCalendarClaims(claims), reader: resolved.Reader, query: query, providerCursor: providerCursor, readKey: key, expires: challengeExpires}
	service.mu.Unlock()
	cleanup = false
	return CalendarMirrorChallengeResult{1, base64.RawURLEncoding.EncodeToString(challenge), base64.RawURLEncoding.EncodeToString(signature), metadata}, nil
}

func (service *CalendarMirrorService) Read(ctx context.Context, principal trust.Principal, proof trust.Proof) (CalendarMirrorChallengeResult, error) {
	var done func()
	var ok bool
	ctx, done, ok = service.beginOperation(ctx)
	if !ok {
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	defer done()
	if err := ctx.Err(); err != nil {
		return CalendarMirrorChallengeResult{}, mirrorReadError(err)
	}
	service.sweep(service.clock.Now())
	service.mu.Lock()
	if service.closed {
		service.mu.Unlock()
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	pending, ok := service.pending[proof.ChallengeID]
	if !ok || !pending.principal.Same(principal) || !pending.expires.After(service.clock.Now()) || pending.claiming || pending.reader == nil {
		service.mu.Unlock()
		return CalendarMirrorChallengeResult{}, fail(ErrorDenied, "admission_denied")
	}
	token := &claimToken{}
	pending.claiming, pending.claimToken = true, token
	service.pending[proof.ChallengeID] = pending
	service.mu.Unlock()
	admissionID, authorizedSource, authorizedClaims, policyExpiry, err := service.enforcement.ClaimCalendarMirrorAdmission(principal, proof)
	if err != nil {
		service.finishMirrorAdmissionClaim(proof.ChallengeID, token, false)
		if ctx.Err() != nil {
			return CalendarMirrorChallengeResult{}, mirrorReadError(ctx.Err())
		}
		return CalendarMirrorChallengeResult{}, fail(ErrorDenied, "admission_denied")
	}
	service.finishMirrorAdmissionClaim(proof.ChallengeID, token, true)
	defer service.enforcement.CancelCalendarMirrorAdmission(admissionID)
	if err = ctx.Err(); err != nil {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, mirrorReadError(err)
	}
	if !reflect.DeepEqual(pending.source, authorizedSource) || !reflect.DeepEqual(pending.claims, authorizedClaims) || !policyExpiry.After(service.clock.Now()) {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, fail(ErrorConflict, "admission_unavailable")
	}
	ctx, cancel := context.WithDeadline(ctx, pending.expires)
	defer cancel()
	if err = service.resolver.PreflightSource(ctx, principal, pending.source); err != nil {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, fail(ErrorConflict, "source_changed")
	}
	if err = ctx.Err(); err != nil {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, mirrorReadError(err)
	}
	query, err := viewcontracts.ParseMirrorQuery(pending.claims.Query)
	if err != nil {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, fail(ErrorInvalid, "validation")
	}
	request := ReadRequest{Source: CloneSource(pending.source), Query: Query{ViewID: CalendarMirror, Mirror: &query}, Bounds: Bounds{MaxItems: pending.claims.Limits.MaxPageRecords, MaxBytes: pending.claims.Limits.MaxPageBytes}, ProviderCursor: pending.providerCursor}
	providerResult, readErr := pending.reader.Read(ctx, request)
	if err = ctx.Err(); err != nil {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, mirrorReadError(err)
	}
	now := service.clock.Now()
	page := CalendarProductPage{SchemaVersion: 1, ResultKind: "calendar.mirror", RefreshOperationID: pending.claims.RefreshOperationID, ReadOperationID: pending.claims.ReadOperationID, PageID: pending.claims.PageID, PersonID: principal.PersonID(), DeviceID: principal.DeviceID(), Source: pending.claims.Source, CalendarID: query.CalendarID, RangeStartUnixMS: query.RangeStartUnixMS, RangeEndUnixMS: query.RangeEndUnixMS, ObservedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: policyExpiry.UnixMilli()}
	providerNext := ""
	if readErr != nil {
		page.Outcome = calendarPageOutcome{state: "failed", reason: mirrorFailure(readErr)}
	} else {
		if providerResult.ViewID != CalendarMirror || providerResult.Mirror == nil || providerResult.Calendar != nil || providerResult.Communication != nil || providerResult.Work != nil || providerResult.Logistics != nil || viewcontracts.ValidateMirrorResult(*providerResult.Mirror, request) != nil || providerResult.Mirror.ObservedAtUnixMS > now.Add(5*time.Second).UnixMilli() {
			service.failRead(pending.readKey)
			return CalendarMirrorChallengeResult{}, fail(ErrorUpstream, "invalid_provider_response")
		}
		payload := providerResult.Mirror
		page.ObservedAtUnixMS = payload.ObservedAtUnixMS
		page.ExpiresAtUnixMS = min(payload.ExpiresAtUnixMS, policyExpiry.UnixMilli())
		if page.ExpiresAtUnixMS <= now.UnixMilli() {
			service.failRead(pending.readKey)
			return CalendarMirrorChallengeResult{}, fail(ErrorUpstream, "expired_provider_response")
		}
		page.Outcome = calendarPageOutcome{state: "complete", records: payload.Records}
		providerNext = payload.NextCursor
		if providerNext != "" {
			page.Outcome.state = "more"
			page.Outcome.cursor = "mirror_" + trust.Token()
		}
	}
	if err = service.resolver.PreflightSource(ctx, principal, pending.source); err != nil {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, fail(ErrorConflict, "source_changed")
	}
	if err = ctx.Err(); err != nil {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, mirrorReadError(err)
	}
	raw, err := json.Marshal(page)
	if err != nil || len(raw) > int(pending.claims.Limits.MaxPageBytes) {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, fail(ErrorLimited, "page_budget")
	}
	count := uint32(len(page.Outcome.records))
	service.mu.Lock()
	read := service.reads[pending.readKey]
	closed := service.closed
	if closed || read == nil || read.failed || !read.expires.After(service.clock.Now()) {
		service.mu.Unlock()
		if closed {
			service.failRead(pending.readKey)
			return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "cancelled")
		}
		return CalendarMirrorChallengeResult{}, fail(ErrorConflict, "read_changed")
	}
	resource := read.resources[query.CalendarID]
	if resource == nil || !resource.pending || read.records+count > pending.claims.Limits.MaxRecords || read.bytes+uint32(len(raw)) > pending.claims.Limits.MaxBytes {
		service.mu.Unlock()
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, fail(ErrorLimited, "read_budget")
	}
	if providerNext != "" && resource.seenCursors[providerNext] {
		service.mu.Unlock()
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, fail(ErrorUpstream, "repeated_provider_cursor")
	}
	if providerNext != "" {
		resource.seenCursors[providerNext] = true
	}
	for _, record := range page.Outcome.records {
		encoded, _ := json.Marshal(record)
		digest := sha256.Sum256(encoded)
		if _, exists := resource.seen[record.ExternalID]; exists {
			service.mu.Unlock()
			service.failRead(pending.readKey)
			return CalendarMirrorChallengeResult{}, fail(ErrorUpstream, "duplicate_provider_record")
		}
		resource.seen[record.ExternalID] = digest
	}
	read.records += count
	read.bytes += uint32(len(raw))
	resource.pending = false
	resource.terminal = page.Outcome.state != "more"
	resource.next = page.Outcome.cursor
	resource.providerNext = providerNext
	service.mu.Unlock()

	if err = ctx.Err(); err != nil {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, mirrorReadError(err)
	}
	releaseID, release, releaseExpiry, err := service.enforcement.StageCalendarMirrorResult(admissionID, principal, raw, count)
	if err != nil {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, fail(ErrorDenied, "release_denied")
	}
	cleanupRelease := true
	defer func() {
		if cleanupRelease {
			service.enforcement.CancelCalendarMirrorRelease(releaseID)
		}
	}()
	metadata, err := service.trust.ProducerMetadata()
	if err != nil {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	signature, err := service.trust.SignProducerChallenge(release)
	if err != nil {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "producer_unavailable")
	}
	if err = ctx.Err(); err != nil || !releaseExpiry.After(service.clock.Now()) {
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "cancelled")
	}
	service.mu.Lock()
	closed = service.closed
	if closed {
		service.mu.Unlock()
		service.failRead(pending.readKey)
		return CalendarMirrorChallengeResult{}, fail(ErrorUnavailable, "cancelled")
	}
	service.releases[releaseID] = mirrorRelease{principal: principal, source: sourcecontract.Clone(pending.source), expires: releaseExpiry}
	service.mu.Unlock()
	cleanupRelease = false
	return CalendarMirrorChallengeResult{1, base64.RawURLEncoding.EncodeToString(release), base64.RawURLEncoding.EncodeToString(signature), metadata}, nil
}

func (service *CalendarMirrorService) Release(ctx context.Context, principal trust.Principal, proof trust.Proof) (CalendarMirrorReleaseResult, error) {
	var done func()
	var ok bool
	ctx, done, ok = service.beginOperation(ctx)
	if !ok {
		return CalendarMirrorReleaseResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	defer done()
	if err := ctx.Err(); err != nil {
		return CalendarMirrorReleaseResult{}, fail(ErrorUnavailable, "cancelled")
	}
	service.sweep(service.clock.Now())
	service.mu.Lock()
	if service.closed {
		service.mu.Unlock()
		return CalendarMirrorReleaseResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	release, ok := service.releases[proof.ChallengeID]
	if !ok || !release.principal.Same(principal) || release.claiming {
		service.mu.Unlock()
		return CalendarMirrorReleaseResult{}, fail(ErrorDenied, "release_denied")
	}
	token := &claimToken{}
	release.claiming, release.claimToken = true, token
	service.releases[proof.ChallengeID] = release
	service.mu.Unlock()
	if !release.expires.After(service.clock.Now()) {
		service.finishMirrorReleaseClaim(proof.ChallengeID, token, true)
		service.enforcement.CancelCalendarMirrorRelease(proof.ChallengeID)
		return CalendarMirrorReleaseResult{}, fail(ErrorDenied, "release_denied")
	}
	ctx, cancel := context.WithDeadline(ctx, release.expires)
	defer cancel()
	if err := service.resolver.PreflightSource(ctx, principal, release.source); err != nil {
		service.finishMirrorReleaseClaim(proof.ChallengeID, token, true)
		service.enforcement.CancelCalendarMirrorRelease(proof.ChallengeID)
		return CalendarMirrorReleaseResult{}, fail(ErrorDenied, "release_denied")
	}
	raw, err := service.enforcement.ClaimCalendarMirrorRelease(ctx, principal, proof)
	if err != nil {
		service.finishMirrorReleaseClaim(proof.ChallengeID, token, false)
		return CalendarMirrorReleaseResult{}, fail(ErrorDenied, "release_denied")
	}
	service.finishMirrorReleaseClaim(proof.ChallengeID, token, true)
	if !json.Valid(raw) {
		return CalendarMirrorReleaseResult{}, fail(ErrorUnavailable, "view_unavailable")
	}
	return CalendarMirrorReleaseResult{SchemaVersion: 1, Page: append(json.RawMessage(nil), raw...)}, nil
}

func productClaimsBinding(claims viewcontracts.ProductCalendarClaims) [32]byte {
	claims.PageID = ""
	claims.Query = nil
	claims.QuerySHA256 = ""
	raw, _ := json.Marshal(claims)
	return sha256.Sum256(raw)
}

func (service *CalendarMirrorService) failRead(key string) {
	service.mu.Lock()
	if read := service.reads[key]; read != nil {
		read.failed = true
	}
	service.mu.Unlock()
}

func (service *CalendarMirrorService) sweep(now time.Time) {
	var admissions, releases []string
	service.mu.Lock()
	for key, read := range service.reads {
		if !read.expires.After(now) {
			delete(service.reads, key)
		}
	}
	for id, pending := range service.pending {
		if !pending.claiming && !pending.expires.After(now) {
			delete(service.pending, id)
			admissions = append(admissions, id)
		}
	}
	for id, release := range service.releases {
		if !release.claiming && !release.expires.After(now) {
			delete(service.releases, id)
			releases = append(releases, id)
		}
	}
	service.mu.Unlock()
	for _, id := range admissions {
		service.enforcement.CancelCalendarMirrorAdmission(id)
	}
	for _, id := range releases {
		service.enforcement.CancelCalendarMirrorRelease(id)
	}
}

func (service *CalendarMirrorService) finishMirrorAdmissionClaim(id string, token *claimToken, consume bool) {
	service.mu.Lock()
	closed := service.closed
	matched := false
	if pending, ok := service.pending[id]; ok && pending.claiming && pending.claimToken == token {
		matched = true
		if consume {
			delete(service.pending, id)
		} else {
			pending.claiming, pending.claimToken = false, nil
			service.pending[id] = pending
		}
	}
	service.mu.Unlock()
	if matched && !closed && !consume {
		service.sweep(service.clock.Now())
	}
}

func (service *CalendarMirrorService) finishMirrorReleaseClaim(id string, token *claimToken, consume bool) {
	service.mu.Lock()
	closed := service.closed
	matched := false
	if release, ok := service.releases[id]; ok && release.claiming && release.claimToken == token {
		matched = true
		if consume {
			delete(service.releases, id)
		} else {
			release.claiming, release.claimToken = false, nil
			service.releases[id] = release
		}
	}
	service.mu.Unlock()
	if matched && !closed && !consume {
		service.sweep(service.clock.Now())
	}
}

func mirrorEnforcementFailure(err error) error {
	var failure EnforcementFailure
	if errors.As(err, &failure) {
		return fail(ErrorCategory(failure.Category()), failure.Code())
	}
	return fail(ErrorDenied, "admission_denied")
}

func mirrorReadError(err error) error {
	_, failure := readFailure(err)
	return failure
}

func mirrorFailure(err error) string {
	if errors.Is(err, context.Canceled) {
		return "cancelled"
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return "deadline_exceeded"
	}
	var failure ReadError
	if errors.As(err, &failure) {
		switch failure.Kind {
		case PermissionDenied:
			return "permission_denied"
		case InvalidQuery:
			return "calendar_unavailable"
		}
	}
	return "provider_unavailable"
}
