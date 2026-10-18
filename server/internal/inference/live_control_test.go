package inference

import (
	"context"
	"errors"
	"floe/server/internal/trust"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

type controlTrust struct {
	denied     atomic.Bool
	scopeValue atomic.Pointer[trust.PrincipalSnapshot]
}

func (t *controlTrust) WithCurrentPrincipal(_ trust.Principal, f func(trust.PrincipalSnapshot) error) error {
	if t.denied.Load() {
		return errors.New("synthetic revoked principal")
	}
	return f(*t.scopeValue.Load())
}
func (t *controlTrust) WithCurrentOperator(_ trust.OperatorPrincipal, f func() error) error {
	return f()
}
func (t *controlTrust) ActiveIssuer(trust.Principal) (trust.IssuerSnapshot, error) {
	if t.denied.Load() {
		return trust.IssuerSnapshot{}, errors.New("revoked")
	}
	return trust.IssuerSnapshot{}, nil
}

type controlAccount struct {
	prepares, starts atomic.Int32
	release          chan struct{}
	ignoreCancel     bool
	transport        *controlTransport
}

func (a *controlAccount) Ready(ctx context.Context) error { return ctx.Err() }
func (a *controlAccount) ReplayIdentity() string          { return "synthetic-fixed-account" }
func (a *controlAccount) ProtocolCapabilities() []string  { return []string{} }
func (a *controlAccount) ModelIdentity() ModelIdentity {
	return ModelIdentity{ProviderID: "codex_oauth", ModelID: "gpt-live-1-codex", Endpoint: "https://chatgpt.com/backend-api/codex"}
}
func (a *controlAccount) BudgetOverride() *ModelBudgetOverride { return nil }
func (a *controlAccount) LiveProfile() (LiveProfile, bool) {
	return LiveProfile{ModelIdentity: a.ModelIdentity(), Capabilities: append([]string(nil), liveCapabilityOrder...), Limits: LiveTransportLimits{2048, 500, 10000}, ProtocolSource: "synthetic-transport", Experimental: true}, true
}
func (a *controlAccount) PrepareLiveTransport(ctx context.Context, expected string) (PreparedLiveTransport, error) {
	a.prepares.Add(1)
	if expected != a.ReplayIdentity() {
		return nil, Failure{Code: CapabilityChanged}
	}
	return &controlPrepared{a}, ctx.Err()
}

type controlPrepared struct{ account *controlAccount }

func (p *controlPrepared) ValidateStart(in LiveStartInput) error {
	if !strings.HasPrefix(in.Offer, "v=0\r\n") || !LiveText(in.Instructions, true) {
		return Failure{Code: Validation}
	}
	return nil
}
func (p *controlPrepared) Start(ctx context.Context, in LiveStartInput) (LiveTransport, error) {
	p.account.starts.Add(1)
	if p.account.release != nil {
		if p.account.ignoreCancel {
			<-p.account.release
		} else {
			select {
			case <-p.account.release:
			case <-ctx.Done():
				return nil, ctx.Err()
			}
		}
	}
	return p.account.transport, nil
}

type controlTransport struct {
	deliveryMu    sync.Mutex
	deliveries    []LiveDelivery
	acks          atomic.Int32
	closed        atomic.Bool
	changedAnswer atomic.Bool
	once          sync.Once
	done          chan struct{}
}

func (t *controlTransport) Answer() (string, error) {
	if t.closed.Load() {
		return "", Failure{Code: ModelUnavailable}
	}
	if t.changedAnswer.Load() {
		return "v=0\r\ns=changed\r\n", nil
	}
	return "v=0\r\ns=synthetic-answer\r\n", nil
}
func (t *controlTransport) Observe(ctx context.Context) (LiveDelivery, error) {
	t.deliveryMu.Lock()
	if len(t.deliveries) > 0 {
		d := t.deliveries[0]
		t.deliveryMu.Unlock()
		return d, ctx.Err()
	}
	t.deliveryMu.Unlock()
	<-ctx.Done()
	return LiveDelivery{}, ctx.Err()
}
func (t *controlTransport) Acknowledge(sequence uint64) error {
	t.deliveryMu.Lock()
	defer t.deliveryMu.Unlock()
	if len(t.deliveries) == 0 || t.deliveries[0].Sequence != sequence {
		return Failure{Code: RequestRejected}
	}
	t.deliveries = t.deliveries[1:]
	t.acks.Add(1)
	return nil
}
func (t *controlTransport) PrepareAppend(LiveAppend) (PreparedLiveWrite, error) {
	panic("start fixture cannot append")
}
func (t *controlTransport) RequestClose(context.Context) (bool, error) { t.Fence(); return true, nil }
func (t *controlTransport) Fence()                                     { t.closed.Store(true); t.once.Do(func() { close(t.done) }) }
func (t *controlTransport) Done() <-chan struct{}                      { return t.done }
func controlFixture(t *testing.T, blocked, ignoreCancel bool) (*Service, *controlAccount, *controlTrust) {
	t.Helper()
	authority := &controlTrust{}
	authority.scopeValue.Store(&trust.PrincipalSnapshot{ClientID: "11111111-1111-4111-8111-111111111111", PersonID: "22222222-2222-4222-8222-222222222222", DeviceID: "fixture-device", Generation: 1, Active: true})
	service, err := NewService(authority, emptyCapabilityMetadata())
	if err != nil {
		t.Fatal(err)
	}
	account := &controlAccount{ignoreCancel: ignoreCancel, transport: &controlTransport{done: make(chan struct{})}}
	if blocked {
		account.release = make(chan struct{})
	}
	err = service.Configure(InferenceConfig{Routes: map[Purpose]PurposeRoute{LivePresentation: {TargetID: "live", Enabled: true}}}, map[string]ModelAccount{"live": account}, selectionTestExecutor{})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), time.Second)
		defer cancel()
		if err := service.ShutdownLive(ctx); err != nil {
			t.Error(err)
		}
	})
	return service, account, authority
}
func controlTicket(t *testing.T, s *Service) (LiveTicket, LiveStartCommand) {
	t.Helper()
	capability, err := s.ObserveLivePurpose(context.Background(), trust.Principal{})
	if err != nil {
		t.Fatal(err)
	}
	ticket, err := s.PrepareLiveCall(context.Background(), trust.Principal{}, capability.CapabilityRevision)
	if err != nil {
		t.Fatal(err)
	}
	return ticket, LiveStartCommand{TicketID: ticket.ID, RequestID: "33333333-3333-4333-8333-333333333333", CapabilityRevision: ticket.CapabilityRevision, DurationMS: 5000, DataClasses: []string{"synthetic"}, Input: LiveStartInput{Instructions: "synthetic role", InitialContext: "synthetic context", Offer: "v=0\r\ns=fixture\r\n"}}
}
func awaitControl(t *testing.T, s *Service, ticket LiveTicket, phase string) LiveCallStatus {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		status, err := s.LiveCallStatus(context.Background(), trust.Principal{}, ticket.ID)
		if err == nil && status.Phase == phase {
			return status
		}
		time.Sleep(time.Millisecond)
	}
	t.Fatal("live phase did not settle", phase)
	return LiveCallStatus{}
}
func TestLiveStartTicketIsOneShotAndObserverCancellationDoesNotRetireIt(t *testing.T) {
	s, a, _ := controlFixture(t, true, false)
	ticket, in := controlTicket(t, s)
	observer, cancel := context.WithCancel(context.Background())
	first, err := s.StartLiveCall(observer, trust.Principal{}, in)
	if err != nil || first.Phase != "starting" {
		t.Fatal("start not admitted", first, err)
	}
	cancel()
	if _, err = s.LiveCallStatus(observer, trust.Principal{}, ticket.ID); !errors.Is(err, context.Canceled) {
		t.Fatal("cancelled status did not remain an observation", err)
	}
	if _, err = s.StartLiveCall(observer, trust.Principal{}, in); !errors.Is(err, context.Canceled) {
		t.Fatal("cancelled duplicate did not remain an observation", err)
	}
	close(a.release)
	ready := awaitControl(t, s, ticket, "ready")
	if ready.Answer == "" || !ready.Attempted {
		t.Fatal("ready evidence missing")
	}
	duplicate, err := s.StartLiveCall(context.Background(), trust.Principal{}, in)
	if err != nil || duplicate.Phase != "ready" {
		t.Fatal("exact retry failed", err)
	}
	changed := in
	changed.Input.InitialContext = "different"
	if _, err = s.StartLiveCall(context.Background(), trust.Principal{}, changed); err == nil {
		t.Fatal("changed retry accepted")
	}
	if a.starts.Load() != 1 || a.prepares.Load() != 1 {
		t.Fatal("observer/retry recreated provider call")
	}
	if err = s.ReleaseLiveCall(trust.Principal{}, ticket.ID); err != nil {
		t.Fatal(err)
	}
	if !a.transport.closed.Load() {
		t.Fatal("release did not fence transport")
	}
	if _, err = s.StartLiveCall(context.Background(), trust.Principal{}, in); err == nil {
		t.Fatal("retired ticket became a fresh call")
	}
}
func TestLiveTicketCannotBeRecreatedAfterServerRestart(t *testing.T) {
	old, a, _ := controlFixture(t, false, false)
	ticket, in := controlTicket(t, old)
	if _, err := old.StartLiveCall(context.Background(), trust.Principal{}, in); err != nil {
		t.Fatal(err)
	}
	awaitControl(t, old, ticket, "ready")
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	if err := old.ShutdownLive(ctx); err != nil {
		t.Fatal(err)
	}
	next, newAccount, _ := controlFixture(t, false, false)
	for _, revision := range []string{in.CapabilityRevision, func() string {
		v, _ := next.ObserveLivePurpose(context.Background(), trust.Principal{})
		return v.CapabilityRevision
	}()} {
		in.CapabilityRevision = revision
		if _, err := next.StartLiveCall(context.Background(), trust.Principal{}, in); err == nil {
			t.Fatal("unknown prior-process ticket dispatched")
		}
	}
	if a.starts.Load() != 1 || newAccount.starts.Load() != 0 || newAccount.prepares.Load() != 0 {
		t.Fatal("restart replay created a provider operation")
	}
}
func TestExpiredUnusedTicketDoesNotDispatch(t *testing.T) {
	s, a, _ := controlFixture(t, false, false)
	ticket, in := controlTicket(t, s)
	s.live.mu.Lock()
	entry := s.live.entries[ticket.ID]
	s.live.mu.Unlock()
	entry.mu.Lock()
	entry.deadline = time.Now().Add(-time.Second)
	entry.mu.Unlock()
	if _, err := s.StartLiveCall(context.Background(), trust.Principal{}, in); err == nil {
		t.Fatal("expired preparation dispatched")
	}
	if a.starts.Load() != 0 {
		t.Fatal("expiry reached provider")
	}
}
func TestLiveShutdownFencesALateStartWithoutPublishingItsAnswer(t *testing.T) {
	s, a, _ := controlFixture(t, true, true)
	ticket, in := controlTicket(t, s)
	if _, err := s.StartLiveCall(context.Background(), trust.Principal{}, in); err != nil {
		t.Fatal(err)
	}
	deadline := time.Now().Add(time.Second)
	for a.starts.Load() == 0 && time.Now().Before(deadline) {
		time.Sleep(time.Millisecond)
	}
	if a.starts.Load() != 1 {
		close(a.release)
		t.Fatal("start not entered")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Millisecond)
	err := s.ShutdownLive(ctx)
	cancel()
	if !errors.Is(err, context.DeadlineExceeded) {
		close(a.release)
		t.Fatal("shutdown invented a completed drain", err)
	}
	close(a.release)
	ctx, cancel = context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	if err = s.ShutdownLive(ctx); err != nil {
		t.Fatal(err)
	}
	if !a.transport.closed.Load() {
		t.Fatal("late returned transport escaped fence")
	}
	if _, err = s.LiveCallStatus(context.Background(), trust.Principal{}, ticket.ID); err == nil {
		t.Fatal("closed owner published late SDP")
	}
}
func TestLiveTicketImmutableAnswerIsChecked(t *testing.T) {
	s, a, _ := controlFixture(t, false, false)
	ticket, in := controlTicket(t, s)
	if _, err := s.StartLiveCall(context.Background(), trust.Principal{}, in); err != nil {
		t.Fatal(err)
	}
	awaitControl(t, s, ticket, "ready")
	a.transport.changedAnswer.Store(true)
	if _, err := s.LiveCallStatus(context.Background(), trust.Principal{}, ticket.ID); err == nil {
		t.Fatal("mutated answer was released")
	}
	if !a.transport.closed.Load() {
		t.Fatal("invalid answer did not fence")
	}
}

