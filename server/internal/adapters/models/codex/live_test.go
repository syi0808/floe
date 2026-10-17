package codexauth

import (
	"context"
	"encoding/json"
	"errors"
	"floe/server/internal/inference"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"reflect"
	"strings"
	"sync/atomic"
	"testing"
	"time"
	"unicode/utf8"

	"github.com/coder/websocket"
)

const fixtureOperation = "11111111-1111-4111-8111-111111111111"

func liveFixtureRuntime(t *testing.T, handler http.HandlerFunc) *Runtime {
	t.Helper()
	server := httptest.NewServer(handler)
	t.Cleanup(server.Close)
	r := New(&refreshStore{})
	r.tokens = &tokenBundle{AccessToken: "synthetic-live-access", RefreshToken: "synthetic-live-refresh", AccountID: "synthetic-live-account", ExpiresAt: time.Now().Add(time.Hour)}
	r.liveEndpoint = server.URL + "/create?intent=quicksilver&architecture=avas"
	r.liveSidebandBase = server.URL + "/sideband/"
	t.Cleanup(r.Close)
	return r
}
func liveFixtureStart() LiveStart {
	return LiveStart{Model: "gpt-live-1-codex", Instructions: "Synthetic role instructions", InitialContext: `{"source":"synthetic"}`, SDP: "v=0\r\ns=synthetic-offer\r\n"}
}
func liveFixtureContext(t *testing.T) context.Context {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	t.Cleanup(cancel)
	return ctx
}
func liveFixturePrepare(t *testing.T, r *Runtime) *LivePrepared {
	t.Helper()
	p, err := r.PrepareLive(liveFixtureContext(t), r.ReplayIdentity())
	if err != nil {
		t.Fatal(err)
	}
	return p
}
func liveFixtureCall(t *testing.T, r *Runtime) *LiveCall {
	t.Helper()
	c, err := liveFixturePrepare(t, r).Start(liveFixtureContext(t), liveFixtureStart())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(c.CloseLocal)
	return c
}
func liveFixtureAnswer(w http.ResponseWriter) {
	w.Header().Set("Location", "/v1/realtime/calls/calls/rtc_fixture")
	w.Header().Set("Content-Type", "application/sdp")
	w.WriteHeader(http.StatusCreated)
	_, _ = w.Write([]byte("v=0\r\ns=synthetic-answer\r\n"))
}

