package inference

import (
	"context"
	"encoding/json"
	"errors"
	"floe/server/internal/trust"
	"sync"
	"sync/atomic"
	"testing"
)

type testCapabilityMetadata struct {
	mu       sync.RWMutex
	snapshot CapabilityEvidenceSnapshot
	err      error
	calls    atomic.Int32
}

func emptyCapabilityMetadata() *testCapabilityMetadata {
	return &testCapabilityMetadata{snapshot: CapabilityEvidenceSnapshot{ContractVersion: CapabilityEvidenceContractVersion, Entries: []CapabilityEvidenceEntry{}}}
}

func (source *testCapabilityMetadata) Snapshot(ctx context.Context) (CapabilityEvidenceSnapshot, error) {
	source.calls.Add(1)
	if err := ctx.Err(); err != nil {
		return CapabilityEvidenceSnapshot{}, err
	}
	source.mu.RLock()
	defer source.mu.RUnlock()
	copy := CapabilityEvidenceSnapshot{ContractVersion: source.snapshot.ContractVersion, Entries: append([]CapabilityEvidenceEntry(nil), source.snapshot.Entries...)}
	for i := range copy.Entries {
		copy.Entries[i].Facts = append([]CapabilityEvidenceFact(nil), copy.Entries[i].Facts...)
	}
	return copy, source.err
}

func (source *testCapabilityMetadata) callCount() int32 { return source.calls.Load() }

func (source *testCapabilityMetadata) replace(snapshot CapabilityEvidenceSnapshot) {
	source.mu.Lock()
	source.snapshot = snapshot
	source.err = nil
	source.mu.Unlock()
}

func testEvidence(provider, model, endpoint, capability string, status CapabilityStatus) CapabilityEvidenceEntry {
	return CapabilityEvidenceEntry{
		ProviderID: provider,
		ModelID:    model,
		Endpoint:   endpoint,
		Facts: []CapabilityEvidenceFact{{
			Capability: capability,
			Status:     status,
			Provenance: CapabilityProvenance{Source: "synthetic-test-fixture", VerifiedAt: "2026-10-10T00:00:00Z"},
		}},
	}
}

func TestCapabilityEvidenceResolvesExactIdentityAndThreeStates(t *testing.T) {
	official := testEvidence("openai_compatible", "model-a", "https://official.example/v1", ChatCapability, CapabilitySupported)
	official.Facts = append(official.Facts, testEvidence("openai_compatible", "model-a", "https://official.example/v1", StructuredOutputCapability, CapabilitySupported).Facts...)
	snapshot := CapabilityEvidenceSnapshot{
		ContractVersion: CapabilityEvidenceContractVersion,
		Entries: []CapabilityEvidenceEntry{
			official,
			testEvidence("openai_compatible", "model-a", "https://custom.example/v1", ChatCapability, CapabilityUnsupported),
		},
	}
	protocols := []string{ChatCapability, StructuredOutputCapability, ToolProposalsCapability}
	identity := ModelIdentity{ProviderID: "openai_compatible", ModelID: "model-a", Endpoint: "https://official.example/v1"}
	states := resolveCapabilityStates(identity, protocols, snapshot, nil)
	if states[ChatCapability].Status != CapabilitySupported || states[StructuredOutputCapability].Status != CapabilitySupported || states[ToolProposalsCapability].Status != CapabilityUnknown {
		t.Fatalf("unexpected exact-identity states: %#v", states)
	}
	if states[StructuredOutputCapability].Provenance == nil || states[StructuredOutputCapability].Provenance.Source != "synthetic-test-fixture" {
		t.Fatalf("support provenance missing: %#v", states[StructuredOutputCapability])
	}

	identity.Endpoint = "https://custom.example/v1"
	states = resolveCapabilityStates(identity, protocols, snapshot, nil)
	if states[ChatCapability].Status != CapabilityUnsupported || states[StructuredOutputCapability].Status != CapabilityUnknown {
		t.Fatalf("custom endpoint did not get its exact unsupported fact: %#v", states)
	}

	identity.Endpoint = "https://unlisted.example/v1"
	states = resolveCapabilityStates(identity, protocols, snapshot, nil)
	if states[ChatCapability].Status != CapabilityUnknown || states[StructuredOutputCapability].Status != CapabilityUnknown || states[ToolProposalsCapability].Status != CapabilityUnknown {
		t.Fatalf("missing endpoint evidence was not unknown: %#v", states)
	}
}

