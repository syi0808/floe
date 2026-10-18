package inference

// These are process-bound transport capabilities, not another Voice/Work or
// presentation journal. Unknown/expired tickets are never treated as new calls.
// Durable logical intent/receipts stay with the client Voice owner; an exact
// transport retry can consume a server-issued capability only once.
import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"floe/server/internal/trust"
	"sync"
	"time"
	"unicode/utf8"
)

const maxLiveControlEntries = 8
const maxLiveClientEntries = 2
const livePreparationLifetime = 30 * time.Second
const liveControlCheckInterval = 250 * time.Millisecond

type LiveTicket struct {
	ID, CapabilityRevision string
	ExpiresAtUnixMS        int64
}

func (LiveTicket) String() string     { return "LiveTicket[redacted]" }
func (t LiveTicket) GoString() string { return t.String() }

type LiveStartCommand struct {
	TicketID, RequestID, CapabilityRevision string
	DurationMS                              uint64
	DataClasses                             []string
	Input                                   LiveStartInput
}

func (LiveStartCommand) String() string     { return "LiveStartCommand[redacted]" }
func (v LiveStartCommand) GoString() string { return v.String() }

type LiveCallStatus struct {
	TicketID, Phase, RequestID string
	ExpiresAtUnixMS            int64
	Attempted                  bool
	Failure                    FailureCode
	Answer                     string
}

func (LiveCallStatus) String() string     { return "LiveCallStatus[redacted]" }
func (v LiveCallStatus) GoString() string { return v.String() }

type liveEntry struct {
	mu                                    sync.Mutex
	id                                    string
	principal                             trust.Principal
	scope                                 trust.PrincipalSnapshot
	selected                              selectedLive
	prepared                              PreparedLiveTransport
	phase                                 string
	deadline                              time.Time
	requestID, fingerprint, answerDigest  string
	attempted                             bool
	issue                                 FailureCode
	controlCtx                            context.Context
	cancelControl                         context.CancelFunc
	transportCtx                          context.Context
	cancelTransport                       context.CancelFunc
	transport                             LiveTransport
	workers                               int
	retired                               bool
	deliveryGate                          chan struct{}
	pendingSequence, acknowledgedSequence uint64
	pendingDigest, acknowledgedDigest     string
	writes                                map[string]*liveWriteEntry
	activeWrite                           string
	closeWrite                            *liveWriteEntry
	releaseSelection                      func()
}
type liveRegistry struct {
	service      *Service
	ctx          context.Context
	cancel       context.CancelFunc
	mu           sync.Mutex
	entries      map[string]*liveEntry
	reserved     map[string]int
	reservations int
	closed       bool
	wg           sync.WaitGroup
}

