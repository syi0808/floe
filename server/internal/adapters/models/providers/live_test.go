package providers

import (
	"context"
	"encoding/json"
	"errors"
	codexauth "floe/server/internal/adapters/models/codex"
	"floe/server/internal/inference"
	"fmt"
	"strings"
	"testing"
)

type liveCodexStub struct {
	calls      int
	identity   string
	prepareErr error
	rotate     bool
}

func (s *liveCodexStub) Ready(context.Context) bool { return true }
func (s *liveCodexStub) ReplayIdentity() string     { return s.identity }
func (s *liveCodexStub) Generate(context.Context, string, string, string, json.RawMessage, json.RawMessage) (string, error) {
	panic("live must not generate")
}
func (s *liveCodexStub) GenerateAgent(context.Context, string, string, string, json.RawMessage, json.RawMessage) (string, inference.UsageObservation, error) {
	panic("live must not invoke Manager")
}
func (s *liveCodexStub) PrepareLive(_ context.Context, expected string) (*codexauth.LivePrepared, error) {
	s.calls++
	if expected != s.identity {
		return nil, errors.New("wrong identity")
	}
	if s.prepareErr != nil {
		return nil, s.prepareErr
	}
	if s.rotate {
		s.identity = "changed"
	}
	return &codexauth.LivePrepared{}, nil
}
func TestConfiguredLiveTargetUsesExistingCredentialOwnerAndExactModel(t *testing.T) {
	stub := &liveCodexStub{identity: "synthetic-account"}
	p, err := newProvider(context.Background(), inference.ProviderTarget{Provider: "codex_oauth", Model: "gpt-live-1-codex", BaseURL: "https://chatgpt.com/backend-api/codex"}, nil, stub)
	if err != nil {
		t.Fatal(err)
	}
	profile, ok := p.LiveProfile()
	if !ok || profile.Validate() != nil || !profile.Experimental {
		t.Fatal("missing experimental live profile")
	}
	if _, err = p.PrepareLiveTransport(context.Background(), "wrong"); err == nil || stub.calls != 0 {
		t.Fatal("changed selection reached credential owner")
	}
	prepared, err := p.PrepareLiveTransport(context.Background(), p.ReplayIdentity())
	if err != nil || prepared == nil || stub.calls != 1 {
		t.Fatal("same credential owner not used")
	}
	input := inference.LiveStartInput{Instructions: "role", InitialContext: "context", Offer: "v=0\r\ns=fixture\r\n"}
	if err = prepared.ValidateStart(input); err != nil {
		t.Fatal(err)
	}
	input.Offer = "invalid"
	if prepared.ValidateStart(input) == nil {
		t.Fatal("invalid SDP passed pure preflight")
	}
	if _, err = prepared.Start(context.Background(), input); err == nil {
		t.Fatal("invalid input or unbounded lifetime accepted")
	}
	for _, value := range []any{prepared, &liveTransport{}, &liveWrite{}} {
		if strings.Contains(fmt.Sprintf("%#v", value), "transport:") || !strings.Contains(fmt.Sprintf("%#v", value), "redacted") {
			t.Fatal("adapter diagnostics not redacted")
		}
	}
	p.target.Model = "ordinary-generation-model"
	if _, ok = p.LiveProfile(); ok {
		t.Fatal("generation target gained live capability")
	}
	if _, err = p.PrepareLiveTransport(context.Background(), p.ReplayIdentity()); err == nil || stub.calls != 1 {
		t.Fatal("unsupported target used hidden live route")
	}
}
func TestLivePreparationRechecksIdentityAfterCredentialAccess(t *testing.T) {
	stub := &liveCodexStub{identity: "synthetic-account", rotate: true}
	p, _ := newProvider(context.Background(), inference.ProviderTarget{Provider: "codex_oauth", Model: "gpt-live-1-codex", BaseURL: "https://chatgpt.com/backend-api/codex"}, nil, stub)
	if _, err := p.PrepareLiveTransport(context.Background(), p.ReplayIdentity()); err == nil {
		t.Fatal("rotated identity escaped preparation fence")
	}
}