func TestLiveCreateIsOneShotWithInitialContextAndNoTokenReadback(t *testing.T) {
	var requests atomic.Int32
	r := liveFixtureRuntime(t, func(w http.ResponseWriter, req *http.Request) {
		requests.Add(1)
		if req.Method != "POST" || req.URL.Path != "/create" || req.URL.Query().Get("intent") != "quicksilver" || req.URL.Query().Get("architecture") != "avas" {
			t.Error("incorrect create route")
		}
		if req.Header.Get("Authorization") != "Bearer synthetic-live-access" || req.Header.Get("ChatGPT-Account-Id") != "synthetic-live-account" {
			t.Error("wrong synthetic account generation")
		}
		var body struct {
			SDP     string         `json:"sdp"`
			Session map[string]any `json:"session"`
		}
		if json.NewDecoder(req.Body).Decode(&body) != nil || body.SDP != liveFixtureStart().SDP || body.Session["model"] != "gpt-live-1-codex" || body.Session["initial_items"] == nil {
			t.Error("initial context absent from create")
		}
		if _, ok := body.Session["id"]; ok {
			t.Error("host ID is not a provider session ID")
		}
		liveFixtureAnswer(w)
	})
	p := liveFixturePrepare(t, r)
	if _, err := p.Start(context.Background(), liveFixtureStart()); err == nil {
		t.Fatal("unbounded owner lifetime accepted")
	}
	c, err := p.Start(liveFixtureContext(t), liveFixtureStart())
	if err != nil {
		t.Fatal(err)
	}
	defer c.CloseLocal()
	answer, answerErr := c.AnswerSDP()
	if c.CallID() != "rtc_fixture" || answerErr != nil || answer != "v=0\r\ns=synthetic-answer\r\n" {
		t.Fatal("lost exact signaling result")
	}
	if _, err = p.Start(liveFixtureContext(t), liveFixtureStart()); !errors.Is(err, errLiveConsumed) || requests.Load() != 1 {
		t.Fatal("create was retried")
	}
	for _, v := range []any{p, c, liveFixtureStart()} {
		for _, format := range []string{"%v", "%+v", "%#v"} {
			output := fmt.Sprintf(format, v)
			if strings.Contains(output, "synthetic") {
				t.Fatal("debug leaked signaling or credentials")
			}
		}
	}
}
func TestLiveLocationIsNotAnArbitrarySidebandDestination(t *testing.T) {
	for _, value := range []string{"https://evil.example/v1/realtime/calls/rtc_bad", "//evil.example/v1/realtime/calls/rtc_bad", "/v1/realtime/calls/../rtc_bad", "/v1/realtime/calls/rtc_bad%2fnext", "/v1/realtime/calls/rtc_bad?token=hidden", "/v1/realtime/calls/rtc_", "/unrelated/rtc_bad", "/v1/realtime/calls/rtc_bad/"} {
		if _, err := parseLiveLocation(value); err == nil {
			t.Fatalf("unsafe location accepted: %q", value)
		}
	}
	for _, value := range []string{"/v1/realtime/calls/calls/rtc_valid", "https://api.openai.com/v1/realtime/calls/rtc_valid", "/v1/realtime/calls/11111111-1111-4111-8111-111111111111"} {
		if _, err := parseLiveLocation(value); err != nil {
			t.Fatal("valid fixed-route location rejected")
		}
	}
}
func TestLiveCreateFailuresRemainConsumedAndNeverFollowRedirects(t *testing.T) {
	var redirected atomic.Int32
	target := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) { redirected.Add(1) }))
	defer target.Close()
	for _, status := range []int{http.StatusTemporaryRedirect, http.StatusUnauthorized, http.StatusTooManyRequests, http.StatusInternalServerError, http.StatusCreated} {
		t.Run(fmt.Sprint(status), func(t *testing.T) {
			var requests atomic.Int32
			r := liveFixtureRuntime(t, func(w http.ResponseWriter, req *http.Request) {
				requests.Add(1)
				w.Header().Set("Location", target.URL+"/rtc_wrong")
				w.WriteHeader(status)
				_, _ = w.Write([]byte("provider-secret-body"))
			})
			p := liveFixturePrepare(t, r)
			_, err := p.Start(liveFixtureContext(t), liveFixtureStart())
			var failure LiveStartError
			if !errors.As(err, &failure) || !failure.Dispatched || strings.Contains(err.Error(), "secret") {
				t.Fatal("lost dispatched uncertainty or leaked body")
			}
			if _, err := p.Start(liveFixtureContext(t), liveFixtureStart()); !errors.Is(err, errLiveConsumed) || requests.Load() != 1 {
				t.Fatal("uncertain create repeated")
			}
		})
	}
	if redirected.Load() != 0 {
		t.Fatal("credential-bearing redirect followed")
	}
}
func TestLiveCredentialRotationFencesPreparedAndActiveCalls(t *testing.T) {
	var creates atomic.Int32
	r := liveFixtureRuntime(t, func(w http.ResponseWriter, req *http.Request) { creates.Add(1); liveFixtureAnswer(w) })
	stale := liveFixturePrepare(t, r)
	active := liveFixtureCall(t, r)
	next := &tokenBundle{AccessToken: "synthetic-next", RefreshToken: "synthetic-next-refresh", AccountID: "synthetic-live-account", ExpiresAt: time.Now().Add(time.Hour)}
	if err := r.save(context.Background(), next); err != nil {
		t.Fatal(err)
	}
	if active.current() || !active.closed.Load() {
		t.Fatal("old live call survived rotation")
	}
	if _, err := stale.Start(liveFixtureContext(t), liveFixtureStart()); err == nil || creates.Load() != 1 {
		t.Fatal("old prepared identity dispatched")
	}
	if _, err := r.PrepareLive(liveFixtureContext(t), "wrong-identity"); err == nil {
		t.Fatal("foreign selected account accepted")
	}
	fresh := liveFixturePrepare(t, r)
	r.Close()
	if _, err := fresh.Start(liveFixtureContext(t), liveFixtureStart()); err == nil {
		t.Fatal("shutdown did not fence prepared calls")
	}
}
func TestLiveRotationCancelsInFlightCreateWithoutRetry(t *testing.T) {
	entered := make(chan struct{})
	released := make(chan struct{})
	defer close(released)
	var requests atomic.Int32
	r := liveFixtureRuntime(t, func(w http.ResponseWriter, req *http.Request) {
		requests.Add(1)
		var body any
		_ = json.NewDecoder(req.Body).Decode(&body)
		close(entered)
		select {
		case <-req.Context().Done():
		case <-released:
		}
	})
	p := liveFixturePrepare(t, r)
	done := make(chan error, 1)
	go func() { _, err := p.Start(liveFixtureContext(t), liveFixtureStart()); done <- err }()
	select {
	case <-entered:
	case <-time.After(time.Second):
		t.Fatal("create did not reach fixture")
	}
	r.beginCredentialMutation()
	select {
	case err := <-done:
		var failure LiveStartError
		if !errors.As(err, &failure) || !failure.Dispatched {
			t.Fatal("lost uncertainty")
		}
	case <-time.After(time.Second):
		t.Fatal("credential rotation waited on create I/O")
	}
	if requests.Load() != 1 {
		t.Fatal("duplicate create")
	}
}
func TestLiveWireRetainsFinalIdentityAndNeverInventsHintAuthority(t *testing.T) {
	cases := []struct{ raw, kind, id string }{
		{`{"type":"session.started","session":{"id":"session-a"}}`, "ready", ""},
		{`{"type":"input_transcript.added","item":{"id":"caption-not-turn","text":"partial"}}`, "transcript_delta", ""},
		{`{"type":"turn.done","turn":{"id":"turn-a","role":"user","transcript":"final"}}`, "person_utterance_final", "turn-a"},
		{`{"type":"turn.done","turn":{"id":"turn-b","role":"assistant","transcript":"filler"}}`, "assistant_utterance_final", "turn-b"},
		{`{"type":"delegation.created","item":{"id":"delegation-a","type":"delegation","target":"client","content":[{"type":"input_text","text":"untrusted hint"}]}}`, "delegation_requested", ""},
		{`{"type":"delegation.created","item":{"id":"delegation-empty","type":"delegation","target":"client"}}`, "delegation_requested", ""},
	}
	for _, tc := range cases {
		event, err := decodeLiveEvent([]byte(tc.raw))
		if err != nil || event == nil || event.Kind != tc.kind || event.UtteranceID != tc.id || event.UtteranceRef != nil {
			t.Fatalf("wrong normalized event: %v", err)
		}
		if _, err := json.Marshal(event); err != nil {
			t.Fatal(err)
		}
	}
	for _, raw := range []string{`{"type":"turn.done","turn":{"role":"user","transcript":"no ID"}}`, `{"type":"turn.done","turn":{"id":"x","role":"tool","transcript":"not user"}}`, `{"type":"error","message":"provider-secret"}`, `{"type":"session.started","type":"turn.done"}`, `{"type":"turn.done","turn":{"id":"x","role":"user","transcript":"\ud800"}}`} {
		if _, err := decodeLiveEvent([]byte(raw)); err == nil || strings.Contains(err.Error(), "secret") {
			t.Fatal("malformed event accepted or leaked")
		}
	}
	if event, err := decodeLiveEvent([]byte(`{"type":"output_audio.delta","audio":"never-domain-data"}`)); err != nil || event != nil {
		t.Fatal("audio entered normalized journal")
	}
}
func TestLiveAppendPreflightsWholeUTF8BatchAndRejectsUnverifiedInstructionMapping(t *testing.T) {
	request := inference.LiveAppend{OperationID: fixtureOperation, Kind: "presentable_result", DelegationID: "delegation-a", Text: strings.Repeat("가🙂", 400)}
	frames, err := liveAppendFrames(request)
	if err != nil {
		t.Fatal(err)
	}
	var joined strings.Builder
	for _, raw := range frames {
		var frame struct {
			Type, Channel string
			Delegation    string `json:"delegation_item_id"`
			Content       []struct{ Text string }
		}
		if json.Unmarshal(raw, &frame) != nil || frame.Type != "delegation.context.append" || frame.Channel != "speakable" || frame.Delegation != request.DelegationID || len(frame.Content) != 1 {
			t.Fatal("wrong append frame")
		}
		part := frame.Content[0].Text
		if len(part) > 500 || !utf8.ValidString(part) {
			t.Fatal("split a UTF-8 code point")
		}
		joined.WriteString(part)
	}
	if joined.String() != request.Text || len(frames) < 2 {
		t.Fatal("append truncated or reframed text")
	}
	request.Kind = "instruction"
	if _, err := liveAppendFrames(request); !errors.Is(err, ErrRequestRejected) {
		t.Fatal("unverified instructions silently remapped")
	}
	request.Kind = "progress"
	request.Text = strings.Repeat("x", 65537)
	if frames, err := liveAppendFrames(request); err == nil || len(frames) != 0 {
		t.Fatal("invalid batch partly prepared")
	}
}

