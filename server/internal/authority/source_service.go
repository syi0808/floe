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
	"reflect"
	"sync"
	"time"
)

// SourceService owns the bounded assistant-view workflow. Resolver selects one
// immutable source and Reader; authority never switches on a provider or view.
type SourceService struct {
	engine     *Engine
	trust      ProducerTrust
	resolver   SourceResolver
	fence      SourceFence
	mu         sync.Mutex
	admissions map[string]viewAdmissionState
}
type ProducerTrust interface {
	ProducerMetadata() (trust.ProducerMetadata, error)
	SignProducerChallenge([]byte) ([]byte, error)
}
type viewAdmissionState struct {
	viewID    views.ID
	query     views.ParsedQuery
	principal trust.Principal
	source    views.SourceSnapshot
	reader    views.Reader
	bounds    views.Bounds
	expires   time.Time
}

func NewSourceService(engine *Engine, t ProducerTrust, resolver SourceResolver, fence SourceFence) (*SourceService, error) {
	if engine == nil || t == nil || resolver == nil || fence == nil {
		return nil, ErrUnavailable
	}
	return &SourceService{engine: engine, trust: t, resolver: resolver, fence: fence, admissions: map[string]viewAdmissionState{}}, nil
}

const maxViewSourcePreviewProofBytes = 16 << 10

type viewGrantWire struct {
	ID          string `json:"id"`
	Incarnation string `json:"incarnation"`
	Epoch       uint64 `json:"epoch"`
}
type ViewAdmission struct {
	SchemaVersion      int             `json:"schema_version"`
	ConnectorID        string          `json:"connector_id"`
	ConnectionID       string          `json:"connection_id"`
	ConnectionRevision uint64          `json:"connection_revision"`
	Resources          []string        `json:"resources"`
	Grant              viewGrantWire   `json:"grant"`
	Purpose            string          `json:"purpose"`
	Consumer           string          `json:"consumer"`
	MaxItems           uint32          `json:"max_items"`
	MaxBytes           uint32          `json:"max_bytes"`
	Query              json.RawMessage `json:"query"`
}
type SourcePreview struct {
	ConnectorID  string `json:"connector_id"`
	ConnectionID string `json:"connection_id"`
	Resource     string `json:"resource"`
}
type sourcePreviewReply struct {
	trust.ProducerMetadata
	Descriptor         string   `json:"descriptor_b64url"`
	Signature          string   `json:"producer_signature"`
	Expires            int64    `json:"expires_at_unix_ms"`
	ConnectionRevision uint64   `json:"connection_revision"`
	Resources          []string `json:"source_resources"`
}
type sourceChallengeReply struct {
	SchemaVersion int                    `json:"schema_version"`
	Operation     string                 `json:"operation"`
	ChallengeID   string                 `json:"challenge_id"`
	Challenge     string                 `json:"challenge_b64url"`
	Signature     string                 `json:"producer_signature"`
	Producer      trust.ProducerMetadata `json:"producer"`
	Expires       time.Time              `json:"expires"`
}

