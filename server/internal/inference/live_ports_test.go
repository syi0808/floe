package inference

import (
	"fmt"
	"strings"
	"testing"
)

func TestLiveProfileKeepsLongLivedCapabilitiesSeparateAndBounded(t *testing.T) {
	p := LiveProfile{ModelIdentity: ModelIdentity{ProviderID: "codex_oauth", ModelID: "gpt-live-1-codex", Endpoint: "https://chatgpt.com/backend-api/codex"}, Capabilities: append([]string(nil), liveCapabilityOrder...), Limits: LiveTransportLimits{1, 1, 1000}, ProtocolSource: "openai/codex@fixture", Experimental: true}
	if err := p.Validate(); err != nil {
		t.Fatal(err)
	}
	for _, caps := range [][]string{{}, {"chat"}, {"duplex_audio", "duplex_audio"}, {"context_append", "duplex_audio"}} {
		q := CloneLiveProfile(p)
		q.Capabilities = caps
		if q.Validate() == nil {
			t.Fatal("invalid capabilities accepted")
		}
	}
	q := CloneLiveProfile(p)
	q.Capabilities[0] = "changed"
	if p.Capabilities[0] != "duplex_audio" {
		t.Fatal("shared capability slice")
	}
	for _, limits := range []LiveTransportLimits{{0, 1, 1}, {1, 0, 1}, {1, 1, 0}, {1, 1, 9007199254740991}} {
		if limits.Validate() == nil {
			t.Fatal("invalid effective limits accepted")
		}
	}
}
func TestLivePortValuesRedactContentAndRejectInvalidSequence(t *testing.T) {
	start := LiveStartInput{Instructions: "private-role", InitialContext: "private-context", Offer: "private-offer"}
	delivery := LiveDelivery{Sequence: 1, Event: LiveEvent{Kind: "person_utterance_final", UtteranceID: "u", Text: "private-text"}}
	if delivery.Validate() != nil {
		t.Fatal("valid delivery rejected")
	}
	for _, value := range []any{start, delivery} {
		for _, format := range []string{"%v", "%+v", "%#v"} {
			if strings.Contains(fmt.Sprintf(format, value), "private-") {
				t.Fatal("content leaked in diagnostics")
			}
		}
	}
	delivery.Sequence = 0
	if delivery.Validate() == nil {
		t.Fatal("zero sequence accepted")
	}
	delivery.Sequence = 9007199254740992
	if delivery.Validate() == nil {
		t.Fatal("unsafe wire integer accepted")
	}
}
