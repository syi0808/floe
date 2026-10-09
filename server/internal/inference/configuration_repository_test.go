package inference

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"

	"floe/server/internal/operation"
	"floe/server/internal/trust"
)

type configurationTestRepository struct {
	state               ConfigState
	present             bool
	health              ConfigRepositoryHealth
	loadDisposition     ConfigReadDisposition
	nextWrite           *ConfigWriteOutcome
	commitIndeterminate bool
	writes              int
}

func (repository *configurationTestRepository) LoadConfig() ConfigReadOutcome {
	if repository.loadDisposition != 0 {
		return ConfigReadOutcome{Disposition: repository.loadDisposition}
	}
	if !repository.present {
		return ConfigReadOutcome{Disposition: ConfigReadAbsent}
	}
	return ConfigReadOutcome{Disposition: ConfigReadPresent, State: repository.state}
}

func (repository *configurationTestRepository) SaveConfig(state ConfigState) ConfigWriteOutcome {
	repository.writes++
	outcome := ConfigWriteOutcome{Disposition: ConfigWriteCommitted}
	if repository.nextWrite != nil {
		outcome = *repository.nextWrite
		repository.nextWrite = nil
	}
	if outcome.Disposition == ConfigWriteCommitted || repository.commitIndeterminate && outcome.Disposition == ConfigWriteIndeterminate {
		repository.state = cloneConfigurationState(state)
		repository.present = true
	}
	return outcome
}

func (repository *configurationTestRepository) Health() ConfigRepositoryHealth {
	if repository.health == 0 {
		return ConfigRepositoryReady
	}
	return repository.health
}

type configurationTestCredentialAccess struct {
	storeErr error
	reads    int
	writes   int
}

func (access *configurationTestCredentialAccess) ReadProviderCredential(context.Context, string) (string, error) {
	access.reads++
	if access.storeErr != nil {
		return "", access.storeErr
	}
	return "synthetic-key", nil
}

func (access *configurationTestCredentialAccess) StoreProviderCredential(_ context.Context, reference, value string) error {
	access.writes++
	if !strings.HasPrefix(reference, "FLOE_KEY_") || value == "" {
		return errors.New("invalid synthetic provider credential")
	}
	return access.storeErr
}

type configurationTestFactory struct{}

func (configurationTestFactory) ValidateTarget(target ProviderTarget) error {
	if target.Provider != "openai_compatible" || target.BaseURL == "" || target.Model == "" || !ValidCapabilities(target.Capabilities) {
		return errors.New("invalid synthetic provider target")
	}
	return nil
}

func (configurationTestFactory) Open(_ context.Context, targets map[string]ProviderTarget) (map[string]ModelAccount, ModelExecutor, error) {
	accounts := make(map[string]ModelAccount, len(targets))
	for id, target := range targets {
		accounts[id] = configurationTestAccount{provider: target.Provider, model: target.Model, capabilities: append([]string(nil), target.Capabilities...)}
	}
	return accounts, selectionTestExecutor{}, nil
}

type configurationTestAccount struct {
	provider, model string
	capabilities    []string
}

func (account configurationTestAccount) Ready(context.Context) error { return nil }
func (account configurationTestAccount) ReplayIdentity() string {
	return account.provider + ":" + account.model
}
func (account configurationTestAccount) Capabilities() []string {
	return append([]string(nil), account.capabilities...)
}
func (account configurationTestAccount) ModelIdentity() ModelIdentity {
	return ModelIdentity{ProviderID: account.provider, ModelID: account.model}
}
func (configurationTestAccount) BudgetOverride() *ModelBudgetOverride { return nil }

func configurationTestState(targetID, model string) ConfigState {
	return ConfigState{
		SchemaVersion: 1,
		Targets: map[string]ProviderTarget{
			targetID: {Provider: "openai_compatible", BaseURL: "https://example.invalid", Model: model, Capabilities: []string{ChatCapability}},
		},
		Routes: map[Purpose]PurposeRoute{
			QuickResponse: {TargetID: targetID, ReasoningEffort: "medium", Enabled: true},
		},
		Providers: map[string]ProviderProfile{},
	}
}