func TestLiveSidebandUsesOwnedReaderAndOneShotAppendWithoutSessionOverwrite(t *testing.T) {
	writes := make(chan []byte, 16)
	attached := make(chan struct{})
	var creates atomic.Int32
	r := liveFixtureRuntime(t, func(w http.ResponseWriter, req *http.Request) {
		if req.URL.Path == "/create" {
			creates.Add(1)
			liveFixtureAnswer(w)
			return
		}
		if req.URL.Path != "/sideband/rtc_fixture" || req.Header.Get("Authorization") != "Bearer synthetic-live-access" {
			t.Error("wrong exact sideband identity")
			w.WriteHeader(400)
			return
		}
		conn, err := websocket.Accept(w, req, nil)
		if err != nil {
			return
		}
		defer conn.CloseNow()
		close(attached)
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		for _, event := range []string{`{"type":"session.started","session":{"id":"session-a"}}`, `{"type":"turn.done","turn":{"id":"turn-a","role":"user","transcript":"synthetic final"}}`} {
			if conn.Write(ctx, websocket.MessageText, []byte(event)) != nil {
				return
			}
		}
		for {
			_, raw, err := conn.Read(ctx)
			if err != nil {
				return
			}
			writes <- raw
		}
	})
	call := liveFixtureCall(t, r)
	if err := call.Attach(); err != nil {
		t.Fatal(err)
	}
	<-attached
	ready, err := call.Next()
	if err != nil || ready.Kind != "ready" {
		t.Fatal("missing session readiness")
	}
	final, err := call.Next()
	if err != nil || final.UtteranceID != "turn-a" {
		t.Fatal("lost turn identity")
	}
	select {
	case <-writes:
		t.Fatal("sideband overwrote session initialization")
	default:
	}
	append, err := call.PrepareAppend(inference.LiveAppend{OperationID: fixtureOperation, Kind: "presentable_result", Text: strings.Repeat("가", 200)})
	if err != nil {
		t.Fatal(err)
	}
	observed, err := append.Send(liveFixtureContext(t))
	if err != nil || !observed.WriteComplete || observed.CompletedFrames != 2 {
		t.Fatal("wrong write observation")
	}
	if _, err := append.Send(liveFixtureContext(t)); !errors.Is(err, errLiveConsumed) {
		t.Fatal("append resent")
	}
	for i := 0; i < 2; i++ {
		select {
		case raw := <-writes:
			if strings.Contains(string(raw), "session.update") {
				t.Fatal("unexpected session overwrite")
			}
		case <-time.After(time.Second):
			t.Fatal("missing append")
		}
	}
	done := make(chan error, 1)
	go func() { _, err := call.Next(); done <- err }()
	call.CloseLocal()
	select {
	case err := <-done:
		if err == nil {
			t.Fatal("close pretended an event")
		}
	case <-time.After(time.Second):
		t.Fatal("owned reader survived local close")
	}
	if creates.Load() != 1 {
		t.Fatal("sideband recreated call")
	}
}