func newLiveRegistry(s *Service) *liveRegistry {
	ctx, cancel := context.WithCancel(context.Background())
	return &liveRegistry{service: s, ctx: ctx, cancel: cancel, entries: map[string]*liveEntry{}, reserved: map[string]int{}}
}
func liveCause(err error) FailureCode {
	var f Failure
	if errors.As(err, &f) {
		return f.Code
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return ModelTimeout
	}
	return ModelUnavailable
}
func sameLiveScope(a, b trust.PrincipalSnapshot) bool {
	return a.ClientID == b.ClientID && a.PersonID == b.PersonID && a.DeviceID == b.DeviceID
}
func (r *liveRegistry) scope(p trust.Principal) (trust.PrincipalSnapshot, error) {
	if _, err := r.service.trust.ActiveIssuer(p); err != nil {
		return trust.PrincipalSnapshot{}, Failure{Code: Unauthorized}
	}
	var scope trust.PrincipalSnapshot
	err := r.service.trust.WithCurrentPrincipal(p, func(value trust.PrincipalSnapshot) error { scope = value; return nil })
	if err != nil || !scope.Active || scope.CleanupBlocked || scope.Generation == 0 || !trust.ValidID(scope.ClientID) || !trust.ValidID(scope.PersonID) || !trust.ValidDevice(scope.DeviceID) {
		return trust.PrincipalSnapshot{}, Failure{Code: Unauthorized}
	}
	return scope, nil
}
func (r *liveRegistry) lookup(p trust.Principal, id string) (*liveEntry, error) {
	scope, err := r.scope(p)
	if err != nil {
		return nil, err
	}
	if !trust.ValidID(id) {
		return nil, Failure{Code: Validation}
	}
	r.mu.Lock()
	e := r.entries[id]
	closed := r.closed
	r.mu.Unlock()
	if closed {
		return nil, Failure{Code: ModelUnavailable}
	}
	if e == nil {
		return nil, Failure{Code: NotFound}
	}
	if !sameLiveScope(scope, e.scope) {
		return nil, Failure{Code: PermissionDenied}
	}
	e.mu.Lock()
	expired := !time.Now().Before(e.deadline)
	retired := e.retired
	e.mu.Unlock()
	if retired {
		return nil, Failure{Code: NotFound}
	}
	if expired {
		r.retire(e, ModelTimeout)
		return nil, Failure{Code: ModelTimeout}
	}
	return e, nil
}
func (r *liveRegistry) source(ctx context.Context, e *liveEntry, ready bool) error {
	scope, err := r.scope(e.principal)
	if err != nil {
		return err
	}
	if !sameLiveScope(scope, e.scope) || scope.Generation != e.scope.Generation {
		return Failure{Code: Unauthorized}
	}
	current, err := r.service.selectLive(ctx, ready)
	if err != nil {
		return err
	}
	if current.revision != e.selected.revision {
		return Failure{Code: CapabilityChanged}
	}
	return nil
}
func (s *Service) leaseLive(selected selectedLive) (func(), error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	route := s.config.Routes[LivePresentation]
	if s.unavailable || s.generation != selected.generation || !route.Enabled || route.TargetID != selected.targetID {
		return nil, Failure{Code: CapabilityChanged}
	}
	s.leases[selected.generation]++
	var once sync.Once
	return func() {
		once.Do(func() {
			s.mu.Lock()
			defer s.mu.Unlock()
			if s.leases[selected.generation] <= 1 {
				delete(s.leases, selected.generation)
			} else {
				s.leases[selected.generation]--
			}
		})
	}, nil
}

// PrepareLiveCall allocates an unused server capability, never an external call.
// A caller must retain this ticket; Start never allocates a replacement for it.
func (s *Service) PrepareLiveCall(ctx context.Context, p trust.Principal, revision string) (LiveTicket, error) {
	r := s.live
	if !ValidHex(revision, 32) {
		return LiveTicket{}, Failure{Code: Validation}
	}
	scope, err := r.scope(p)
	if err != nil {
		return LiveTicket{}, err
	}
	r.mu.Lock()
	perClient := r.reserved[scope.ClientID]
	for _, entry := range r.entries {
		if entry.scope.ClientID == scope.ClientID {
			perClient++
		}
	}
	if r.closed || len(r.entries)+r.reservations >= maxLiveControlEntries || perClient >= maxLiveClientEntries {
		r.mu.Unlock()
		return LiveTicket{}, Failure{Code: ModelBusy}
	}
	r.reservations++
	r.reserved[scope.ClientID]++
	r.wg.Add(1)
	r.mu.Unlock()
	defer func() {
		r.mu.Lock()
		r.reservations--
		r.reserved[scope.ClientID]--
		if r.reserved[scope.ClientID] == 0 {
			delete(r.reserved, scope.ClientID)
		}
		r.mu.Unlock()
		r.wg.Done()
	}()
	observation, cancel := context.WithTimeout(ctx, 15*time.Second)
	stop := context.AfterFunc(r.ctx, cancel)
	defer stop()
	defer cancel()
	selected, err := s.currentLive(observation)
	if err != nil {
		return LiveTicket{}, err
	}
	if selected.revision != revision {
		return LiveTicket{}, Failure{Code: CapabilityChanged}
	}
	release, err := s.leaseLive(selected)
	if err != nil {
		return LiveTicket{}, err
	}
	admitted := false
	defer func() {
		if !admitted {
			release()
		}
	}()
	prepared, err := selected.account.PrepareLiveTransport(observation, selected.identity)
	if err != nil {
		return LiveTicket{}, err
	}
	if prepared == nil {
		return LiveTicket{}, Failure{Code: ModelUnavailable}
	}
	e := &liveEntry{principal: p, scope: scope, selected: selected, prepared: prepared, phase: "prepared", deadline: time.Now().Add(livePreparationLifetime), releaseSelection: release, deliveryGate: make(chan struct{}, 1), writes: make(map[string]*liveWriteEntry)}
	if err = r.source(observation, e, false); err != nil {
		return LiveTicket{}, err
	}
	if err = observation.Err(); err != nil {
		return LiveTicket{}, err
	}
	r.mu.Lock()
	if r.closed {
		r.mu.Unlock()
		return LiveTicket{}, Failure{Code: ModelUnavailable}
	}
	for {
		e.id = trust.NewID()
		if r.entries[e.id] == nil {
			break
		}
	}
	r.entries[e.id] = e
	r.wg.Add(1)
	admitted = true
	r.mu.Unlock()
	go r.watch(e)
	return LiveTicket{ID: e.id, CapabilityRevision: selected.revision, ExpiresAtUnixMS: e.deadline.UnixMilli()}, nil
}
func liveStartFingerprint(in LiveStartCommand) (string, error) {
	b, err := json.Marshal(in)
	if err != nil {
		return "", Failure{Code: Validation}
	}
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:]), nil
}
func (e *liveEntry) snapshotLocked() LiveCallStatus {
	return LiveCallStatus{TicketID: e.id, Phase: e.phase, RequestID: e.requestID, ExpiresAtUnixMS: e.deadline.UnixMilli(), Attempted: e.attempted, Failure: e.issue}
}