func openConfigurationTestOwner(t *testing.T, repository ConfigRepository, credentials ProviderCredentialAccess) (*Configuration, *Service) {
	t.Helper()
	engine, err := NewService(selectionTestTrust{})
	if err != nil {
		t.Fatal(err)
	}
	owner, err := OpenConfiguration(context.Background(), repository, engine, selectionTestTrust{}, credentials, configurationTestFactory{})
	if err != nil {
		t.Fatal(err)
	}
	return owner, engine
}

func TestOpenConfigurationAbsentAndCorruptSnapshots(t *testing.T) {
	credentials := &configurationTestCredentialAccess{}
	repository := &configurationTestRepository{}
	owner, engine := openConfigurationTestOwner(t, repository, credentials)
	if len(owner.state.Routes) != 0 {
		t.Fatalf("absent config did not open as an empty first-run snapshot: %#v", owner.state)
	}
	if _, _, _, err := engine.current(context.Background(), QuickResponse); err == nil {
		t.Fatal("absent config unexpectedly selected a target")
	}

	corrupt := &configurationTestRepository{loadDisposition: ConfigReadInvalid}
	corruptEngine, err := NewService(selectionTestTrust{})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := OpenConfiguration(context.Background(), corrupt, corruptEngine, selectionTestTrust{}, credentials, configurationTestFactory{}); err == nil {
		t.Fatal("corrupt persisted config was accepted")
	}
	if corruptEngine.generation != 1 {
		t.Fatalf("failed startup partially configured the live engine: generation=%d", corruptEngine.generation)
	}
	semanticCorruption := &configurationTestRepository{present: true, state: ConfigState{SchemaVersion: 1}}
	semanticEngine, err := NewService(selectionTestTrust{})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := OpenConfiguration(context.Background(), semanticCorruption, semanticEngine, selectionTestTrust{}, credentials, configurationTestFactory{}); err == nil {
		t.Fatal("persisted config with missing semantic maps was accepted")
	}
	if semanticEngine.generation != 1 {
		t.Fatalf("semantically corrupt config partially configured the live engine: generation=%d", semanticEngine.generation)
	}
}

func TestConfigurationSaveDispositionControlsAdoptionAndReopen(t *testing.T) {
	tests := []struct {
		name                string
		disposition         ConfigWriteDisposition
		commitIndeterminate bool
		wantPoison          bool
		wantReopenTarget    string
	}{
		{name: "rejected", disposition: ConfigWriteRejected, wantReopenTarget: "target-a"},
		{name: "indeterminate", disposition: ConfigWriteIndeterminate, commitIndeterminate: true, wantPoison: true, wantReopenTarget: "target-b"},
		{name: "integrity", disposition: ConfigWriteIntegrityFailure, wantPoison: true, wantReopenTarget: "target-a"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			repository := &configurationTestRepository{state: configurationTestState("target-a", "model-a"), present: true, commitIndeterminate: test.commitIndeterminate}
			credentials := &configurationTestCredentialAccess{}
			owner, engine := openConfigurationTestOwner(t, repository, credentials)
			next := cloneConfigurationState(owner.state)
			next.Targets["target-b"] = ProviderTarget{Provider: "openai_compatible", BaseURL: "https://example.invalid", Model: "model-b", Capabilities: []string{ChatCapability}}
			next.Routes[QuickResponse] = PurposeRoute{TargetID: "target-b", ReasoningEffort: "high", Enabled: true}
			repository.nextWrite = &ConfigWriteOutcome{Disposition: test.disposition, Cause: errors.New("synthetic repository outcome")}
			result := owner.commitConfiguration(context.Background(), next)
			if result.Category != operation.Unavailable {
				t.Fatalf("save failure did not return unavailable: %#v", result)
			}
			if owner.state.Routes[QuickResponse].TargetID != "target-a" {
				t.Fatalf("owner adopted a snapshot without confirmed commit: %#v", owner.state.Routes)
			}
			if test.wantPoison != owner.configUnavailable {
				t.Fatalf("config availability=%v, want poisoned=%v", owner.configUnavailable, test.wantPoison)
			}
			live, _, _, liveErr := engine.current(context.Background(), QuickResponse)
			if test.wantPoison {
				if liveErr == nil {
					t.Fatalf("%s save left the live engine available on target %q", test.name, live.TargetID())
				}
			} else if liveErr != nil || live.TargetID() != "target-a" {
				t.Fatalf("known rejection partially adopted new live state: target=%q err=%v", live.TargetID(), liveErr)
			}
			if test.name == "rejected" && owner.RequiredError() != nil {
				t.Fatalf("known rejection incorrectly poisoned required persistence: %v", owner.RequiredError())
			}
			if test.wantPoison && owner.RequiredError() == nil {
				t.Fatal("uncertain or integrity failure remained available")
			}

			reopened, reopenedEngine := openConfigurationTestOwner(t, repository, credentials)
			reopenedTarget, _, _, reopenErr := reopenedEngine.current(context.Background(), QuickResponse)
			if reopenErr != nil || reopenedTarget.TargetID() != test.wantReopenTarget || reopened.state.Routes[QuickResponse].TargetID != test.wantReopenTarget {
				t.Fatalf("reopen did not adopt only the durable complete snapshot: state=%q live=%q err=%v", reopened.state.Routes[QuickResponse].TargetID, reopenedTarget.TargetID(), reopenErr)
			}
		})
	}
}