func TestCanonicalModelEndpointPreservesEscapedPathSemantics(t *testing.T) {
	canonical, ok := CanonicalModelEndpoint("HTTPS://EXAMPLE.INVALID:443/v1/")
	if !ok || canonical != "https://example.invalid/v1" {
		t.Fatalf("endpoint spelling did not canonicalize consistently: %q ok=%v", canonical, ok)
	}
	if same, ok := CanonicalModelEndpoint("https://example.invalid/v1"); !ok || same != canonical {
		t.Fatalf("canonical endpoint did not round trip: %q ok=%v", same, ok)
	}
	canonicalPairs := []struct {
		name string
		a    string
		b    string
		want bool
	}{
		{name: "encoded slash versus path separator", a: "https://example.invalid/tenant%2Fmodel", b: "https://example.invalid/tenant/model"},
		{name: "escaped percent versus escaped slash", a: "https://example.invalid/tenant%252Fmodel", b: "https://example.invalid/tenant%2Fmodel"},
		{name: "path case", a: "https://example.invalid/Tenant/Model", b: "https://example.invalid/tenant/model"},
		{name: "base path", a: "https://example.invalid/api/v1", b: "https://example.invalid/v1"},
		{name: "provider-trimmed trailing slash", a: "https://example.invalid/api/v1/", b: "https://example.invalid/api/v1", want: true},
	}
	for _, pair := range canonicalPairs {
		t.Run(pair.name, func(t *testing.T) {
			first, firstOK := CanonicalModelEndpoint(pair.a)
			second, secondOK := CanonicalModelEndpoint(pair.b)
			if !firstOK || !secondOK || (first == second) != pair.want {
				t.Fatalf("endpoint identities collapsed or differed unexpectedly: %q=%q (%v), %q=%q (%v)", pair.a, first, firstOK, pair.b, second, secondOK)
			}
		})
	}

	for _, endpoint := range []string{
		"https://user:secret@example.invalid/v1",
		"https://example.invalid/v1?tenant=one",
		"https://example.invalid/v1?",
		"https://example.invalid/v1#fragment",
		"https://example.invalid/v1#",
	} {
		t.Run(endpoint, func(t *testing.T) {
			if canonical, ok := CanonicalModelEndpoint(endpoint); ok {
				t.Fatalf("unsafe or ambiguous endpoint was accepted as %q", canonical)
			}
		})
	}
}