func TestLiveKnownCallIdentitySurvivesMalformedAnswerWithoutPermittingMedia(t *testing.T) {
	r := liveFixtureRuntime(t, func(w http.ResponseWriter, req *http.Request) {
		w.Header().Set("Location", "/v1/realtime/calls/rtc_known")
		w.WriteHeader(201)
		_, _ = w.Write([]byte("not SDP"))
	})
	p := liveFixturePrepare(t, r)
	call, err := p.Start(liveFixtureContext(t), liveFixtureStart())
	var failure LiveStartError
	if !errors.As(err, &failure) || !failure.Dispatched || call == nil || call.CallID() != "rtc_known" {
		t.Fatal("known call identity lost after signaling failure")
	}
	if _, err := call.AnswerSDP(); err == nil {
		t.Fatal("failed signaling exposed media answer")
	}
	if err := call.Attach(); err == nil {
		t.Fatal("failed call reactivated")
	}
	if _, err := p.Start(liveFixtureContext(t), liveFixtureStart()); !errors.Is(err, errLiveConsumed) {
		t.Fatal("known remote call recreated")
	}
}
func TestLiveSessionIdentityChangeClosesCanonicalReader(t *testing.T) {
	r := liveFixtureRuntime(t, func(w http.ResponseWriter, req *http.Request) {
		if req.URL.Path == "/create" {
			liveFixtureAnswer(w)
			return
		}
		conn, err := websocket.Accept(w, req, nil)
		if err != nil {
			return
		}
		defer conn.CloseNow()
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		defer cancel()
		for _, id := range []string{"one", "different"} {
			if conn.Write(ctx, websocket.MessageText, []byte(fmt.Sprintf(`{"type":"session.updated","session":{"id":%q}}`, id))) != nil {
				return
			}
		}
		_, _, _ = conn.Read(ctx)
	})
	call := liveFixtureCall(t, r)
	if err := call.Attach(); err != nil {
		t.Fatal(err)
	}
	if _, err := call.Next(); err != nil {
		t.Fatal(err)
	}
	if _, err := call.Next(); !errors.Is(err, invalidOutput) || call.current() {
		t.Fatal("mixed provider sessions released events")
	}
}
func TestLiveCloseWriteIsOnlyTransportEvidenceAndFencesFurtherAppends(t *testing.T) {
	frames := make(chan string, 2)
	r := liveFixtureRuntime(t, func(w http.ResponseWriter, req *http.Request) {
		if req.URL.Path == "/create" {
			liveFixtureAnswer(w)
			return
		}
		conn, err := websocket.Accept(w, req, nil)
		if err != nil {
			return
		}
		defer conn.CloseNow()
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		defer cancel()
		_, raw, err := conn.Read(ctx)
		if err == nil {
			frames <- string(raw)
		}
	})
	call := liveFixtureCall(t, r)
	if err := call.Attach(); err != nil {
		t.Fatal(err)
	}
	wrote, err := call.RequestClose(liveFixtureContext(t))
	if err != nil || !wrote || call.current() {
		t.Fatal("close write did not fence local call")
	}
	select {
	case frame := <-frames:
		if frame != `{"type":"session.close"}` {
			t.Fatal("wrong close protocol")
		}
	case <-time.After(time.Second):
		t.Fatal("missing close request")
	}
	if _, err := call.PrepareAppend(inference.LiveAppend{OperationID: fixtureOperation, Kind: "progress", Text: "late"}); err == nil {
		t.Fatal("append after close")
	}
}

