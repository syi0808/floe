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
			if fixture.Purposes.QuickResponse.Capabilities == nil || len(fixture.Purposes.QuickResponse.Capabilities) != 1 || fixture.Purposes.QuickResponse.Capabilities[0] != inference.ChatCapability {
				t.Fatalf("catalog metadata changed declared capabilities: %#v", fixture.Purposes.QuickResponse.Capabilities)
			}
			if profile.Sources.Catalog.ContextWindowTokens != nil && profile.ContextWindow.Status != inference.LimitUnknown && *profile.ContextWindow.Tokens == *profile.Sources.Catalog.ContextWindowTokens {
				t.Fatal("catalog limit was silently reused as the effective configured limit")
			}
		})
	}
}