func (s *SourceService) PreviewView(ctx context.Context, p trust.Principal, id string, in SourcePreview) operation.Result {
	ctx, cancel := context.WithTimeout(ctx, ChallengeTTL)
	defer cancel()
	if !trust.ValidID(in.ConnectionID) || in.ConnectorID == "" || in.Resource != id+":"+in.ConnectionID || len(in.Resource) > MaxResourceBytes {
		return operation.Reject(operation.Invalid, "validation")
	}
	resolved, err := s.resolver.ResolveSource(ctx, p, views.SourceTarget{ViewID: views.ID(id), ConnectorID: in.ConnectorID, ConnectionID: in.ConnectionID, ResourceID: in.Resource})
	if err != nil {
		return operation.Reject(operation.Conflict, "source_unavailable")
	}
	source := resolved.Snapshot
	if err = s.resolver.PreflightSource(ctx, p, source); err != nil {
		return operation.Reject(operation.Unavailable, "source_identity_unavailable")
	}
	metadata, err := s.trust.ProducerMetadata()
	if err != nil {
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	now := time.Now()
	descriptor := map[string]any{
		"v": 1, "operation": "remote_view_source_preview", "challenge_id": trust.NewID(), "nonce": trust.Token(),
		"view_id": id, "person_id": p.PersonID(), "client_id": p.ClientID(), "device_id": p.DeviceID(), "audience": metadata.Audience,
		"connector_id": source.ConnectorID, "connection_id": source.ConnectionID, "connection_revision": source.ConnectionRevision,
		"execution_owner": source.ExecutionOwner, "incarnation": source.Incarnation, "epoch": source.Epoch, "resource": in.Resource,
		"source_resources": source.Resources, "provider_identity": source.ProviderIdentity, "issued_at_unix_ms": now.UnixMilli(),
	}
	raw, err := json.Marshal(descriptor)
	if err != nil || len(raw) > maxViewSourcePreviewProofBytes {
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	signature, err := s.trust.SignProducerChallenge(raw)
	if err != nil {
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	if err = s.fence.WithCurrentSource(p, source, func(views.SourceSnapshot) error { return nil }); err != nil {
		return operation.Reject(operation.Conflict, "source_changed")
	}
	return operation.Accept(sourcePreviewReply{metadata, base64.RawURLEncoding.EncodeToString(raw), base64.RawURLEncoding.EncodeToString(signature), now.Add(ChallengeTTL).UnixMilli(), source.ConnectionRevision, append([]string(nil), source.Resources...)})
}
func (s *SourceService) AdmitView(ctx context.Context, p trust.Principal, id string, in ViewAdmission) operation.Result {
	ctx, cancel := context.WithTimeout(ctx, ChallengeTTL)
	defer cancel()
	if in.SchemaVersion != SchemaVersion || !trust.ValidID(in.ConnectionID) || in.ConnectionRevision == 0 || len(in.Resources) != 1 || in.Resources[0] != id+":"+in.ConnectionID || len(in.Resources[0]) > MaxResourceBytes || in.MaxItems == 0 || in.MaxItems > 128 || in.MaxBytes == 0 || in.MaxBytes > MaxStageBytesPerResult {
		return operation.Reject(operation.Invalid, "validation")
	}
	query, err := views.ParseQuery(views.ID(id), in.Query)
	if err != nil {
		return operation.Reject(operation.Invalid, "validation")
	}
	// Sign the exact validated Rust query bytes, not an independently re-encoded object.
	query.Canonical = append([]byte(nil), in.Query...)
	query.Digest = sha256.Sum256(query.Canonical)
	resolved, err := s.resolver.ResolveSource(ctx, p, views.SourceTarget{ViewID: views.ID(id), ConnectorID: in.ConnectorID, ConnectionID: in.ConnectionID, ConnectionRevision: in.ConnectionRevision, ResourceID: in.Resources[0]})
	if err != nil {
		return operation.Reject(operation.Conflict, "connection_changed")
	}
	bounds := views.Bounds{MaxItems: min(in.MaxItems, resolved.Limits.MaxItems), MaxBytes: min(in.MaxBytes, resolved.Limits.MaxBytes)}
	if bounds.MaxItems != in.MaxItems || bounds.MaxBytes != in.MaxBytes || views.ValidateQueryBounds(query.Query, bounds) != nil {
		return operation.Reject(operation.Invalid, "query_budget")
	}
	if err = s.resolver.PreflightSource(ctx, p, resolved.Snapshot); err != nil {
		return operation.Reject(operation.Unavailable, "source_identity_unavailable")
	}
	metadata, err := s.trust.ProducerMetadata()
	if err != nil {
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	challenge, err := s.engine.IssueAdmission(p, Request{Source: resolved.Snapshot, policy: assistantPolicy{Audience: metadata.Audience, Purpose: in.Purpose, Consumer: in.Consumer, Grant: GrantReference{in.Grant.ID, in.Grant.Incarnation, in.Grant.Epoch}, Resources: append([]string(nil), in.Resources...), QueryDigest: query.Digest, MaxItems: bounds.MaxItems, MaxBytes: bounds.MaxBytes}}, s.fence)
	if err != nil {
		return operation.Reject(operation.Denied, "admission_denied")
	}
	s.mu.Lock()
	now := time.Now()
	count := 0
	for key, old := range s.admissions {
		if !old.expires.After(now) {
			delete(s.admissions, key)
		} else if old.principal.ClientID() == p.ClientID() {
			count++
		}
	}
	if count >= MaxPendingPerClient {
		s.mu.Unlock()
		s.engine.CancelAdmission(challenge.ID)
		return operation.Reject(operation.Limited, "admission_capacity")
	}
	s.admissions[challenge.ID] = viewAdmissionState{views.ID(id), query, p, views.CloneSource(resolved.Snapshot), resolved.Reader, bounds, challenge.ExpiresAt}
	s.mu.Unlock()
	signature, err := s.trust.SignProducerChallenge(challenge.Bytes)
	if err != nil {
		s.dropAdmission(challenge.ID)
		s.engine.CancelAdmission(challenge.ID)
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	return operation.Accept(sourceChallengeReply{SchemaVersion, "admission", challenge.ID, challenge.BytesB64, base64.RawURLEncoding.EncodeToString(signature), metadata, challenge.ExpiresAt})
}
func (s *SourceService) dropAdmission(id string) {
	s.mu.Lock()
	delete(s.admissions, id)
	s.mu.Unlock()
}
func (s *SourceService) ReadView(ctx context.Context, p trust.Principal, id string, proof trust.Proof) operation.Result {
	admissionID, authorized, err := s.engine.ClaimAdmission(p, views.ID(id), proof, s.fence)
	if err != nil {
		return operation.Reject(operation.Denied, "admission_denied")
	}
	defer s.engine.CancelAdmission(admissionID)
	s.mu.Lock()
	state, ok := s.admissions[admissionID]
	delete(s.admissions, admissionID)
	s.mu.Unlock()
	if !ok || state.viewID != views.ID(id) || !state.principal.Same(p) || !state.expires.After(time.Now()) || !assistantDigestMatches(authorized, state.query.Digest) || !reflect.DeepEqual(state.source, authorized.Source) || state.reader == nil {
		return operation.Reject(operation.Conflict, "admission_unavailable")
	}
	ctx, cancel := context.WithDeadline(ctx, state.expires)
	defer cancel()
	if err = s.resolver.PreflightSource(ctx, p, state.source); err != nil {
		return operation.Reject(operation.Conflict, "source_changed")
	}
	if err = s.fence.WithCurrentSource(p, state.source, func(views.SourceSnapshot) error { return ctx.Err() }); err != nil {
		return operation.Reject(operation.Denied, "source_changed")
	}
	parsed, err := views.ParseQuery(state.viewID, state.query.Canonical)
	if err != nil {
		return operation.Reject(operation.Invalid, "validation")
	}
	request := views.ReadRequest{Source: views.CloneSource(state.source), Query: parsed.Query, Bounds: state.bounds}
	result, err := state.reader.Read(ctx, request)
	if err != nil {
		return readFailure(err)
	}
	if err = ctx.Err(); err != nil {
		return readFailure(err)
	}
	if views.ValidateResultRequest(result, request) != nil {
		return operation.Reject(operation.Upstream, "invalid_provider_response")
	}
	raw, count, err := views.EncodeBounded(result, state.bounds)
	if err != nil {
		return operation.Reject(operation.Upstream, "invalid_provider_response")
	}
	if err = s.resolver.PreflightSource(ctx, p, state.source); err != nil {
		return operation.Reject(operation.Denied, "source_changed")
	}
	release, err := s.engine.StageResult(admissionID, p, authorized, raw, count, s.fence)
	if err != nil {
		return operation.Reject(operation.Denied, "release_denied")
	}
	metadata, err := s.trust.ProducerMetadata()
	if err != nil {
		s.engine.CancelRelease(release.ID)
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	signature, err := s.trust.SignProducerChallenge(release.Bytes)
	if err != nil {
		s.engine.CancelRelease(release.ID)
		return operation.Reject(operation.Unavailable, "producer_unavailable")
	}
	return operation.Accept(sourceChallengeReply{SchemaVersion, "release", release.ID, release.BytesB64, base64.RawURLEncoding.EncodeToString(signature), metadata, release.ExpiresAt})
}
func (s *SourceService) Release(ctx context.Context, p trust.Principal, id string, proof trust.Proof) operation.Result {
	ctx, cancel := context.WithTimeout(ctx, ChallengeTTL)
	defer cancel()
	raw, err := s.engine.ClaimRelease(ctx, p, views.ID(id), proof, s.resolver, s.fence)
	if err != nil {
		return operation.Reject(operation.Denied, "release_denied")
	}
	if len(raw) == 0 || !json.Valid(raw) {
		return operation.Reject(operation.Unavailable, "view_unavailable")
	}
	return operation.Accept(struct {
		SchemaVersion int             `json:"schema_version"`
		View          json.RawMessage `json:"view"`
	}{SchemaVersion, raw})
}
func readFailure(err error) operation.Result {
	if errors.Is(err, context.Canceled) {
		return operation.Reject(operation.Unavailable, "cancelled")
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return operation.Reject(operation.Unavailable, "deadline_exceeded")
	}
	var failure views.ReadError
	if errors.As(err, &failure) {
		switch failure.Kind {
		case views.InvalidQuery:
			return operation.Reject(operation.Invalid, string(failure.Kind))
		case views.CredentialExpired, views.PermissionDenied:
			return operation.Reject(operation.Denied, string(failure.Kind))
		case views.RateLimited:
			return operation.Reject(operation.Limited, string(failure.Kind))
		case views.InvalidProviderResponse:
			return operation.Reject(operation.Upstream, string(failure.Kind))
		}
	}
	return operation.Reject(operation.Unavailable, "view_unavailable")
}

func assistantDigestMatches(r Request, digest [32]byte) bool {
	policy, ok := r.policy.(assistantPolicy)
	return ok && policy.QueryDigest == digest
}
