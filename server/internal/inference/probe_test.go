package inference

import "testing"

func TestProbeAdmissionDoesNotEstablishProductCapability(t *testing.T) {
	text := "synthetic"
	request := AgentInvocation{Input: AgentInput{Messages: []Message{{Role: "user", Content: &text}}}, OutputFormat: OutputFormat{Kind: "text"}}
	product := ResolvedModelTarget{capabilityStates: CapabilityStates{ChatCapability: {Status: CapabilityUnknown}}}
	if product.AllowsAgentInvocation(request) {
		t.Fatal("unknown product chat was admitted")
	}
	digest, err := probeInvocationDigest(request)
	if err != nil {
		t.Fatal(err)
	}
	probe := ResolvedModelTarget{operatorProbeDigest: digest}
	if !probe.AllowsAgentInvocation(request) {
		t.Fatal("exact admitted diagnostic rejected")
	}
	if probe.SupportsStructuredOutput() {
		t.Fatal("diagnostic established structured-output capability")
	}
	request.Purpose = "changed"
	if probe.AllowsAgentInvocation(request) {
		t.Fatal("diagnostic admitted changed purpose")
	}
	if product.AllowsAgentInvocation(request) {
		t.Fatal("diagnostic changed product admission")
	}
	product.capabilityStates[ChatCapability] = CapabilityStateView{Status: CapabilitySupported}
	if !product.AllowsAgentInvocation(request) {
		t.Fatal("positive product chat evidence ignored")
	}
}