func TestLiveConcurrentStartsConsumeOnlyOneProviderHandoff(t *testing.T) {
	s, a, _ := controlFixture(t, true, false)
	ticket, in := controlTicket(t, s)
	var wg sync.WaitGroup
	for i := 0; i < 32; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := s.StartLiveCall(context.Background(), trust.Principal{}, in); err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	close(a.release)
	awaitControl(t, s, ticket, "ready")
	if a.starts.Load() != 1 {
		t.Fatal("concurrent retries duplicated external creation")
	}
}

func TestLiveUnusedCapabilitiesAreBoundedAndReleaseTheirSelectionLease(t *testing.T) {
	s, a, _ := controlFixture(t, false, false)
	first, _ := controlTicket(t, s)
	second, _ := controlTicket(t, s)
	cap, err := s.ObserveLivePurpose(context.Background(), trust.Principal{})
	if err != nil {
		t.Fatal(err)
	}
	if _, err = s.PrepareLiveCall(context.Background(), trust.Principal{}, cap.CapabilityRevision); liveCause(err) != ModelBusy {
		t.Fatal("unbounded preparation admission", err)
	}
	if a.prepares.Load() != 2 || a.starts.Load() != 0 {
		t.Fatal("rejected preparation touched provider")
	}
	for _, ticket := range []LiveTicket{first, second} {
		if err := s.ReleaseLiveCall(trust.Principal{}, ticket.ID); err != nil {
			t.Fatal(err)
		}
	}
	s.mu.RLock()
	leases := len(s.leases)
	s.mu.RUnlock()
	if leases != 0 {
		t.Fatal("retired preparation retained generation lease")
	}
	s.live.mu.Lock()
	reservations := len(s.live.reserved)
	s.live.mu.Unlock()
	if reservations != 0 {
		t.Fatal("completed reservations retained client identities")
	}
}

func TestLivePrincipalRevocationFencesWithoutStatusObserver(t *testing.T) {
	s, a, authority := controlFixture(t, false, false)
	ticket, in := controlTicket(t, s)
	if _, err := s.StartLiveCall(context.Background(), trust.Principal{}, in); err != nil {
		t.Fatal(err)
	}
	awaitControl(t, s, ticket, "ready")
	authority.denied.Store(true)
	select {
	case <-a.transport.Done():
	case <-time.After(time.Second):
		t.Fatal("revocation depended on a polling client")
	}
}