// StartLiveCall consumes the existing capability atomically and gives the
// provider handoff its own bounded owner context. HTTP observation cancellation
// does not cancel that task; exact duplicate Start only reads the same status.
func (s *Service) StartLiveCall(ctx context.Context, p trust.Principal, in LiveStartCommand) (LiveCallStatus, error) {
	r := s.live
	e, err := r.lookup(p, in.TicketID)
	if err != nil {
		return LiveCallStatus{}, err
	}
	if !trust.ValidID(in.RequestID) || !ValidHex(in.CapabilityRevision, 32) || !validClasses(in.DataClasses) || in.DurationMS == 0 || in.DurationMS > e.selected.profile.Limits.LocalSessionDurationMS {
		return LiveCallStatus{}, Failure{Code: Validation}
	}
	if !LiveText(in.Input.Instructions, true) || uint64(len(in.Input.InitialContext)) > e.selected.profile.Limits.InitialContextBytes || !utf8.ValidString(in.Input.InitialContext) || len(in.Input.Offer) > 65536 || !utf8.ValidString(in.Input.Offer) {
		return LiveCallStatus{}, Failure{Code: Validation}
	}
	if in.CapabilityRevision != e.selected.revision {
		return LiveCallStatus{}, Failure{Code: CapabilityChanged}
	}
	if err = r.source(ctx, e, true); err != nil {
		// Request observation cancellation is not owner cancellation.
		if ctx.Err() == nil {
			r.retire(e, liveCause(err))
		}
		return LiveCallStatus{}, err
	}
	fingerprint, err := liveStartFingerprint(in)
	if err != nil {
		return LiveCallStatus{}, err
	}
	if err = ctx.Err(); err != nil {
		return LiveCallStatus{}, err
	}
	r.mu.Lock()
	e.mu.Lock()
	if r.closed || e.retired {
		e.mu.Unlock()
		r.mu.Unlock()
		return LiveCallStatus{}, Failure{Code: NotFound}
	}
	if !time.Now().Before(e.deadline) {
		e.mu.Unlock()
		r.mu.Unlock()
		r.retire(e, ModelTimeout)
		return LiveCallStatus{}, Failure{Code: ModelTimeout}
	}
	if e.phase != "prepared" {
		if e.fingerprint != fingerprint {
			e.mu.Unlock()
			r.mu.Unlock()
			return LiveCallStatus{}, Failure{Code: RequestRejected}
		}
		out := e.snapshotLocked()
		e.mu.Unlock()
		r.mu.Unlock()
		return out, nil
	}
	if err = e.prepared.ValidateStart(in.Input); err != nil {
		e.mu.Unlock()
		r.mu.Unlock()
		return LiveCallStatus{}, err
	}
	e.phase = "starting"
	e.fingerprint = fingerprint
	e.requestID = in.RequestID
	e.deadline = time.Now().Add(time.Duration(in.DurationMS) * time.Millisecond)
	e.controlCtx, e.cancelControl = context.WithDeadline(r.ctx, e.deadline)
	e.transportCtx, e.cancelTransport = context.WithCancel(e.controlCtx)
	e.workers++
	r.wg.Add(1)
	out := e.snapshotLocked()
	e.mu.Unlock()
	r.mu.Unlock()
	go r.start(e, in.Input)
	return out, nil
}
func (r *liveRegistry) start(e *liveEntry, input LiveStartInput) {
	defer r.workerDone(e)
	if err := r.source(e.transportCtx, e, true); err != nil {
		r.fail(e, liveCause(err))
		return
	}
	e.mu.Lock()
	if e.retired || e.phase != "starting" {
		e.mu.Unlock()
		return
	}
	e.attempted = true
	prepared := e.prepared
	e.mu.Unlock()
	transport, err := prepared.Start(e.transportCtx, input)
	if err != nil {
		if transport != nil {
			transport.Fence()
		}
		r.fail(e, liveCause(err))
		return
	}
	if transport == nil {
		r.fail(e, InvalidOutput)
		return
	}
	if err = r.source(e.transportCtx, e, false); err != nil {
		transport.Fence()
		r.fail(e, liveCause(err))
		return
	}
	answer, err := transport.Answer()
	if err != nil {
		transport.Fence()
		r.fail(e, liveCause(err))
		return
	}
	if answer == "" || len(answer) > 65536 || !utf8.ValidString(answer) {
		transport.Fence()
		r.fail(e, InvalidOutput)
		return
	}
	e.mu.Lock()
	if e.retired || e.phase != "starting" || e.transportCtx.Err() != nil {
		e.mu.Unlock()
		transport.Fence()
		return
	}
	e.transport = transport
	e.answerDigest = trust.Digest(answer)
	e.prepared = nil
	e.phase = "ready"
	e.mu.Unlock()
}
func (r *liveRegistry) workerDone(e *liveEntry) {
	e.mu.Lock()
	e.workers--
	remove := e.retired && e.workers == 0
	e.mu.Unlock()
	if remove {
		r.remove(e)
	}
	r.wg.Done()
}
func (r *liveRegistry) fail(e *liveEntry, issue FailureCode) {
	e.mu.Lock()
	if e.issue == "" {
		e.issue = issue
	}
	if !e.retired {
		e.phase = "failed"
	}
	cancel := e.cancelTransport
	transport := e.transport
	e.prepared = nil
	e.mu.Unlock()
	if cancel != nil {
		cancel()
	}
	if transport != nil {
		transport.Fence()
	}
}
func (r *liveRegistry) retire(e *liveEntry, issue FailureCode) {
	e.mu.Lock()
	if e.issue == "" {
		e.issue = issue
	}
	e.retired = true
	e.phase = "closed"
	cancel := e.cancelControl
	transport := e.transport
	e.prepared = nil
	remove := e.workers == 0
	e.mu.Unlock()
	if cancel != nil {
		cancel()
	}
	if transport != nil {
		transport.Fence()
	}
	if remove {
		r.remove(e)
	}
}
func (r *liveRegistry) remove(e *liveEntry) {
	r.mu.Lock()
	if r.entries[e.id] == e {
		delete(r.entries, e.id)
	}
	r.mu.Unlock()
	e.releaseSelection()
}
func (r *liveRegistry) watch(e *liveEntry) {
	defer r.wg.Done()
	ticker := time.NewTicker(liveControlCheckInterval)
	defer ticker.Stop()
	for {
		select {
		case <-r.ctx.Done():
			r.retire(e, ModelUnavailable)
			return
		case <-ticker.C:
		}
		e.mu.Lock()
		expired := !time.Now().Before(e.deadline)
		retired := e.retired
		transport := e.transport
		expectedClose := e.phase == "closing" || e.phase == "closed_local"
		e.mu.Unlock()
		if retired {
			return
		}
		if expired {
			r.retire(e, ModelTimeout)
			return
		}
		if err := r.source(r.ctx, e, false); err != nil {
			r.retire(e, liveCause(err))
			return
		}
		if transport != nil {
			select {
			case <-transport.Done():
				if !expectedClose {
					r.fail(e, ModelUnavailable)
				}
			default:
			}
		}
	}
}
func (s *Service) LiveCallStatus(ctx context.Context, p trust.Principal, id string) (LiveCallStatus, error) {
	r := s.live
	e, err := r.lookup(p, id)
	if err != nil {
		return LiveCallStatus{}, err
	}
	if err = r.source(ctx, e, false); err != nil {
		// Request observation cancellation is not owner cancellation.
		if ctx.Err() == nil {
			r.retire(e, liveCause(err))
		}
		return LiveCallStatus{}, err
	}
	e.mu.Lock()
	if e.retired {
		e.mu.Unlock()
		return LiveCallStatus{}, Failure{Code: NotFound}
	}
	out := e.snapshotLocked()
	transport := e.transport
	digest := e.answerDigest
	e.mu.Unlock()
	if out.Phase == "ready" {
		answer, answerErr := transport.Answer()
		if answerErr != nil {
			issue := liveCause(answerErr)
			r.fail(e, issue)
			return LiveCallStatus{}, Failure{Code: issue}
		}
		if trust.Digest(answer) != digest {
			r.fail(e, InvalidOutput)
			return LiveCallStatus{}, Failure{Code: InvalidOutput}
		}
		if err = r.source(ctx, e, false); err != nil {
			if ctx.Err() == nil {
				r.retire(e, liveCause(err))
			}
			return LiveCallStatus{}, err
		}
		e.mu.Lock()
		current := !e.retired && e.phase == "ready" && time.Now().Before(e.deadline) && e.controlCtx.Err() == nil
		e.mu.Unlock()
		if !current {
			return LiveCallStatus{}, Failure{Code: ModelUnavailable}
		}
		out.Answer = answer
	}
	return out, nil
}

