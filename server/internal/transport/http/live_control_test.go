package httptransport

import (
	"context"
	"encoding/json"
	"floe/server/internal/adapters/modelmetadata"
	"floe/server/internal/inference"
	"floe/server/internal/trust"
	"net/http"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

type liveHTTPAccount struct {
	liveObservationAccount
	starts   atomic.Int32
	prepares atomic.Int32
	call     *liveHTTPTransport
}

func (a *liveHTTPAccount) PrepareLiveTransport(context.Context, string) (inference.PreparedLiveTransport, error) {
	a.prepares.Add(1)
	return &liveHTTPPrepared{a}, nil
}

type liveHTTPPrepared struct{ account *liveHTTPAccount }

func (p *liveHTTPPrepared) ValidateStart(in inference.LiveStartInput) error {
	if !strings.HasPrefix(in.Offer, "v=0\r\n") {
		return inference.Failure{Code: inference.Validation}
	}
	return nil
}
func (p *liveHTTPPrepared) Start(context.Context, inference.LiveStartInput) (inference.LiveTransport, error) {
	p.account.starts.Add(1)
	return p.account.call, nil
}

type liveHTTPTransport struct {
	acks   atomic.Int32
	closed atomic.Bool
	once   sync.Once
	done   chan struct{}
}

func (t *liveHTTPTransport) Answer() (string, error) {
	if t.closed.Load() {
		return "", inference.Failure{Code: inference.ModelUnavailable}
	}
	return "v=0\r\ns=synthetic-private-answer\r\n", nil
}
func (t *liveHTTPTransport) Observe(ctx context.Context) (inference.LiveDelivery, error) {
	if t.acks.Load() == 0 {
		return inference.LiveDelivery{Sequence: 1, Event: inference.LiveEvent{Kind: "person_utterance_final", UtteranceID: "synthetic-utterance", Text: "synthetic-private-event"}}, ctx.Err()
	}
	<-ctx.Done()
	return inference.LiveDelivery{}, ctx.Err()
}
func (t *liveHTTPTransport) Acknowledge(sequence uint64) error {
	if sequence != 1 || !t.acks.CompareAndSwap(0, 1) {
		return inference.Failure{Code: inference.RequestRejected}
	}
	return nil
}
func (t *liveHTTPTransport) PrepareAppend(inference.LiveAppend) (inference.PreparedLiveWrite, error) {
	panic("start fixture cannot append")
}
func (t *liveHTTPTransport) RequestClose(context.Context) (bool, error) { t.Fence(); return true, nil }
func (t *liveHTTPTransport) Fence()                                     { t.closed.Store(true); t.once.Do(func() { close(t.done) }) }
func (t *liveHTTPTransport) Done() <-chan struct{}                      { return t.done }
func configureHTTPLive(t *testing.T, f *consoleFixture, a *liveHTTPAccount) {
	t.Helper()
	if err := f.handler.Inference.Service.Configure(inference.InferenceConfig{Routes: map[inference.Purpose]inference.PurposeRoute{inference.LivePresentation: {TargetID: "live", Enabled: true}}}, map[string]inference.ModelAccount{"live": a}, noLiveGeneration{}); err != nil {
		t.Fatal(err)
	}
}
func liveJSON(t *testing.T, v any) []byte {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return b
}
func TestLiveHTTPCallIsPrincipalBoundOneShotAndNotRecreatedAfterRestart(t *testing.T) {
	f := newConsoleFixture(t)
	cookie, csrf := f.login(t)
	owner := completeSyntheticPairing(t, f, cookie, csrf)
	principal, err := f.trust.AuthenticateBearer(context.Background(), owner)
	if err != nil {
		t.Fatal(err)
	}
	// Exercise the real pairing owner, including its ten-second enrollment throttle.
	time.Sleep(10 * time.Second)
	other := finishSyntheticPairing(t, f, cookie, csrf, f.startPairingForPerson(t, principal.PersonID()))
	a := &liveHTTPAccount{call: &liveHTTPTransport{done: make(chan struct{})}}
	configureHTTPLive(t, f, a)
	observed := f.inferenceRequest(http.MethodGet, "/v1/live-model", nil, owner)
	var capability livePurposeDTO
	if observed.Code != 200 || json.Unmarshal(observed.Body.Bytes(), &capability) != nil {
		t.Fatal("observation failed")
	}
	prepared := f.inferenceRequest(http.MethodPost, "/v1/live/prepare", liveJSON(t, livePrepareRequest{1, capability.CapabilityRevision}), owner)
	var ticket liveTicketDTO
	if prepared.Code != 200 || json.Unmarshal(prepared.Body.Bytes(), &ticket) != nil {
		t.Fatal("preparation failed", prepared.Code, prepared.Body.String())
	}
	if a.starts.Load() != 0 || a.prepares.Load() != 1 {
		t.Fatal("preparation created a call")
	}
	request := liveStartRequest{SchemaVersion: 1, TicketID: ticket.TicketID, RequestID: trust.NewID(), CapabilityRevision: ticket.CapabilityRevision, DurationMS: 5000, DataClasses: []string{"synthetic"}, Instructions: "synthetic role", InitialContext: "synthetic private context", Offer: "v=0\r\ns=synthetic-private-offer\r\n"}
	body := liveJSON(t, request)
	started := f.inferenceRequest(http.MethodPost, "/v1/live/start", body, owner)
	if started.Code != 200 {
		t.Fatal("start rejected", started.Code, started.Body.String())
	}
	deadline := time.Now().Add(2 * time.Second)
	var ready liveStatusDTO
	for time.Now().Before(deadline) {
		r := f.inferenceRequest(http.MethodGet, "/v1/live/calls/"+ticket.TicketID, nil, owner)
		if r.Code == 200 && json.Unmarshal(r.Body.Bytes(), &ready) == nil && ready.Phase == "ready" {
			break
		}
		time.Sleep(time.Millisecond)
	}
	if ready.Phase != "ready" || ready.Answer == "" {
		t.Fatal("ready answer missing")
	}
	for i := 0; i < 3; i++ {
		if r := f.inferenceRequest(http.MethodPost, "/v1/live/start", body, owner); r.Code != 200 {
			t.Fatal("exact retry failed", r.Code)
		}
	}
	request.InitialContext = "changed"
	if r := f.inferenceRequest(http.MethodPost, "/v1/live/start", liveJSON(t, request), owner); r.Code == 200 {
		t.Fatal("changed start accepted")
	}
	if r := f.inferenceRequest(http.MethodGet, "/v1/live/calls/"+ticket.TicketID, nil, other); r.Code != http.StatusForbidden || strings.Contains(r.Body.String(), "synthetic-private-answer") {
		t.Fatal("foreign client read answer", r.Code)
	}
	if r := f.inferenceRequest(http.MethodPost, "/v1/live/release", liveJSON(t, liveReleaseRequest{1, ticket.TicketID}), other); r.Code != http.StatusForbidden {
		t.Fatal("foreign client closed the call", r.Code)
	}
	eventPath := "/v1/live/calls/" + ticket.TicketID + "/event"
	if r := f.inferenceRequest(http.MethodGet, eventPath, nil, other); r.Code != http.StatusForbidden || strings.Contains(r.Body.String(), "synthetic-private-event") {
		t.Fatal("foreign client read retained event", r.Code)
	}
	var event liveEventDTO
	for i := 0; i < 2; i++ {
		r := f.inferenceRequest(http.MethodGet, eventPath, nil, owner)
		if r.Code != 200 || json.Unmarshal(r.Body.Bytes(), &event) != nil || event.Sequence != 1 || event.Event.Text != "synthetic-private-event" {
			t.Fatal("retained event response invalid", r.Code)
		}
	}
	ack := liveEventAckRequest{1, ticket.TicketID, event.Sequence, event.Digest}
	if r := f.inferenceRequest(http.MethodPost, "/v1/live/event/ack", liveJSON(t, ack), other); r.Code != http.StatusForbidden {
		t.Fatal("foreign client acknowledged event", r.Code)
	}
	for i := 0; i < 2; i++ {
		if r := f.inferenceRequest(http.MethodPost, "/v1/live/event/ack", liveJSON(t, ack), owner); r.Code != 200 {
			t.Fatal("exact ACK retry failed", r.Code)
		}
	}
	if a.call.acks.Load() != 1 {
		t.Fatal("ACK response loss duplicated queue mutation")
	}
	listed := f.inferenceRequest(http.MethodGet, "/v1/live/calls", nil, owner)
	if listed.Code != 200 || strings.Contains(listed.Body.String(), "synthetic-private") || strings.Contains(listed.Body.String(), "answer") {
		t.Fatal("handle inventory leaked signaling or input")
	}
	foreignList := f.inferenceRequest(http.MethodGet, "/v1/live/calls", nil, other)
	if strings.Contains(foreignList.Body.String(), ticket.TicketID) {
		t.Fatal("foreign client listed ticket")
	}
	if a.call.closed.Load() {
		t.Fatal("foreign request fenced legitimate call")
	}
	h := f.handler.Inference
	h.liveBodiesOnce.Do(func() { h.liveBodies = make(chan struct{}, 2) })
	h.liveBodies <- struct{}{}
	h.liveBodies <- struct{}{}
	busy := f.inferenceRequest(http.MethodPost, "/v1/live/start", body, owner)
	if busy.Code != http.StatusTooManyRequests {
		t.Fatal("body admission pressure was not answered", busy.Code)
	}
	if read := f.inferenceRequest(http.MethodGet, "/v1/live/calls/"+ticket.TicketID, nil, owner); read.Code != 200 {
		t.Fatal("body pressure blocked read control")
	}
	<-h.liveBodies
	<-h.liveBodies
	if a.starts.Load() != 1 {
		t.Fatal("exact/changed/foreign requests duplicated provider creation")
	}
	old := h.Service
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	if err := old.ShutdownLive(ctx); err != nil {
		t.Fatal(err)
	}
	cancel()
	next, err := inference.NewService(f.trust, modelmetadata.NewCatalogSource(f.catalog))
	if err != nil {
		t.Fatal(err)
	}
	h.Service = next
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), time.Second)
		defer cancel()
		if err := next.ShutdownLive(ctx); err != nil {
			t.Error(err)
		}
	})
	configureHTTPLive(t, f, a)
	if r := f.inferenceRequest(http.MethodPost, "/v1/live/start", body, owner); r.Code != http.StatusNotFound {
		t.Fatal("unknown prior-process ticket was not rejected", r.Code)
	}
	if a.starts.Load() != 1 || a.prepares.Load() != 1 {
		t.Fatal("restart request silently re-prepared or re-created")
	}
}