func TestChatCapabilityRequiresExactEvidenceAndAdapterSupport(t *testing.T) {
	identity := ModelIdentity{ProviderID: "openai_compatible", ModelID: "model-a", Endpoint: "https://example.invalid/v1"}
	protocols := []string{ChatCapability, StructuredOutputCapability, ToolProposalsCapability}
	textCall := AgentInvocation{OutputFormat: OutputFormat{Kind: "text"}, Input: AgentInput{Tools: []Tool{}}}
	empty := CapabilityEvidenceSnapshot{ContractVersion: CapabilityEvidenceContractVersion, Entries: []CapabilityEvidenceEntry{}}
	states := resolveCapabilityStates(identity, protocols, empty, nil)
	if states[ChatCapability].Status != CapabilityUnknown || states[ChatCapability].Reason != "evidence_absent" || supportsAgent(states, textCall) {
		t.Fatalf("missing chat evidence was treated as supported: %#v", states)
	}

	for _, status := range []CapabilityStatus{CapabilitySupported, CapabilityUnsupported} {
		snapshot := CapabilityEvidenceSnapshot{ContractVersion: CapabilityEvidenceContractVersion, Entries: []CapabilityEvidenceEntry{testEvidence(identity.ProviderID, identity.ModelID, identity.Endpoint, ChatCapability, status)}}
		states = resolveCapabilityStates(identity, protocols, snapshot, nil)
		if states[ChatCapability].Status != status || supportsAgent(states, textCall) != (status == CapabilitySupported) {
			t.Fatalf("explicit chat evidence was not honored for %q: %#v", status, states)
		}
	}

	chatEvidence := CapabilityEvidenceSnapshot{ContractVersion: CapabilityEvidenceContractVersion, Entries: []CapabilityEvidenceEntry{testEvidence(identity.ProviderID, identity.ModelID, identity.Endpoint, ChatCapability, CapabilitySupported)}}
	states = resolveCapabilityStates(identity, []string{StructuredOutputCapability, ToolProposalsCapability}, chatEvidence, nil)
	if states[ChatCapability].Status != CapabilityUnsupported || states[ChatCapability].Reason != "adapter_protocol_unsupported" || supportsAgent(states, textCall) {
		t.Fatalf("model evidence exceeded the adapter protocol upper bound: %#v", states)
	}

	jsonOnly := CapabilityStates{
		ChatCapability:             {Status: CapabilityUnknown},
		StructuredOutputCapability: {Status: CapabilitySupported},
		ToolProposalsCapability:    {Status: CapabilityUnknown},
	}
	if supportsStructured(jsonOnly) {
		t.Fatal("structured output was admitted without positive chat evidence")
	}
	jsonOnly[ChatCapability] = CapabilityStateView{Status: CapabilitySupported}
	if !supportsStructured(jsonOnly) {
		t.Fatal("chat plus structured-output evidence did not admit structured output")
	}
}

func TestCapabilityProtocolIsNecessaryButDoesNotProveModelSupport(t *testing.T) {
	identity := ModelIdentity{ProviderID: "openai_compatible", ModelID: "model-a", Endpoint: "https://custom.example/v1"}
	chat := testEvidence("openai_compatible", "model-a", identity.Endpoint, ChatCapability, CapabilitySupported)
	tool := testEvidence("openai_compatible", "model-a", identity.Endpoint, ToolProposalsCapability, CapabilitySupported)
	chat.Facts = append(chat.Facts, tool.Facts...)
	snapshot := CapabilityEvidenceSnapshot{
		ContractVersion: CapabilityEvidenceContractVersion,
		Entries:         []CapabilityEvidenceEntry{chat},
	}
	states := resolveCapabilityStates(identity, []string{ChatCapability}, snapshot, nil)
	if states[ToolProposalsCapability].Status != CapabilityUnsupported || states[ToolProposalsCapability].Reason != "adapter_protocol_unsupported" {
		t.Fatalf("metadata overrode an unsupported adapter protocol: %#v", states)
	}
	empty := testEvidence("openai_compatible", "model-a", identity.Endpoint, ChatCapability, CapabilitySupported)
	states = resolveCapabilityStates(identity, []string{ChatCapability, StructuredOutputCapability, ToolProposalsCapability}, CapabilityEvidenceSnapshot{ContractVersion: 1, Entries: []CapabilityEvidenceEntry{empty}}, nil)
	if states[ChatCapability].Status != CapabilitySupported || states[ToolProposalsCapability].Status != CapabilityUnknown || supportsAgent(states, AgentInvocation{OutputFormat: OutputFormat{Kind: "text"}, Input: AgentInput{Tools: []Tool{{}}}}) {
		t.Fatalf("unknown tool support was treated as proven: %#v", states)
	}
}

