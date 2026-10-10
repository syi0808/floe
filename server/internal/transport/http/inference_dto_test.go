package httptransport

import (
	"os"
	"path/filepath"
	"testing"

	"floe/server/internal/inference"
)

func TestInferenceSchema3BudgetFixturesRoundTripAsClosedDTOs(t *testing.T) {
	for _, name := range []string{
		"inference-budget-schema3-unknown.json",
		"inference-budget-schema3-unavailable.json",
		"inference-budget-schema3-configured.json",
	} {
		t.Run(name, func(t *testing.T) {
			data, err := os.ReadFile(filepath.Join("..", "..", "..", "..", "testdata", name))
			if err != nil {
				t.Fatal(err)
			}
			var fixture InventoryResponseDTO
			if !decodeDTO(data, &fixture) || fixture.SchemaVersion != 3 {
				t.Fatal("schema-3 inventory fixture did not decode as a closed DTO")
			}
			profile := fixture.Purposes.QuickResponse.BudgetProfile
			if fixture.Purposes.QuickResponse.Status != string(inference.Available) || profile == nil {
				t.Fatal("fixture omitted the available purpose's required budget profile")
			}
			if err := profile.Validate(); err != nil {
				t.Fatalf("fixture profile is invalid: %v", err)
			}
			if len(fixture.Purposes.QuickResponse.Capabilities) != 0 {
				t.Fatalf("wire capabilities included chat without exact model evidence: %#v", fixture.Purposes.QuickResponse.Capabilities)
			}
			states := fixture.Purposes.QuickResponse.CapabilityStates
			if states[inference.ChatCapability].Status != string(inference.CapabilityUnknown) || states[inference.StructuredOutputCapability].Status != string(inference.CapabilityUnknown) || states[inference.ToolProposalsCapability].Status != string(inference.CapabilityUnknown) {
				t.Fatalf("wire inventory omitted unknown capability states: %#v", states)
			}
			if profile.Sources.Catalog.ContextWindowTokens != nil && profile.ContextWindow.Status != inference.LimitUnknown && *profile.ContextWindow.Tokens == *profile.Sources.Catalog.ContextWindowTokens {
				t.Fatal("catalog limit was silently reused as the effective configured limit")
			}
		})
	}
}

func TestCapabilityStateDTOPreservesProvenanceAndUnknownReason(t *testing.T) {
	states := capabilityStatesDTO(inference.CapabilityStates{
		inference.ChatCapability: {Status: inference.CapabilitySupported, Provenance: &inference.CapabilityProvenance{
			Source: "synthetic chat evidence", VerifiedAt: "2026-10-10T00:00:00Z",
		}},
		inference.StructuredOutputCapability: {Status: inference.CapabilitySupported, Provenance: &inference.CapabilityProvenance{
			Source: "synthetic DTO fixture", VerifiedAt: "2026-10-10T00:00:00Z",
		}},
		inference.ToolProposalsCapability: {Status: inference.CapabilityUnknown, Reason: "evidence_absent"},
	})
	if states[inference.ChatCapability].Status != "supported" || states[inference.ChatCapability].Provenance == nil || states[inference.ChatCapability].Provenance.Source != "synthetic chat evidence" || states[inference.StructuredOutputCapability].Provenance == nil ||
		states[inference.StructuredOutputCapability].Provenance.Source != "synthetic DTO fixture" ||
		states[inference.ToolProposalsCapability].Status != "unknown" || states[inference.ToolProposalsCapability].Reason != "evidence_absent" {
		t.Fatalf("capability state DTO lost status, provenance, or reason: %#v", states)
	}
}