func TestConfigurationCredentialUnavailableDoesNotCommitOrExposeSecret(t *testing.T) {
	repository := &configurationTestRepository{state: configurationTestState("target-a", "model-a"), present: true}
	credentials := &configurationTestCredentialAccess{storeErr: errors.New("synthetic credential store unavailable")}
	owner, engine := openConfigurationTestOwner(t, repository, credentials)
	result := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, TargetUpdate{
		ID: "target-b", Provider: "openai_compatible", BaseURL: "https://example.invalid", Model: "model-b",
		APIKey: "synthetic-key", Capabilities: []string{ChatCapability},
	})
	if result.Category != operation.Unavailable || result.Code != "credential_store_unavailable" {
		t.Fatalf("credential unavailability was not returned as a safe rejection: %#v", result)
	}
	if credentials.writes != 1 || repository.writes != 0 || owner.state.Targets["target-b"].Model != "" {
		t.Fatalf("configuration changed after a failed credential write: writes=%d repo=%d state=%#v", credentials.writes, repository.writes, owner.state)
	}
	encoded, err := jsonMarshalConfig(owner.state)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(encoded, "synthetic-key") {
		t.Fatal("provider secret appeared in the persisted-state representation")
	}
	live, _, _, err := engine.current(context.Background(), QuickResponse)
	if err != nil || live.TargetID() != "target-a" {
		t.Fatalf("credential failure changed the live engine: target=%q err=%v", live.TargetID(), err)
	}
}

func jsonMarshalConfig(state ConfigState) (string, error) {
	data, err := json.Marshal(state)
	return string(data), err
}

func TestCloneConfigurationStateDeepCopiesBoundedCollections(t *testing.T) {
	maxInputBytes := uint32(4096)
	wantMaxInputBytes := maxInputBytes
	state := configurationTestState("target-a", "model-a")
	target := state.Targets["target-a"]
	target.Capabilities = []string{ChatCapability}
	target.BudgetOverride = &ModelBudgetOverride{SchemaVersion: ModelBudgetOverrideVersion, MaxInputJSONBytes: &maxInputBytes}
	state.Targets["target-a"] = target
	state.Providers["openai_compatible"] = ProviderProfile{
		BaseURL: "https://example.invalid", APIKeyEnv: "FLOE_KEY_SYNTHETIC",
		Purposes: map[string]PurposeModel{"quick_response": {Model: "model-a", Capabilities: []string{ChatCapability}}},
	}
	cloned := cloneConfigurationState(state)
	state.Routes[QuickResponse] = PurposeRoute{TargetID: "changed", Enabled: true}
	target = state.Targets["target-a"]
	target.Capabilities[0] = "changed"
	*target.BudgetOverride.MaxInputJSONBytes = 1
	state.Targets["target-a"] = target
	state.Providers["openai_compatible"].Purposes["quick_response"] = PurposeModel{Model: "changed"}
	if cloned.Routes[QuickResponse].TargetID != "target-a" || cloned.Targets["target-a"].Capabilities[0] != ChatCapability ||
		*cloned.Targets["target-a"].BudgetOverride.MaxInputJSONBytes != wantMaxInputBytes ||
		cloned.Providers["openai_compatible"].Purposes["quick_response"].Model != "model-a" {
		t.Fatalf("configuration clone shared mutable state: %#v", cloned)
	}
}
