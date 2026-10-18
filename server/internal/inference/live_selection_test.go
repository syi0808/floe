package inference

import (
	"context"
	"errors"
	"floe/server/internal/trust"
	"strings"
	"testing"
)

type liveSelectionAccount struct {
	selectionTestAccount
	readyErr error
	profile  LiveProfile
}

func (a *liveSelectionAccount) Ready(context.Context) error { return a.readyErr }
func (a *liveSelectionAccount) LiveProfile() (LiveProfile, bool) {
	return CloneLiveProfile(a.profile), true
}
func (a *liveSelectionAccount) PrepareLiveTransport(context.Context, string) (PreparedLiveTransport, error) {
	panic("observation cannot prepare or dispatch")
}
func liveSelectionFixture(t *testing.T) (*Service, *liveSelectionAccount) {
	t.Helper()
	s, err := NewService(selectionTestTrust{}, emptyCapabilityMetadata())
	if err != nil {
		t.Fatal(err)
	}
	a := &liveSelectionAccount{selectionTestAccount: selectionTestAccount{model: "synthetic-live", identity: "identity"}}
	a.profile = LiveProfile{ModelIdentity: a.ModelIdentity(), Capabilities: append([]string(nil), liveCapabilityOrder...), Limits: LiveTransportLimits{100, 100, 1000}, ProtocolSource: "synthetic-live-source", Experimental: true}
	return s, a
}
func TestLiveSelectionUsesCanonicalRouteWithoutGenerationCapabilityRequirements(t *testing.T) {
	s, a := liveSelectionFixture(t)
	ctx := context.Background()
	missing, err := s.ObserveLivePurpose(ctx, trust.Principal{})
	if err != nil || missing.Status != NotConfigured {
		t.Fatal("missing route not explicit absence")
	}
	config := InferenceConfig{Routes: map[Purpose]PurposeRoute{LivePresentation: {TargetID: "live", Enabled: true}}}
	if err = s.Configure(config, map[string]ModelAccount{"live": a}, selectionTestExecutor{}); err != nil {
		t.Fatal(err)
	}
	before, err := s.ObserveLivePurpose(ctx, trust.Principal{})
	if err != nil || before.Status != Available || before.Profile == nil {
		t.Fatal("live unavailable")
	}
	before.Profile.Capabilities[0] = "changed-client-copy"
	after, _ := s.ObserveLivePurpose(ctx, trust.Principal{})
	if after.Profile.Capabilities[0] != "duplex_audio" || after.CapabilityRevision != before.CapabilityRevision {
		t.Fatal("caller mutated selected profile")
	}
	a.identity = "rotated"
	rotated, _ := s.ObserveLivePurpose(ctx, trust.Principal{})
	if rotated.CapabilityRevision == before.CapabilityRevision {
		t.Fatal("account change retained capability revision")
	}
	a.readyErr = errors.New("synthetic credential failure")
	if _, err = s.ObserveLivePurpose(ctx, trust.Principal{}); err == nil {
		t.Fatal("credential failure became absence/fallback")
	}
}
func TestLiveRouteCannotCarryGenerationEffortOrEnterGenerationCalls(t *testing.T) {
	s, a := liveSelectionFixture(t)
	config := InferenceConfig{Routes: map[Purpose]PurposeRoute{LivePresentation: {TargetID: "live", Enabled: true, ReasoningEffort: "high"}}}
	if s.Configure(config, map[string]ModelAccount{"live": a}, selectionTestExecutor{}) == nil {
		t.Fatal("live accepted unused reasoning effort")
	}
	text := `{"run_instructions":{"output_format":{"kind":"text"}}}`
	in := AgentInvocation{Purpose: QuickResponse, CapabilityRevision: strings.Repeat("a", 64), AttemptID: "11111111-1111-4111-8111-111111111111", DataClasses: []string{"synthetic"}, Instructions: "synthetic instructions", Input: AgentInput{Messages: []Message{{Role: "user", Content: &text}}, Tools: []Tool{}}, OutputFormat: OutputFormat{Kind: "text"}, MaxOutputBytes: 512}
	if err := ValidateAgentInvocation(in); err != nil {
		t.Fatal("invalid generation fixture", err)
	}
	in.Purpose = LivePresentation
	if ValidateAgentInvocation(in) == nil {
		t.Fatal("live purpose accepted as generation")
	}
	if !ValidPurpose(string(LivePresentation)) || ValidGenerationPurpose(string(LivePresentation)) {
		t.Fatal("purpose families conflated")
	}
}

func TestUnsupportedLiveTargetIsRejectedBeforeConfigurationPersistence(t *testing.T) {
	config := InferenceConfig{Routes: map[Purpose]PurposeRoute{LivePresentation: {TargetID: "generation", Enabled: true}}}
	accounts := map[string]ModelAccount{"generation": selectionTestAccount{model: "ordinary", identity: "identity"}}
	if validatePreparedConfiguration(config, accounts, selectionTestExecutor{}) == nil {
		t.Fatal("unsupported live account passed pre-commit configuration gate")
	}
}