// Release retires only an already-issued control capability. It cannot create
// or resume a provider call, and its local close is not remote finalization.
func (s *Service) ReleaseLiveCall(p trust.Principal, id string) error {
	e, err := s.live.lookup(p, id)
	if err != nil {
		return err
	}
	s.live.retire(e, "")
	return nil
}
func (s *Service) CloseLiveAdmission() {
	r := s.live
	r.mu.Lock()
	r.closed = true
	r.cancel()
	entries := make([]*liveEntry, 0, len(r.entries))
	for _, e := range r.entries {
		entries = append(entries, e)
	}
	r.mu.Unlock()
	for _, e := range entries {
		r.retire(e, ModelUnavailable)
	}
}
func (s *Service) ShutdownLive(ctx context.Context) error {
	s.CloseLiveAdmission()
	r := s.live
	done := make(chan struct{})
	go func() { r.wg.Wait(); close(done) }()
	select {
	case <-done:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

// List exposes only this authenticated client's process-bound control handles,
// allowing cleanup after a lost UI response. It never exposes SDP or input text.
func (s *Service) ListLiveCalls(p trust.Principal) ([]LiveCallStatus, error) {
	r := s.live
	scope, err := r.scope(p)
	if err != nil {
		return nil, err
	}
	r.mu.Lock()
	entries := make([]*liveEntry, 0, len(r.entries))
	for _, e := range r.entries {
		if sameLiveScope(scope, e.scope) {
			entries = append(entries, e)
		}
	}
	closed := r.closed
	r.mu.Unlock()
	if closed {
		return nil, Failure{Code: ModelUnavailable}
	}
	out := make([]LiveCallStatus, 0, len(entries))
	for _, e := range entries {
		e.mu.Lock()
		if !e.retired {
			out = append(out, e.snapshotLocked())
		}
		e.mu.Unlock()
	}
	return out, nil
}