func TestCapabilityRevisionTracksEffectiveEvidenceButNotCosmeticProvenance(t *testing.T) {
	metadata := emptyCapabilityMetadata()
	service, err := NewService(selectionTestTrust{}, metadata)
	if err != nil {
		t.Fatal(err)
	}
	config := InferenceConfig{Routes: map[Purpose]PurposeRoute{QuickResponse: {TargetID: "selected", Enabled: true}}}
	account := selectionTestAccount{model: "model-a", identity: "account-a"}
	if err := service.Configure(config, map[string]ModelAccount{"selected": account}, selectionTestExecutor{}); err != nil {
		t.Fatal(err)
	}
	_, unknownRevision, _, err := service.current(context.Background(), QuickResponse)
	if err != nil {
		t.Fatal(err)
	}
	metadata.replace(CapabilityEvidenceSnapshot{ContractVersion: 1, Entries: []CapabilityEvidenceEntry{testEvidence("openai_compatible", "model-a", "https://example.invalid", StructuredOutputCapability, CapabilitySupported)}})
	_, supportedRevision, _, err := service.current(context.Background(), QuickResponse)
	if err != nil {
		t.Fatal(err)
	}
	if unknownRevision == supportedRevision {
		t.Fatal("effective unknown-to-supported metadata change retained the capability revision")
	}

	changedProvenance := CapabilityEvidenceEntry{
		ProviderID: "openai_compatible", ModelID: "model-a", Endpoint: "https://example.invalid",
		Facts: []CapabilityEvidenceFact{{Capability: StructuredOutputCapability, Status: CapabilitySupported, Provenance: CapabilityProvenance{Source: "new synthetic source", VerifiedAt: "2026-10-10T00:00:01Z"}}},
	}
	metadata.replace(CapabilityEvidenceSnapshot{ContractVersion: 1, Entries: []CapabilityEvidenceEntry{changedProvenance}})
	_, cosmeticRevision, _, err := service.current(context.Background(), QuickResponse)
	if err != nil {
		t.Fatal(err)
	}
	if cosmeticRevision != supportedRevision {
		t.Fatal("provenance-only edit changed the effective capability commitment")
	}
}

func TestCapabilityRevisionTracksChatEvidence(t *testing.T) {
	metadata := emptyCapabilityMetadata()
	service, err := NewService(selectionTestTrust{}, metadata)
	if err != nil {
		t.Fatal(err)
	}
	config := InferenceConfig{Routes: map[Purpose]PurposeRoute{QuickResponse: {TargetID: "selected", Enabled: true}}}
	if err := service.Configure(config, map[string]ModelAccount{"selected": selectionTestAccount{model: "model-a", identity: "account-a"}}, selectionTestExecutor{}); err != nil {
		t.Fatal(err)
	}
	_, unknownRevision, _, err := service.current(context.Background(), QuickResponse)
	if err != nil {
		t.Fatal(err)
	}
	metadata.replace(CapabilityEvidenceSnapshot{ContractVersion: 1, Entries: []CapabilityEvidenceEntry{
		testEvidence("openai_compatible", "model-a", "https://example.invalid", ChatCapability, CapabilitySupported),
	}})
	_, supportedRevision, _, err := service.current(context.Background(), QuickResponse)
	if err != nil {
		t.Fatal(err)
	}
	if unknownRevision == supportedRevision {
		t.Fatal("effective unknown-to-supported chat evidence retained the capability revision")
	}
}

func TestCapabilityResolutionAndRevisionUseOneMetadataSnapshot(t *testing.T) {
	metadata := emptyCapabilityMetadata()
	service, err := NewService(selectionTestTrust{}, metadata)
	if err != nil {
		t.Fatal(err)
	}
	config := InferenceConfig{Routes: map[Purpose]PurposeRoute{QuickResponse: {TargetID: "selected", Enabled: true}}}
	if err := service.Configure(config, map[string]ModelAccount{"selected": selectionTestAccount{model: "model-a", identity: "account-a"}}, selectionTestExecutor{}); err != nil {
		t.Fatal(err)
	}
	before := metadata.callCount()
	target, revision, _, err := service.current(context.Background(), QuickResponse)
	if err != nil {
		t.Fatal(err)
	}
	if metadata.callCount()-before != 1 || revision == "" || target.capabilityStates[StructuredOutputCapability].Status != CapabilityUnknown {
		t.Fatalf("support and revision were not resolved from one snapshot: calls=%d revision=%q states=%#v", metadata.callCount()-before, revision, target.capabilityStates)
	}
}