func TestLiveSharedNormalizedContractFixture(t *testing.T) {
	raw, err := os.ReadFile("../../../../../fixtures/live-model/codex-normalized-events.json")
	if err != nil {
		t.Fatal(err)
	}
	var cases []struct {
		Name             string
		Wire, Normalized json.RawMessage
	}
	if json.Unmarshal(raw, &cases) != nil {
		t.Fatal("invalid fixture")
	}
	for _, tc := range cases {
		t.Run(tc.Name, func(t *testing.T) {
			event, err := decodeLiveEvent(tc.Wire)
			if err != nil {
				t.Fatal(err)
			}
			got, err := json.Marshal(event)
			if err != nil {
				t.Fatal(err)
			}
			var actual, expected any
			if json.Unmarshal(got, &actual) != nil || json.Unmarshal(tc.Normalized, &expected) != nil || !reflect.DeepEqual(actual, expected) {
				t.Fatal("normalized Rust/Go fixture drift")
			}
		})
	}
}

func TestLiveHostCapacityIsBoundedAndReleasedByLocalClose(t *testing.T) {
	var creates atomic.Int32
	r := liveFixtureRuntime(t, func(w http.ResponseWriter, req *http.Request) {
		n := creates.Add(1)
		w.Header().Set("Location", fmt.Sprintf("/v1/realtime/calls/rtc_capacity_%d", n))
		w.WriteHeader(201)
		_, _ = w.Write([]byte("v=0\r\ns=synthetic-answer\r\n"))
	})
	calls := make([]*LiveCall, 0, maxLiveCalls)
	for i := 0; i < maxLiveCalls; i++ {
		calls = append(calls, liveFixtureCall(t, r))
	}
	p := liveFixturePrepare(t, r)
	_, err := p.Start(liveFixtureContext(t), liveFixtureStart())
	var failure LiveStartError
	if !errors.Is(err, errLiveCapacity) || !errors.As(err, &failure) || failure.Dispatched || creates.Load() != maxLiveCalls {
		t.Fatal("capacity exceeded before dispatch fence")
	}
	calls[0].CloseLocal()
	replacement := liveFixtureCall(t, r)
	replacement.CloseLocal()
	if creates.Load() != maxLiveCalls+1 {
		t.Fatal("local close did not release transport slot")
	}
}

func TestLiveDuplicateRemoteIdentityDoesNotTakeOverAnotherActiveCall(t *testing.T) {
	r := liveFixtureRuntime(t, func(w http.ResponseWriter, req *http.Request) { liveFixtureAnswer(w) })
	first := liveFixtureCall(t, r)
	second, err := liveFixturePrepare(t, r).Start(liveFixtureContext(t), liveFixtureStart())
	if err == nil || second == nil || !second.closed.Load() || !first.current() {
		t.Fatal("duplicate remote identity adopted or closed unrelated call")
	}
	if _, err := second.AnswerSDP(); err == nil {
		t.Fatal("collision exposed usable media answer")
	}
}
