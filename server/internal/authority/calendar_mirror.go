package authority

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"floe/server/internal/operation"
	"floe/server/internal/trust"
	"floe/server/internal/views"
	"sync"
	"time"
)

const maxMirrorReads = 16
const maxMirrorPages = 512

type CalendarMirrorService struct {
	engine   *Engine
	trust    ProducerTrust
	resolver SourceResolver
	fence    SourceFence
	mu       sync.Mutex
	reads    map[string]*mirrorRead
	pending  map[string]mirrorPending
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
	request        Request
	reader         views.Reader
	query          views.CalendarMirrorQuery
	providerCursor string
	readKey        string
	expires        time.Time
}

func NewCalendarMirrorService(engine *Engine, t ProducerTrust, resolver SourceResolver, fence SourceFence) (*CalendarMirrorService, error) {
	if engine == nil || t == nil || resolver == nil || fence == nil {
		return nil, ErrUnavailable
	}
	return &CalendarMirrorService{engine: engine, trust: t, resolver: resolver, fence: fence, reads: map[string]*mirrorRead{}, pending: map[string]mirrorPending{}}, nil
}
func (s *CalendarMirrorService) Preview(ctx context.Context, p trust.Principal, in ProductCalendarPreviewRequest) operation.Result {
	ctx, cancel := context.WithTimeout(ctx, ChallengeTTL)
	defer cancel()
	if in.SchemaVersion != 1 || !trust.ValidID(in.ConnectionID) || in.LocalRevision == 0 {
		return operation.Reject(operation.Invalid, "validation")
	}
	resolved, err := s.resolver.ResolveSource(ctx, p, views.SourceTarget{ViewID: views.CalendarMirror, ConnectorID: in.ConnectorID, ConnectionID: in.ConnectionID, ResourceID: string(views.CalendarMirror) + ":" + in.ConnectionID})
	if err != nil {
		return operation.Reject(operation.Conflict, "source_unavailable")
	}
	if len(resolved.Snapshot.Resources) == 0 || len(resolved.Snapshot.Resources) > 256 {
		return operation.Reject(operation.Limited, "source_limit")
	}
	if err = s.resolver.PreflightSource(ctx, p, resolved.Snapshot); err != nil {
		return operation.Reject(operation.Unavailable, "source_identity_unavailable")
	}
	metadata, err := s.trust.ProducerMetadata()
	if err != nil {
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	nonce := make([]byte, 32)
	if err = s.engine.random(nonce); err != nil {
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	now := s.engine.clock.Now()
	preview := ProductCalendarSourcePreview{1, "day_calendar_source_preview", trust.NewID(), encodeB64(nonce), p.PersonID(), p.ClientID(), p.DeviceID(), metadata.Audience, metadata.InstanceID, metadata.Fingerprint, sourceClaims(resolved.Snapshot, in.LocalRevision), append([]string(nil), resolved.Snapshot.Resources...), now.UnixMilli(), now.Add(ChallengeTTL).UnixMilli()}
	raw, err := json.Marshal(preview)
	if err != nil || len(raw) > MaxChallengeBytes {
		return operation.Reject(operation.Limited, "source_limit")
	}
	signature, err := s.trust.SignProducerChallenge(raw)
	if err != nil {
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	if err = s.fence.WithCurrentSource(p, resolved.Snapshot, func(views.SourceSnapshot) error { return ctx.Err() }); err != nil {
		return operation.Reject(operation.Conflict, "source_changed")
	}
	return operation.Accept(struct {
		SchemaVersion int                    `json:"schema_version"`
		Descriptor    string                 `json:"descriptor_b64url"`
		Signature     string                 `json:"producer_signature"`
		Producer      trust.ProducerMetadata `json:"producer"`
	}{1, encodeB64(raw), encodeB64(signature), metadata})
}
func (s *CalendarMirrorService) Admit(ctx context.Context, p trust.Principal, in ProductCalendarAdmissionRequest) operation.Result {
	ctx, cancel := context.WithTimeout(ctx, ChallengeTTL)
	defer cancel()
	c := in.Claims
	now := s.engine.clock.Now()
	expiry := time.UnixMilli(in.ExpiresAtUnixMS)
	if in.SchemaVersion != 1 || !expiry.After(now) || expiry.After(now.Add(60*time.Second)) || c.PersonID != p.PersonID() || c.ClientID != p.ClientID() || c.DeviceID != p.DeviceID() {
		return operation.Reject(operation.Invalid, "validation")
	}
	query, err := views.ParseMirrorQuery(c.Query)
	if err != nil {
		return operation.Reject(operation.Invalid, "validation")
	}
	metadata, err := s.trust.ProducerMetadata()
	if err != nil {
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	issuer, err := s.engine.trust.ActiveIssuer(p)
	if err != nil || c.EnrollmentID != issuer.EnrollmentID || c.Audience != metadata.Audience || c.ProducerInstance != metadata.InstanceID || c.ProducerKeyFingerprint != metadata.Fingerprint {
		return operation.Reject(operation.Denied, "binding_changed")
	}
	resolved, err := s.resolver.ResolveSource(ctx, p, views.SourceTarget{ViewID: views.CalendarMirror, ConnectorID: c.Source.ConnectorID, ConnectionID: c.Source.ConnectionID, ConnectionRevision: c.Source.ProviderRevision, ResourceID: string(views.CalendarMirror) + ":" + c.Source.ConnectionID})
	if err != nil {
		return operation.Reject(operation.Conflict, "source_changed")
	}
	policy := productCalendarPolicy{claims: c, expires: expiry}
	request := Request{Source: resolved.Snapshot, policy: policy}
	if validateRequest(request) != nil || c.Limits.MaxPageRecords > resolved.Limits.MaxItems || c.Limits.MaxPageBytes > resolved.Limits.MaxBytes {
		return operation.Reject(operation.Invalid, "validation")
	}
	if err = s.resolver.PreflightSource(ctx, p, resolved.Snapshot); err != nil {
		return operation.Reject(operation.Conflict, "source_changed")
	}
	key := p.ClientID() + "/" + c.ReadOperationID
	s.mu.Lock()
	s.sweepLocked(now)
	read := s.reads[key]
	if read == nil {
		if len(s.reads) >= maxMirrorReads || query.Cursor != "" {
			s.mu.Unlock()
			return operation.Reject(operation.Limited, "read_capacity")
		}
		read = &mirrorRead{principal: p, binding: productClaimsBinding(c), rangeStart: query.RangeStartUnixMS, rangeEnd: query.RangeEndUnixMS, expires: expiry, pages: map[string]bool{}, resources: map[string]*mirrorResource{}}
		for _, resource := range c.Resources {
			read.resources[resource] = &mirrorResource{seen: map[string][32]byte{}, seenCursors: map[string]bool{}}
		}
		s.reads[key] = read
	}
	resource := read.resources[query.CalendarID]
	if read.failed || !read.principal.Same(p) || read.binding != productClaimsBinding(c) || read.rangeStart != query.RangeStartUnixMS || read.rangeEnd != query.RangeEndUnixMS || !read.expires.Equal(expiry) || read.pages[c.PageID] || len(read.pages) >= maxMirrorPages || resource == nil || resource.pending || resource.terminal || !resource.started && query.Cursor != "" || resource.started && (resource.next == "" || resource.next != query.Cursor) {
		s.mu.Unlock()
		return operation.Reject(operation.Conflict, "read_changed")
	}
	providerCursor := resource.providerNext
	resource.started = true
	resource.pending = true
	resource.next = ""
	resource.providerNext = ""
	read.pages[c.PageID] = true
	s.mu.Unlock()
	challenge, err := s.engine.IssueAdmission(p, request, s.fence)
	if err != nil {
		s.failRead(key)
		return operation.Reject(operation.Denied, "admission_denied")
	}
	s.mu.Lock()
	s.pending[challenge.ID] = mirrorPending{cloneRequest(request), resolved.Reader, query, providerCursor, key, challenge.ExpiresAt}
	s.mu.Unlock()
	response := s.challenge(challenge.Bytes)
	if response.Code != "" {
		s.engine.CancelAdmission(challenge.ID)
		s.mu.Lock()
		delete(s.pending, challenge.ID)
		s.mu.Unlock()
		s.failRead(key)
	}
	return response
}
func (s *CalendarMirrorService) Read(ctx context.Context, p trust.Principal, proof trust.Proof) operation.Result {
	admission, request, err := s.engine.ClaimAdmission(p, views.CalendarMirror, proof, s.fence)
	if err != nil {
		return operation.Reject(operation.Denied, "admission_denied")
	}
	defer s.engine.CancelAdmission(admission)
	s.mu.Lock()
	pending, ok := s.pending[admission]
	delete(s.pending, admission)
	s.mu.Unlock()
	if !ok || !requestsEqual(pending.request, request) || !pending.expires.After(s.engine.clock.Now()) {
		return operation.Reject(operation.Conflict, "admission_unavailable")
	}
	success := false
	defer func() {
		if !success {
			s.failRead(pending.readKey)
		}
	}()
	policy, ok := request.policy.(productCalendarPolicy)
	if !ok {
		return operation.Reject(operation.Denied, "admission_denied")
	}
	ctx, cancel := context.WithDeadline(ctx, pending.expires)
	defer cancel()
	if err = s.resolver.PreflightSource(ctx, p, request.Source); err != nil {
		return operation.Reject(operation.Conflict, "source_changed")
	}
	if err = s.fence.WithCurrentSource(p, request.Source, func(views.SourceSnapshot) error { return ctx.Err() }); err != nil {
		return operation.Reject(operation.Denied, "source_changed")
	}
	q := pending.query
	readRequest := views.ReadRequest{Source: views.CloneSource(request.Source), Query: views.Query{ViewID: views.CalendarMirror, Mirror: &q}, Bounds: request.bounds(), ProviderCursor: pending.providerCursor}
	result, readErr := pending.reader.Read(ctx, readRequest)
	if err = ctx.Err(); err != nil {
		return readFailure(err)
	}
	now := s.engine.clock.Now()
	page := CalendarProductPage{SchemaVersion: 1, ResultKind: "calendar.mirror", RefreshOperationID: policy.claims.RefreshOperationID, ReadOperationID: policy.claims.ReadOperationID, PageID: policy.claims.PageID, PersonID: p.PersonID(), DeviceID: p.DeviceID(), Source: policy.claims.Source, CalendarID: q.CalendarID, RangeStartUnixMS: q.RangeStartUnixMS, RangeEndUnixMS: q.RangeEndUnixMS, ObservedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: policy.expires.UnixMilli()}
	providerNext := ""
	if readErr != nil {
		page.Outcome = calendarPageOutcome{state: "failed", reason: mirrorFailure(readErr)}
	} else {
		if result.ViewID != views.CalendarMirror || result.Mirror == nil || result.Calendar != nil || result.Communication != nil || result.Work != nil || result.Logistics != nil || views.ValidateMirrorResult(*result.Mirror, readRequest) != nil || result.Mirror.ObservedAtUnixMS > now.Add(5*time.Second).UnixMilli() {
			return operation.Reject(operation.Upstream, "invalid_provider_response")
		}
		payload := result.Mirror
		page.ObservedAtUnixMS = payload.ObservedAtUnixMS
		page.ExpiresAtUnixMS = min(payload.ExpiresAtUnixMS, policy.expires.UnixMilli())
		if page.ExpiresAtUnixMS <= now.UnixMilli() {
			return operation.Reject(operation.Upstream, "expired_provider_response")
		}
		page.Outcome = calendarPageOutcome{state: "complete", records: payload.Records}
		providerNext = payload.NextCursor
		if providerNext != "" {
			page.Outcome.state = "more"
			page.Outcome.cursor = "mirror_" + trust.Token()
		}
	}
	if err = s.resolver.PreflightSource(ctx, p, request.Source); err != nil {
		return operation.Reject(operation.Conflict, "source_changed")
	}
	raw, err := json.Marshal(page)
	if err != nil || len(raw) > int(policy.claims.Limits.MaxPageBytes) {
		return operation.Reject(operation.Limited, "page_budget")
	}
	count := uint32(len(page.Outcome.records))
	s.mu.Lock()
	read := s.reads[pending.readKey]
	if read == nil || read.failed || !read.expires.After(s.engine.clock.Now()) {
		s.mu.Unlock()
		return operation.Reject(operation.Conflict, "read_changed")
	}
	resource := read.resources[q.CalendarID]
	if resource == nil || !resource.pending || read.records+count > policy.claims.Limits.MaxRecords || read.bytes+uint32(len(raw)) > policy.claims.Limits.MaxBytes {
		s.mu.Unlock()
		return operation.Reject(operation.Limited, "read_budget")
	}
	if providerNext != "" && resource.seenCursors[providerNext] {
		s.mu.Unlock()
		return operation.Reject(operation.Upstream, "repeated_provider_cursor")
	}
	if providerNext != "" {
		resource.seenCursors[providerNext] = true
	}
	for _, record := range page.Outcome.records {
		normalized, _ := json.Marshal(record)
		digest := sha256.Sum256(normalized)
		if _, exists := resource.seen[record.ExternalID]; exists {
			s.mu.Unlock()
			return operation.Reject(operation.Upstream, "duplicate_provider_record")
		}
		resource.seen[record.ExternalID] = digest
	}
	read.records += count
	read.bytes += uint32(len(raw))
	resource.pending = false
	resource.terminal = page.Outcome.state != "more"
	resource.next = page.Outcome.cursor
	resource.providerNext = providerNext
	s.mu.Unlock()
	release, err := s.engine.StageResult(admission, p, request, raw, count, s.fence)
	if err != nil {
		return operation.Reject(operation.Denied, "release_denied")
	}
	response := s.challenge(release.Bytes)
	if response.Code != "" {
		s.engine.CancelRelease(release.ID)
		return response
	}
	success = true
	return response
}
func (s *CalendarMirrorService) Release(ctx context.Context, p trust.Principal, proof trust.Proof) operation.Result {
	ctx, cancel := context.WithTimeout(ctx, ChallengeTTL)
	defer cancel()
	raw, err := s.engine.ClaimRelease(ctx, p, views.CalendarMirror, proof, s.resolver, s.fence)
	if err != nil {
		return operation.Reject(operation.Denied, "release_denied")
	}
	return operation.Accept(struct {
		SchemaVersion int             `json:"schema_version"`
		Page          json.RawMessage `json:"page"`
	}{1, raw})
}
func (s *CalendarMirrorService) challenge(raw []byte) operation.Result {
	metadata, err := s.trust.ProducerMetadata()
	if err != nil {
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	signature, err := s.trust.SignProducerChallenge(raw)
	if err != nil {
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	return operation.Accept(struct {
		SchemaVersion int                    `json:"schema_version"`
		Challenge     string                 `json:"challenge_b64url"`
		Signature     string                 `json:"producer_signature"`
		Producer      trust.ProducerMetadata `json:"producer"`
	}{1, base64.RawURLEncoding.EncodeToString(raw), base64.RawURLEncoding.EncodeToString(signature), metadata})
}
func (s *CalendarMirrorService) failRead(key string) {
	s.mu.Lock()
	if read := s.reads[key]; read != nil {
		read.failed = true
	}
	s.mu.Unlock()
}
func (s *CalendarMirrorService) sweepLocked(now time.Time) {
	for key, read := range s.reads {
		if !read.expires.After(now) {
			delete(s.reads, key)
		}
	}
	for key, pending := range s.pending {
		if !pending.expires.After(now) {
			delete(s.pending, key)
		}
	}
}
func mirrorFailure(err error) string {
	if errors.Is(err, context.Canceled) {
		return "cancelled"
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return "deadline_exceeded"
	}
	var failure views.ReadError
	if errors.As(err, &failure) {
		switch failure.Kind {
		case views.PermissionDenied:
			return "permission_denied"
		case views.InvalidQuery:
			return "calendar_unavailable"
		}
	}
	return "provider_unavailable"
}