type countingCapabilityExecutor struct{ calls atomic.Int32 }

func (executor *countingCapabilityExecutor) InvokeAgent(context.Context, ResolvedModelTarget, AgentInvocation) (AgentResult, error) {
	executor.calls.Add(1)
	return AgentResult{}, nil
}

func (executor *countingCapabilityExecutor) InvokeStructured(context.Context, ResolvedModelTarget, StructuredInvocation) (StructuredResult, error) {
	executor.calls.Add(1)
	return StructuredResult{}, nil
}

func TestEffectiveCapabilityDriftFailsBeforeProviderDispatch(t *testing.T) {
	metadata := emptyCapabilityMetadata()
	entry := testEvidence("openai_compatible", "model-a", "https://example.invalid", StructuredOutputCapability, CapabilitySupported)
	entry.Facts = append(entry.Facts, testEvidence("openai_compatible", "model-a", "https://example.invalid", ToolProposalsCapability, CapabilitySupported).Facts...)
	metadata.replace(CapabilityEvidenceSnapshot{ContractVersion: 1, Entries: []CapabilityEvidenceEntry{entry}})
	executor := &countingCapabilityExecutor{}
	service, err := NewService(selectionTestTrust{}, metadata)
	if err != nil {
		t.Fatal(err)
	}
	if err = service.Configure(InferenceConfig{Routes: map[Purpose]PurposeRoute{QuickResponse: {TargetID: "selected", Enabled: true}}}, map[string]ModelAccount{
		"selected": selectionTestAccount{model: "model-a", identity: "account-a"},
	}, executor); err != nil {
		t.Fatal(err)
	}
	_, pinnedRevision, _, err := service.current(context.Background(), QuickResponse)
	if err != nil {
		t.Fatal(err)
	}
	changed := testEvidence("openai_compatible", "model-a", "https://example.invalid", StructuredOutputCapability, CapabilityUnsupported)
	metadata.replace(CapabilityEvidenceSnapshot{ContractVersion: 1, Entries: []CapabilityEvidenceEntry{changed}})
	schema := json.RawMessage(`{"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false}`)
	frame := `{"run_instructions":{"output_format":{"kind":"json","schema":{"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false}}}}`
	input := AgentInvocation{
		Purpose: QuickResponse, CapabilityRevision: pinnedRevision, AttemptID: trust.NewID(),
		DataClasses: []string{"synthetic"}, Instructions: "Return the requested JSON object.",
		Input:        AgentInput{Messages: []Message{{Role: "user", Content: &frame}}, Tools: []Tool{}},
		OutputFormat: OutputFormat{Kind: "json", Schema: schema}, MaxOutputBytes: 1024,
	}
	_, err = service.InvokeAgent(context.Background(), trust.Principal{}, input)
	var failure Failure
	if !errors.As(err, &failure) || failure.Code != CapabilityChanged || failure.Dispatched || executor.calls.Load() != 0 {
		t.Fatalf("metadata drift did not fail before dispatch: err=%v calls=%d", err, executor.calls.Load())
	}
}

