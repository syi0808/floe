package inference

import (
	"encoding/json"
	"errors"
	"testing"
	"unicode/utf8"
)

func tokenPointer(value uint32) *uint32 { return &value }

func TestResolveModelBudgetProfileKeepsLegacyEffectiveMinimum(t *testing.T) {
	tests := []struct {
		name       string
		configured *uint32
		want       uint32
	}{
		{name: "unconfigured preserves legacy cap", want: LegacyAgentInputBytes},
		{name: "below legacy", configured: tokenPointer(4096), want: 4096},
		{name: "at legacy", configured: tokenPointer(LegacyAgentInputBytes), want: LegacyAgentInputBytes},
		{name: "above legacy within outer frame", configured: tokenPointer(40_000), want: LegacyAgentInputBytes},
		{name: "outer framing remains the hard ceiling", configured: tokenPointer(GatewayRequestBytes), want: LegacyAgentInputBytes},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			var override *ModelBudgetOverride
			if test.configured != nil {
				override = &ModelBudgetOverride{SchemaVersion: ModelBudgetOverrideVersion, MaxInputJSONBytes: test.configured}
			}
			profile := ResolveModelBudgetProfile(override)
			if err := profile.Validate(); err != nil {
				t.Fatalf("resolved profile is invalid: %v", err)
			}
			if profile.Framing.MaxInputJSONBytes != test.want {
				t.Fatalf("effective input limit = %d, want %d", profile.Framing.MaxInputJSONBytes, test.want)
			}
			if profile.Framing.MaxInstructionBytes != 9216 || profile.Framing.MaxMessages != 256 ||
				profile.Framing.MaxTools != 64 || profile.Framing.MaxConversationBytes != 128*1024 ||
				profile.Framing.MaxRequestBytes != 98_304 || profile.Framing.MaxOutputBytes != 16_384 {
				t.Fatalf("profile enlarged an existing framing cap: %+v", profile.Framing)
			}
		})
	}
}

func TestModelBudgetOverrideRejectsContradictoryValues(t *testing.T) {
	tests := []struct {
		name     string
		override ModelBudgetOverride
	}{
		{name: "zero context", override: ModelBudgetOverride{SchemaVersion: 1, ContextWindowTokens: tokenPointer(0)}},
		{name: "output above context", override: ModelBudgetOverride{SchemaVersion: 1, ContextWindowTokens: tokenPointer(100), MaxOutputTokens: tokenPointer(101)}},
		{name: "reservation above output", override: ModelBudgetOverride{SchemaVersion: 1, MaxOutputTokens: tokenPointer(100), SelectedOutputReservationTokens: tokenPointer(101)}},
		{name: "input above outer framing", override: ModelBudgetOverride{SchemaVersion: 1, MaxInputJSONBytes: tokenPointer(GatewayRequestBytes + 1)}},
		{name: "unsupported version", override: ModelBudgetOverride{SchemaVersion: 2, ContextWindowTokens: tokenPointer(100)}},
		{name: "empty override", override: ModelBudgetOverride{SchemaVersion: 1}},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if err := test.override.Validate(); err == nil {
				t.Fatal("contradictory override was accepted")
			}
		})
	}
}

func TestProviderConfirmedLimitRequiresMatchingSourceStatus(t *testing.T) {
	profile := ResolveModelBudgetProfile(nil)
	tokens := uint32(32_000)
	profile.ContextWindow = ModelTokenLimit{Status: LimitKnown, Tokens: &tokens, Source: LimitSourceProvider}
	if err := profile.Validate(); err == nil {
		t.Fatal("provider-confirmed token fact with unknown source status was accepted")
	}
	profile.Sources.ProviderConfirmed = ProviderConfirmedAvailable
	if err := profile.Validate(); err != nil {
		t.Fatalf("paired provider-confirmed fact/status should validate: %v", err)
	}
	profile.ContextWindow = unknownTokenLimit()
	if err := profile.Validate(); err == nil {
		t.Fatal("available provider-confirmed status without a provider fact was accepted")
	}
}

func TestModelBudgetEstimatorCountsUTF8MessagesAndSchemas(t *testing.T) {
	profile := ResolveModelBudgetProfile(&ModelBudgetOverride{
		SchemaVersion:       1,
		ContextWindowTokens: tokenPointer(2000), MaxOutputTokens: tokenPointer(1000),
		SelectedOutputReservationTokens: tokenPointer(100), ProviderOverheadTokens: tokenPointer(20),
		SafetyMarginTokens: tokenPointer(30), MaxInputJSONBytes: tokenPointer(4096),
	})
	invocation := AgentInvocation{
		Instructions: "요약 지침 <&>",
		Input: AgentInput{
			Messages: []Message{{Role: "user", Content: stringPointer("서울의 날씨를 알려 주세요")}},
			Tools: []Tool{{Type: "function", Function: ToolFunction{
				Name: "weather", Description: "도구 설명", Parameters: json.RawMessage(`{"type":"object","properties":{"도시":{"type":"string"}}}`),
			}}},
		},
		OutputFormat: OutputFormat{Kind: "json", Schema: json.RawMessage(`{"type":"object","properties":{"요약":{"type":"string"}},"required":["요약"],"additionalProperties":false}`)},
	}
	encoded, err := json.Marshal(modelInputEstimate{Instructions: invocation.Instructions, Input: invocation.Input, OutputFormat: invocation.OutputFormat})
	if err != nil {
		t.Fatal(err)
	}
	if len(encoded) == utf8.RuneCount(encoded) || !containsUTF8(encoded) {
		t.Fatal("fixture must include multi-byte UTF-8 model content")
	}
	profile.ContextWindow.Tokens = tokenPointer(uint32(len(encoded) + 20 + 30 + 100))
	if err := ValidateModelBudgetInput(profile, invocation); err != nil {
		t.Fatalf("exact configured estimate boundary was rejected: %v", err)
	}
	profile.ContextWindow.Tokens = tokenPointer(uint32(len(encoded) + 20 + 30 + 99))
	var failure Failure
	if err := ValidateModelBudgetInput(profile, invocation); !errors.As(err, &failure) || failure.Code != BodyTooLarge {
		t.Fatalf("one-unit context overflow = %v, want typed body_too_large", err)
	}
}

func TestUnknownModelCapacityLeavesFramingGuardsActive(t *testing.T) {
	profile := ResolveModelBudgetProfile(nil)
	if profile.ContextWindow.Status != LimitUnknown || profile.MaxOutput.Status != LimitUnknown || profile.SelectedOutputReservation.Status != LimitUnknown {
		t.Fatal("unknown provider capacity was fabricated")
	}
	invocation := AgentInvocation{
		Instructions: "bounded instruction",
		Input:        AgentInput{Messages: []Message{{Role: "user", Content: stringPointer("small")}}, Tools: []Tool{}},
	}
	if err := ValidateModelBudgetInput(profile, invocation); err != nil {
		t.Fatalf("unknown token capacity should keep existing byte guards only: %v", err)
	}
	profile.Framing.MaxInputJSONBytes = 1
	var failure Failure
	if err := ValidateModelBudgetInput(profile, invocation); !errors.As(err, &failure) || failure.Code != BodyTooLarge {
		t.Fatalf("framing overflow = %v, want typed body_too_large", err)
	}
}

func stringPointer(value string) *string { return &value }

func containsUTF8(value []byte) bool {
	for _, byteValue := range value {
		if byteValue >= 0x80 {
			return true
		}
	}
	return false
}