func TestCapabilityMetadataContractRejectsUnknownAndConflictingAssertions(t *testing.T) {
	identity := ModelIdentity{ProviderID: "openai_compatible", ModelID: "model-a", Endpoint: "https://example.invalid"}
	protocols := []string{ChatCapability, StructuredOutputCapability, ToolProposalsCapability}
	for name, facts := range map[string][]CapabilityEvidenceFact{
		"unknown cannot be asserted": {{Capability: StructuredOutputCapability, Status: CapabilityUnknown, Provenance: CapabilityProvenance{Source: "fixture", VerifiedAt: "2026-10-10T00:00:00Z"}}},
		"duplicate fact":             {{Capability: StructuredOutputCapability, Status: CapabilitySupported, Provenance: CapabilityProvenance{Source: "fixture", VerifiedAt: "2026-10-10T00:00:00Z"}}, {Capability: StructuredOutputCapability, Status: CapabilityUnsupported, Provenance: CapabilityProvenance{Source: "fixture", VerifiedAt: "2026-10-10T00:00:00Z"}}},
	} {
		t.Run(name, func(t *testing.T) {
			snapshot := CapabilityEvidenceSnapshot{ContractVersion: 1, Entries: []CapabilityEvidenceEntry{{ProviderID: identity.ProviderID, ModelID: identity.ModelID, Endpoint: identity.Endpoint, Facts: facts}}}
			states := resolveCapabilityStates(identity, protocols, snapshot, nil)
			if states[StructuredOutputCapability].Status != CapabilityUnknown || states[StructuredOutputCapability].Reason != "metadata_unavailable" {
				t.Fatalf("invalid evidence was not failed closed: %#v", states)
			}
		})
	}
}

func TestCapabilityMetadataContractRejectsDuplicateEntriesAndMalformedEvidence(t *testing.T) {
	entry := testEvidence("openai_compatible", "model-a", "https://example.invalid/v1", StructuredOutputCapability, CapabilitySupported)
	base := CapabilityEvidenceSnapshot{ContractVersion: 1, Entries: []CapabilityEvidenceEntry{entry}}
	malformed := map[string]CapabilityEvidenceSnapshot{
		"duplicate exact identity":  {ContractVersion: 1, Entries: []CapabilityEvidenceEntry{entry, entry}},
		"invalid provider identity": {ContractVersion: 1, Entries: []CapabilityEvidenceEntry{{ProviderID: "", ModelID: "model-a", Endpoint: entry.Endpoint, Facts: entry.Facts}}},
		"noncanonical endpoint":     {ContractVersion: 1, Entries: []CapabilityEvidenceEntry{{ProviderID: entry.ProviderID, ModelID: entry.ModelID, Endpoint: "https://EXAMPLE.invalid/v1/", Facts: entry.Facts}}},
		"empty query marker":        {ContractVersion: 1, Entries: []CapabilityEvidenceEntry{{ProviderID: entry.ProviderID, ModelID: entry.ModelID, Endpoint: "https://example.invalid/v1?", Facts: entry.Facts}}},
		"fragment marker":           {ContractVersion: 1, Entries: []CapabilityEvidenceEntry{{ProviderID: entry.ProviderID, ModelID: entry.ModelID, Endpoint: "https://example.invalid/v1#", Facts: entry.Facts}}},
		"malformed provenance":      {ContractVersion: 1, Entries: []CapabilityEvidenceEntry{{ProviderID: entry.ProviderID, ModelID: entry.ModelID, Endpoint: entry.Endpoint, Facts: []CapabilityEvidenceFact{{Capability: StructuredOutputCapability, Status: CapabilitySupported, Provenance: CapabilityProvenance{Source: "fixture", VerifiedAt: "not-a-timestamp"}}}}}},
	}
	identity := ModelIdentity{ProviderID: entry.ProviderID, ModelID: entry.ModelID, Endpoint: entry.Endpoint}
	protocols := []string{ChatCapability, StructuredOutputCapability, ToolProposalsCapability}
	if !validEvidenceSnapshot(base) {
		t.Fatal("synthetic valid evidence fixture failed validation")
	}
	for name, snapshot := range malformed {
		t.Run(name, func(t *testing.T) {
			if validEvidenceSnapshot(snapshot) {
				t.Fatal("malformed evidence was accepted")
			}
			states := resolveCapabilityStates(identity, protocols, snapshot, nil)
			if states[StructuredOutputCapability].Status != CapabilityUnknown || states[StructuredOutputCapability].Reason != "metadata_unavailable" {
				t.Fatalf("malformed evidence did not fail closed to unknown: %#v", states)
			}
		})
	}
}
