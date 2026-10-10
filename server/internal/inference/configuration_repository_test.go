package inference

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"sync"
	"testing"
	"time"

	"floe/server/internal/operation"
	"floe/server/internal/trust"
)

type configurationTestRepository struct {
	mu                  sync.Mutex
	state               ConfigState
	present             bool
	health              ConfigRepositoryHealth
	loadDisposition     ConfigReadDisposition
	nextWrite           *ConfigWriteOutcome
	writeSequence       []ConfigWriteOutcome
	onSave              func(ConfigState)
	commitIndeterminate bool
	writes              int
}

func matchesOperationError(err error, category operation.Category, code string) bool {
	failure, ok := operation.ErrorOf(err)
	return ok && failure.Category == category && (code == "" || failure.Code == code)
}

func (repository *configurationTestRepository) LoadConfig() ConfigReadOutcome {
	repository.mu.Lock()
	defer repository.mu.Unlock()
	if repository.loadDisposition != 0 {
		return ConfigReadOutcome{Disposition: repository.loadDisposition}
	}
	if !repository.present {
		return ConfigReadOutcome{Disposition: ConfigReadAbsent}
	}
	state := cloneConfigurationState(repository.state)
	if repository.state.Targets == nil {
		state.Targets = nil
	}
	if repository.state.Routes == nil {
		state.Routes = nil
	}
	if repository.state.Providers == nil {
		state.Providers = nil
	}
	if repository.state.OwnedSlots == nil {
		state.OwnedSlots = nil
	}
	if repository.state.Cleanup == nil {
		state.Cleanup = nil
	}
	if repository.state.Receipts == nil {
		state.Receipts = nil
	}
	return ConfigReadOutcome{Disposition: ConfigReadPresent, State: state}
}

func (repository *configurationTestRepository) SaveConfig(state ConfigState) ConfigWriteOutcome {
	repository.mu.Lock()
	repository.writes++
	outcome := ConfigWriteOutcome{Disposition: ConfigWriteCommitted}
	if len(repository.writeSequence) > 0 {
		outcome = repository.writeSequence[0]
		repository.writeSequence = repository.writeSequence[1:]
	} else if repository.nextWrite != nil {
		outcome = *repository.nextWrite
		repository.nextWrite = nil
	}
	if outcome.Disposition == ConfigWriteCommitted || repository.commitIndeterminate && outcome.Disposition == ConfigWriteIndeterminate {
		repository.state = cloneConfigurationState(state)
		repository.present = true
	}
	hook := repository.onSave
	repository.mu.Unlock()
	if hook != nil {
		hook(cloneConfigurationState(state))
	}
	return outcome
}

func (repository *configurationTestRepository) Health() ConfigRepositoryHealth {
	repository.mu.Lock()
	defer repository.mu.Unlock()
	if repository.health == 0 {
		return ConfigRepositoryReady
	}
	return repository.health
}

func (repository *configurationTestRepository) snapshot() ConfigState {
	repository.mu.Lock()
	defer repository.mu.Unlock()
	return cloneConfigurationState(repository.state)
}

type configurationTestCredentialAccess struct {
	mu                  sync.Mutex
	values              map[string]string
	storeErr            error
	readErr             error
	deleteErr           error
	createErr           error
	createAckLoss       bool
	createValueOverride string
	beforeCreate        func(string)
	afterCreate         func()
	reads               int
	writes              int
	deletes             int
}

func (access *configurationTestCredentialAccess) ReadProviderCredential(_ context.Context, reference string) (string, error) {
	access.mu.Lock()
	defer access.mu.Unlock()
	access.reads++
	if access.storeErr != nil {
		return "", access.storeErr
	}
	if access.readErr != nil {
		return "", access.readErr
	}
	if access.values == nil {
		access.values = map[string]string{}
	}
	return access.values[reference], nil
}

func (access *configurationTestCredentialAccess) CreateProviderCredential(_ context.Context, reference, value string) error {
	access.mu.Lock()
	defer access.mu.Unlock()
	access.writes++
	if access.beforeCreate != nil {
		access.beforeCreate(reference)
	}
	if !strings.HasPrefix(reference, "FLOE_KEY_") || value == "" {
		return errors.New("invalid synthetic provider credential")
	}
	if access.storeErr != nil {
		return access.storeErr
	}
	if access.createErr != nil {
		return access.createErr
	}
	if access.values == nil {
		access.values = map[string]string{}
	}
	if _, exists := access.values[reference]; exists {
		return errors.New("slot exists")
	}
	access.values[reference] = value
	if access.createValueOverride != "" {
		access.values[reference] = access.createValueOverride
	}
	if access.afterCreate != nil {
		access.afterCreate()
	}
	if access.createAckLoss {
		return errors.New("synthetic lost create acknowledgement")
	}
	return nil
}

func (access *configurationTestCredentialAccess) DeleteProviderCredential(_ context.Context, reference string) error {
	access.mu.Lock()
	defer access.mu.Unlock()
	access.deletes++
	if access.deleteErr != nil {
		return access.deleteErr
	}
	if access.storeErr != nil {
		return access.storeErr
	}
	delete(access.values, reference)
	return nil
}

func (access *configurationTestCredentialAccess) credential(reference string) string {
	access.mu.Lock()
	defer access.mu.Unlock()
	return access.values[reference]
}
func (access *configurationTestCredentialAccess) setCredential(reference, value string) {
	access.mu.Lock()
	defer access.mu.Unlock()
	if access.values == nil {
		access.values = map[string]string{}
	}
	access.values[reference] = value
}
func (access *configurationTestCredentialAccess) counts() (int, int) {
	access.mu.Lock()
	defer access.mu.Unlock()
	return access.writes, access.deletes
}
func (access *configurationTestCredentialAccess) writeCount() int {
	writes, _ := access.counts()
	return writes
}
func (access *configurationTestCredentialAccess) deleteCount() int {
	_, deletes := access.counts()
	return deletes
}

type configurationTestFactory struct{}

func (configurationTestFactory) ValidateTarget(target ProviderTarget) error {
	if target.Provider != "openai_compatible" || target.BaseURL == "" || target.Model == "" {
		return errors.New("invalid synthetic provider target")
	}
	return nil
}

func (configurationTestFactory) Open(_ context.Context, targets map[string]ProviderTarget) (map[string]ModelAccount, ModelExecutor, error) {
	accounts := make(map[string]ModelAccount, len(targets))
	for id, target := range targets {
		accounts[id] = configurationTestAccount{provider: target.Provider, model: target.Model, endpoint: target.BaseURL}
	}
	return accounts, selectionTestExecutor{}, nil
}

type configurationTestAccount struct {
	provider, model, endpoint string
}

func (account configurationTestAccount) Ready(context.Context) error { return nil }
func (account configurationTestAccount) ReplayIdentity() string {
	return account.provider + ":" + account.model
}
func (configurationTestAccount) ProtocolCapabilities() []string {
	return []string{ChatCapability, StructuredOutputCapability, ToolProposalsCapability}
}
func (account configurationTestAccount) ModelIdentity() ModelIdentity {
	endpoint, _ := CanonicalModelEndpoint(account.endpoint)
	return ModelIdentity{ProviderID: account.provider, ModelID: account.model, Endpoint: endpoint}
}
func (configurationTestAccount) BudgetOverride() *ModelBudgetOverride { return nil }

func configurationTestState(targetID, model string) ConfigState {
	return ConfigState{
		SchemaVersion: 3,
		OwnedSlots:    []string{}, Cleanup: []CredentialCleanup{}, Receipts: []ConfigurationReceipt{},
		Targets: map[string]ProviderTarget{
			targetID: {Provider: "openai_compatible", BaseURL: "https://example.invalid", Model: model},
		},
		Routes: map[Purpose]PurposeRoute{
			QuickResponse: {TargetID: targetID, ReasoningEffort: "medium", Enabled: true},
		},
		Providers: map[string]ProviderProfile{},
	}
}

func openConfigurationTestOwner(t *testing.T, repository ConfigRepository, credentials ProviderCredentialAccess) (*Configuration, *Service) {
	t.Helper()
	engine, err := NewService(selectionTestTrust{}, emptyCapabilityMetadata())
	if err != nil {
		t.Fatal(err)
	}
	owner, err := OpenConfiguration(context.Background(), repository, engine, selectionTestTrust{}, credentials, configurationTestFactory{})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(owner.Close)
	return owner, engine
}

func configurationSnapshot(owner *Configuration) ConfigState {
	owner.mu.Lock()
	defer owner.mu.Unlock()
	return cloneConfigurationState(owner.state)
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
	corruptEngine, err := NewService(selectionTestTrust{}, emptyCapabilityMetadata())
	if err != nil {
		t.Fatal(err)
	}
	if _, err := OpenConfiguration(context.Background(), corrupt, corruptEngine, selectionTestTrust{}, credentials, configurationTestFactory{}); err == nil {
		t.Fatal("corrupt persisted config was accepted")
	}
	if corruptEngine.generation != 1 {
		t.Fatalf("failed startup partially configured the live engine: generation=%d", corruptEngine.generation)
	}
	semanticCorruption := &configurationTestRepository{present: true, state: ConfigState{SchemaVersion: 2}}
	semanticEngine, err := NewService(selectionTestTrust{}, emptyCapabilityMetadata())
	if err != nil {
		t.Fatal(err)
	}
	if _, err := OpenConfiguration(context.Background(), semanticCorruption, semanticEngine, selectionTestTrust{}, credentials, configurationTestFactory{}); !errors.Is(err, ErrUnsupportedConfigVersion) {
		t.Fatalf("schema-2 configuration did not fail with an explicit unsupported-version error: %v", err)
	}
	if semanticEngine.generation != 1 || semanticCorruption.writes != 0 || semanticCorruption.state.SchemaVersion != 2 {
		t.Fatalf("unsupported config was changed or partially adopted: generation=%d writes=%d state=%#v", semanticEngine.generation, semanticCorruption.writes, semanticCorruption.state)
	}
	incompatible := &configurationTestRepository{present: true, state: ConfigState{SchemaVersion: 1}}
	incompatibleEngine, _ := NewService(selectionTestTrust{}, emptyCapabilityMetadata())
	if _, err := OpenConfiguration(context.Background(), incompatible, incompatibleEngine, selectionTestTrust{}, credentials, configurationTestFactory{}); !errors.Is(err, ErrUnsupportedConfigVersion) {
		t.Fatalf("schema-1 profile did not fail with an explicit unsupported-version error: %v", err)
	}
	if incompatible.writes != 0 || incompatible.state.SchemaVersion != 1 || incompatibleEngine.generation != 1 {
		t.Fatal("incompatible stored configuration was changed or partially adopted")
	}
}

func TestUnsupportedConfigurationVersionsFailClosedUnchanged(t *testing.T) {
	for _, version := range []int{1, 2, 4} {
		t.Run(fmt.Sprintf("schema_%d", version), func(t *testing.T) {
			data := []byte(fmt.Sprintf(`{"schema_version":%d,"revision":7,"targets":{"target-a":{"provider":"openai_compatible","base_url":"https://example.invalid","model":"selected-model","capabilities":["chat","tool_proposals"]}},"routes":{},"providers":{},"owned_slots":[],"cleanup":[],"receipts":[]}`, version))
			if _, err := DecodeConfigState(data); !errors.Is(err, ErrUnsupportedConfigVersion) || !strings.Contains(err.Error(), fmt.Sprint(version)) {
				t.Fatalf("unsupported schema did not report its version explicitly: %v", err)
			}

			repository := &configurationTestRepository{
				state: ConfigState{SchemaVersion: version, Revision: 7}, present: true,
			}
			engine, err := NewService(selectionTestTrust{}, emptyCapabilityMetadata())
			if err != nil {
				t.Fatal(err)
			}
			if _, err := OpenConfiguration(context.Background(), repository, engine, selectionTestTrust{}, &configurationTestCredentialAccess{}, configurationTestFactory{}); !errors.Is(err, ErrUnsupportedConfigVersion) {
				t.Fatalf("startup did not return explicit unsupported-version error: %v", err)
			}
			if repository.writes != 0 || repository.state.SchemaVersion != version || repository.state.Revision != 7 || engine.Generation() != 1 {
				t.Fatalf("unsupported profile was modified or adopted: writes=%d state=%#v generation=%d", repository.writes, repository.state, engine.Generation())
			}
		})
	}

	assertionBearingSchemaThree := []byte(`{"schema_version":3,"revision":1,"targets":{"target-a":{"provider":"openai_compatible","base_url":"https://example.invalid","model":"selected-model","capabilities":["chat"]}},"routes":{},"providers":{},"owned_slots":[],"cleanup":[],"receipts":[]}`)
	if _, err := DecodeConfigState(assertionBearingSchemaThree); err == nil {
		t.Fatal("current schema accepted a supplied manual capability assertion")
	}
}

func TestUnknownCapabilityEvidenceDoesNotBlockProviderProfileLoad(t *testing.T) {
	const slot = "FLOE_KEY_PROFILE_UNKNOWN_EVIDENCE"
	repository := &configurationTestRepository{state: providerProfileState(slot), present: true}
	credentials := &configurationTestCredentialAccess{values: map[string]string{slot: "synthetic profile credential"}}
	owner, engine := openConfigurationTestOwner(t, repository, credentials)
	states, err := engine.CapabilityStatesForTarget(context.Background(), profileTargetID("openai_compatible", string(QuickResponse)))
	if err != nil {
		t.Fatalf("unknown feature evidence blocked a valid chat profile: %v", err)
	}
	if states[ChatCapability].Status != CapabilityUnknown || states[StructuredOutputCapability].Status != CapabilityUnknown ||
		states[ToolProposalsCapability].Status != CapabilityUnknown {
		t.Fatalf("bootstrap without verified feature facts fabricated capability support: %#v", states)
	}
	operatorSnapshot, err := owner.Snapshot(context.Background(), trust.OperatorPrincipal{})
	if err != nil || operatorSnapshot.Profiles["openai_compatible"].Purposes["quick_response"].CapabilityStates[ChatCapability].Status != CapabilityUnknown {
		t.Fatalf("unknown evidence blocked configuration management or was not displayed: snapshot=%#v err=%v", operatorSnapshot, err)
	}
}

func TestExactConfigurationOperationReplaySurvivesCapabilityMetadataUpdate(t *testing.T) {
	metadata := emptyCapabilityMetadata()
	engine, err := NewService(selectionTestTrust{}, metadata)
	if err != nil {
		t.Fatal(err)
	}
	repository := &configurationTestRepository{}
	owner, err := OpenConfiguration(context.Background(), repository, engine, selectionTestTrust{}, &configurationTestCredentialAccess{}, configurationTestFactory{})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(owner.Close)
	command := providerTargetUpdate(trust.NewID(), "model-a", "")
	if err := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, command); err != nil {
		t.Fatalf("commit synthetic target command: %v", err)
	}
	before := repository.snapshot()
	generation := engine.Generation()
	metadata.replace(CapabilityEvidenceSnapshot{ContractVersion: 1, Entries: []CapabilityEvidenceEntry{
		testEvidence("openai_compatible", "model-a", "https://example.invalid", StructuredOutputCapability, CapabilitySupported),
	}})
	if err := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, command); err != nil {
		t.Fatalf("replay exact immutable command after metadata update: %v", err)
	}
	after := repository.snapshot()
	if repository.writes != 1 || after.Revision != before.Revision || after.Targets["target-a"].Model != "model-a" || engine.Generation() != generation {
		t.Fatalf("metadata update changed exact operation replay: writes=%d before=%#v after=%#v generations=%d/%d", repository.writes, before, after, generation, engine.Generation())
	}
}

func TestRecoveryWithInvalidAuthoritativeSnapshotKeepsEngineDenied(t *testing.T) {
	repository := &configurationTestRepository{state: configurationTestState("target-a", "model-a"), present: true}
	owner, engine := openConfigurationTestOwner(t, repository, &configurationTestCredentialAccess{})
	if _, _, _, err := engine.current(context.Background(), QuickResponse); err != nil {
		t.Fatalf("initial configuration was not active: %v", err)
	}
	repository.mu.Lock()
	repository.loadDisposition = ConfigReadInvalid
	repository.mu.Unlock()
	_, result := owner.RecoverOperation(context.Background(), trust.OperatorPrincipal{}, trust.NewID())
	if !matchesOperationError(result, operation.Unavailable, "") || owner.RequiredError() == nil {
		t.Fatalf("invalid authoritative snapshot did not keep configuration denied: result=%#v required=%v", result, owner.RequiredError())
	}
	if _, _, _, err := engine.current(context.Background(), QuickResponse); err == nil {
		t.Fatal("engine remained available after authoritative recovery read failed")
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
			next.Targets["target-b"] = ProviderTarget{Provider: "openai_compatible", BaseURL: "https://example.invalid", Model: "model-b"}
			next.Routes[QuickResponse] = PurposeRoute{TargetID: "target-b", ReasoningEffort: "high", Enabled: true}
			repository.nextWrite = &ConfigWriteOutcome{Disposition: test.disposition, Cause: errors.New("synthetic repository outcome")}
			result := owner.commitOperation(context.Background(), trust.NewID(), trust.Digest("synthetic-configuration-commit"), configurationContent(next))
			if !matchesOperationError(result, operation.Unavailable, "") {
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
		OperationID: trust.NewID(),
		ID:          "target-b", Provider: "openai_compatible", BaseURL: "https://example.invalid", Model: "model-b",
		APIKey: "synthetic-key",
	})
	if !matchesOperationError(result, operation.Unavailable, "credential_store_unavailable") {
		t.Fatalf("credential unavailability was not returned as a safe rejection: %#v", result)
	}
	if credentials.writeCount() != 0 || repository.writes != 0 || owner.state.Targets["target-b"].Model != "" {
		t.Fatalf("configuration changed after a failed credential write: writes=%d repo=%d state=%#v", credentials.writeCount(), repository.writes, owner.state)
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
	target.BudgetOverride = &ModelBudgetOverride{SchemaVersion: ModelBudgetOverrideVersion, MaxInputJSONBytes: &maxInputBytes}
	state.Targets["target-a"] = target
	state.Providers["openai_compatible"] = ProviderProfile{
		BaseURL: "https://example.invalid", APIKeyEnv: "FLOE_KEY_SYNTHETIC",
		Purposes: map[string]PurposeModel{"quick_response": {Model: "model-a"}},
	}
	cloned := cloneConfigurationState(state)
	state.Routes[QuickResponse] = PurposeRoute{TargetID: "changed", Enabled: true}
	target = state.Targets["target-a"]
	*target.BudgetOverride.MaxInputJSONBytes = 1
	state.Targets["target-a"] = target
	state.Providers["openai_compatible"].Purposes["quick_response"] = PurposeModel{Model: "changed"}
	if cloned.Routes[QuickResponse].TargetID != "target-a" ||
		*cloned.Targets["target-a"].BudgetOverride.MaxInputJSONBytes != wantMaxInputBytes ||
		cloned.Providers["openai_compatible"].Purposes["quick_response"].Model != "model-a" {
		t.Fatalf("configuration clone shared mutable state: %#v", cloned)
	}
}

func providerTargetUpdate(operationID, model, key string) TargetUpdate {
	return TargetUpdate{OperationID: operationID, ID: "target-a", Provider: "openai_compatible", BaseURL: "https://example.invalid", Model: model, APIKey: key}
}

func providerProfileState(slot string) ConfigState {
	state := emptyConfigurationState()
	state.Providers["openai_compatible"] = ProviderProfile{
		BaseURL: "https://example.invalid", APIKeyEnv: slot,
		Purposes: map[string]PurposeModel{"quick_response": {Model: "model-a", ReasoningEffort: "medium"}},
	}
	state.Routes[QuickResponse] = PurposeRoute{TargetID: "managed_openai_compatible_quick_response", ReasoningEffort: "medium", Enabled: true}
	return state
}

func TestCredentialIntentPrecedesCreateAndRejectedIntentDoesNotWrite(t *testing.T) {
	repository := &configurationTestRepository{state: configurationTestState("target-a", "model-a"), present: true}
	credentials := &configurationTestCredentialAccess{}
	owner, _ := openConfigurationTestOwner(t, repository, credentials)
	var sawPending bool
	credentials.beforeCreate = func(reference string) {
		sawPending = repository.state.Pending != nil && repository.state.Pending.Slot == reference
		encoded, err := jsonMarshalConfig(repository.state)
		if err != nil || strings.Contains(encoded, "synthetic-secret") {
			t.Fatalf("intent contained secret or failed to encode: %v", err)
		}
	}
	result := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, providerTargetUpdate(trust.NewID(), "model-b", "synthetic-secret"))
	if result != nil || !sawPending || credentials.writeCount() != 1 {
		t.Fatalf("intent/write ordering failed: result=%#v pending_seen=%v writes=%d", result, sawPending, credentials.writeCount())
	}
	if repository.state.Pending != nil || owner.state.Targets["target-a"].Model != "model-b" {
		t.Fatalf("candidate did not settle: %#v", repository.state)
	}

	rejectedRepository := &configurationTestRepository{state: configurationTestState("target-a", "model-a"), present: true, nextWrite: &ConfigWriteOutcome{Disposition: ConfigWriteRejected, Cause: errors.New("synthetic reject")}}
	rejectedCredentials := &configurationTestCredentialAccess{}
	rejected, rejectedEngine := openConfigurationTestOwner(t, rejectedRepository, rejectedCredentials)
	result = rejected.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, providerTargetUpdate(trust.NewID(), "model-b", "synthetic-secret"))
	if !matchesOperationError(result, operation.Unavailable, "") || rejectedCredentials.writes != 0 || rejected.state.Targets["target-a"].Model != "model-a" {
		t.Fatalf("rejected intent wrote key or changed active config: result=%#v writes=%d", result, rejectedCredentials.writes)
	}
	live, _, _, err := rejectedEngine.current(context.Background(), QuickResponse)
	if err != nil || live.TargetID() != "target-a" {
		t.Fatalf("known rejected intent altered live engine: target=%q err=%v", live.TargetID(), err)
	}
}

func TestCredentialCreateAckLossAndChangedCommandRetry(t *testing.T) {
	repository := &configurationTestRepository{state: configurationTestState("target-a", "model-a"), present: true}
	credentials := &configurationTestCredentialAccess{createAckLoss: true}
	owner, _ := openConfigurationTestOwner(t, repository, credentials)
	input := providerTargetUpdate(trust.NewID(), "model-b", "synthetic-secret")
	first := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, input)
	if first != nil || credentials.writeCount() != 1 {
		t.Fatalf("readback did not recover lost create ACK: %#v writes=%d", first, credentials.writeCount())
	}
	second := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, input)
	if second != nil || credentials.writeCount() != 1 {
		t.Fatalf("exact retry was not idempotent: %#v writes=%d", second, credentials.writeCount())
	}
	changed := input
	changed.Model = "model-c"
	third := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, changed)
	if !matchesOperationError(third, operation.Conflict, "operation_id_reused") || credentials.writeCount() != 1 {
		t.Fatalf("changed command reused operation identity: %#v writes=%d", third, credentials.writeCount())
	}
	encoded, err := jsonMarshalConfig(repository.state)
	if err != nil || strings.Contains(encoded, "synthetic-secret") {
		t.Fatalf("raw key persisted: err=%v", err)
	}
}

func TestIntentAckLossBeforeWriteRecoversAbsenceAndKeepsOldConfig(t *testing.T) {
	for _, committed := range []bool{false, true} {
		t.Run(map[bool]string{false: "not persisted", true: "pending persisted"}[committed], func(t *testing.T) {
			repository := &configurationTestRepository{state: configurationTestState("target-a", "model-a"), present: true, commitIndeterminate: committed,
				writeSequence: []ConfigWriteOutcome{{Disposition: ConfigWriteIndeterminate, Cause: errors.New("synthetic lost snapshot ACK")}}}
			credentials := &configurationTestCredentialAccess{}
			owner, engine := openConfigurationTestOwner(t, repository, credentials)
			operationID := trust.NewID()
			result := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, providerTargetUpdate(operationID, "model-b", "synthetic-secret"))
			if !matchesOperationError(result, operation.Unavailable, "") || credentials.writeCount() != 0 {
				t.Fatalf("intent ACK loss wrote a key: %#v writes=%d", result, credentials.writeCount())
			}
			if committed && repository.state.Pending == nil {
				t.Fatal("test store did not retain pending intent")
			}
			recovered, recoverErr := owner.RecoverOperation(context.Background(), trust.OperatorPrincipal{}, operationID)
			if recoverErr != nil || !recovered.Recovered {
				t.Fatalf("authoritative recovery failed: result=%#v err=%v", recovered, recoverErr)
			}
			live, _, _, err := engine.current(context.Background(), QuickResponse)
			if err != nil || live.TargetID() != "target-a" || owner.state.Pending != nil {
				t.Fatalf("recovery did not preserve old config after exact absence: state=%#v target=%q err=%v", owner.state, live.TargetID(), err)
			}
		})
	}
}

func TestCandidateCommitAckLossDeniesUntilAuthoritativeRecovery(t *testing.T) {
	repository := &configurationTestRepository{state: configurationTestState("target-a", "model-a"), present: true, commitIndeterminate: true,
		writeSequence: []ConfigWriteOutcome{{Disposition: ConfigWriteCommitted}, {Disposition: ConfigWriteIndeterminate, Cause: errors.New("synthetic candidate commit ACK loss")}}}
	credentials := &configurationTestCredentialAccess{}
	owner, engine := openConfigurationTestOwner(t, repository, credentials)
	operationID := trust.NewID()
	result := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, providerTargetUpdate(operationID, "model-b", "synthetic-secret"))
	if !matchesOperationError(result, operation.Unavailable, "") || !owner.configUnavailable || repository.state.Pending != nil || repository.state.Targets["target-a"].Model != "model-b" {
		t.Fatalf("candidate ACK loss did not durably commit while denying: result=%#v state=%#v", result, repository.state)
	}
	if _, _, _, err := engine.current(context.Background(), QuickResponse); err == nil {
		t.Fatal("ambiguous candidate persistence left inference admitted")
	}
	recovered, recoverErr := owner.RecoverOperation(context.Background(), trust.OperatorPrincipal{}, operationID)
	if recoverErr != nil || !recovered.Recovered {
		t.Fatalf("candidate recovery failed: result=%#v err=%v", recovered, recoverErr)
	}
	live, _, _, err := engine.current(context.Background(), QuickResponse)
	if err != nil || live.TargetID() != "target-a" || owner.state.Targets["target-a"].Model != "model-b" {
		t.Fatalf("authoritative candidate not adopted after recovery: %#v %v", owner.state, err)
	}
}

func TestCredentialReadbackUnavailableOrMismatchRemainsPending(t *testing.T) {
	for _, mismatch := range []bool{false, true} {
		t.Run(map[bool]string{false: "readback unavailable", true: "digest mismatch"}[mismatch], func(t *testing.T) {
			repository := &configurationTestRepository{state: configurationTestState("target-a", "model-a"), present: true}
			credentials := &configurationTestCredentialAccess{}
			if mismatch {
				credentials.createValueOverride = "different-synthetic-value"
			} else {
				credentials.afterCreate = func() { credentials.readErr = errors.New("synthetic readback failure") }
			}
			owner, engine := openConfigurationTestOwner(t, repository, credentials)
			operationID := trust.NewID()
			result := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, providerTargetUpdate(operationID, "model-b", "synthetic-secret"))
			if !matchesOperationError(result, operation.Unavailable, "") || owner.state.Pending == nil || !owner.configUnavailable {
				t.Fatalf("ambiguous readback was not held pending: %#v", result)
			}
			if _, _, _, err := engine.current(context.Background(), QuickResponse); err == nil {
				t.Fatal("ambiguous credential state left engine admitted")
			}
			pending := *owner.state.Pending
			credentials.readErr = nil
			credentials.createValueOverride = ""
			credentials.setCredential(pending.Slot, "synthetic-secret")
			recovered, recoverErr := owner.RecoverOperation(context.Background(), trust.OperatorPrincipal{}, operationID)
			if recoverErr != nil || !recovered.Recovered {
				t.Fatalf("exact recovery failed: result=%#v err=%v", recovered, recoverErr)
			}
			if owner.state.Pending != nil || owner.state.Targets["target-a"].Model != "model-b" {
				t.Fatalf("recovered candidate not committed: %#v", owner.state)
			}
		})
	}
}

func TestEngineAdoptionFailurePersistsCandidateThenRecovers(t *testing.T) {
	repository := &configurationTestRepository{state: configurationTestState("target-a", "model-a"), present: true}
	credentials := &configurationTestCredentialAccess{}
	owner, engine := openConfigurationTestOwner(t, repository, credentials)
	engine.DenyConfiguration()
	operationID := trust.NewID()
	result := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, providerTargetUpdate(operationID, "model-b", "synthetic-secret"))
	if !matchesOperationError(result, operation.Unavailable, "") || repository.state.Targets["target-a"].Model != "model-b" || owner.state.Targets["target-a"].Model != "model-b" {
		t.Fatalf("adoption failure lost durable candidate: result=%#v state=%#v", result, repository.state)
	}
	recovered, recoverErr := owner.RecoverOperation(context.Background(), trust.OperatorPrincipal{}, operationID)
	if recoverErr != nil || !recovered.Recovered {
		t.Fatalf("engine recovery failed: result=%#v err=%v", recovered, recoverErr)
	}
	live, _, _, err := engine.current(context.Background(), QuickResponse)
	if err != nil || live.TargetID() != "target-a" {
		t.Fatalf("recovered engine did not serve committed config: %q %v", live.TargetID(), err)
	}
}

func TestLegacySharedCredentialReferenceIsNeverDeleted(t *testing.T) {
	const legacy = "FLOE_KEY_SHARED_LEGACY"
	state := configurationTestState("target-a", "model-a")
	first := state.Targets["target-a"]
	first.APIKeyEnv = legacy
	state.Targets["target-a"] = first
	second := first
	second.Model = "model-b"
	state.Targets["target-b"] = second
	credentials := &configurationTestCredentialAccess{values: map[string]string{legacy: "synthetic-shared-key"}}
	repository := &configurationTestRepository{state: state, present: true}
	owner, _ := openConfigurationTestOwner(t, repository, credentials)
	result := owner.DeleteTarget(context.Background(), trust.OperatorPrincipal{}, "target-b", trust.NewID())
	if result != nil {
		t.Fatalf("delete shared target failed: %#v", result)
	}
	if credentials.deleteCount() != 0 || credentials.credential(legacy) != "synthetic-shared-key" || contentHasSlot(configurationContent(owner.state), legacy) == false {
		t.Fatalf("legacy/shared slot was deleted or unreferenced: deletes=%d state=%#v", credentials.deleteCount(), owner.state)
	}
}

func TestReopenSettlesExactOwnedCleanupBeforeConfiguration(t *testing.T) {
	const slot = "FLOE_KEY_OWNED_SYNTHETIC"
	state := emptyConfigurationState()
	state.OwnedSlots = []string{slot}
	state.Cleanup = []CredentialCleanup{{Slot: slot, RetiredGeneration: 9}}
	repository := &configurationTestRepository{state: state, present: true}
	credentials := &configurationTestCredentialAccess{values: map[string]string{slot: "synthetic-retired-key"}}
	owner, engine := openConfigurationTestOwner(t, repository, credentials)
	if credentials.deleteCount() != 1 || credentials.credential(slot) != "" || len(owner.state.Cleanup) != 0 || len(owner.state.OwnedSlots) != 0 {
		t.Fatalf("startup did not settle exact retired slot: deletes=%d state=%#v", credentials.deleteCount(), owner.state)
	}
	if _, _, _, err := engine.current(context.Background(), QuickResponse); err == nil {
		t.Fatal("empty recovered config unexpectedly admitted inference")
	}
}

func TestConfigurationReceiptCapacityDoesNotEvict(t *testing.T) {
	state := configurationTestState("target-a", "model-a")
	for i := 0; i < maxConfigurationReceipts; i++ {
		id := trust.NewID()
		state.Receipts = append(state.Receipts, ConfigurationReceipt{OperationID: id, Fingerprint: trust.Digest(id), Category: string(operation.Ready), Code: "ok"})
	}
	repository := &configurationTestRepository{state: state, present: true}
	credentials := &configurationTestCredentialAccess{}
	owner, _ := openConfigurationTestOwner(t, repository, credentials)
	result := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, providerTargetUpdate(trust.NewID(), "model-b", "synthetic-secret"))
	if !matchesOperationError(result, operation.Limited, "configuration_receipt_capacity") || credentials.writeCount() != 0 || repository.writes != 0 {
		t.Fatalf("receipt capacity evicted or mutated state: %#v writes=%d saves=%d", result, credentials.writeCount(), repository.writes)
	}
}

func TestInFlightGenerationLeaseSurvivesCancellationAndDefersCleanup(t *testing.T) {
	const oldSlot = "FLOE_KEY_OLD_PINNED"
	state := providerProfileState(oldSlot)
	state.OwnedSlots = []string{oldSlot}
	credentials := &configurationTestCredentialAccess{values: map[string]string{oldSlot: "synthetic-old-key"}}
	repository := &configurationTestRepository{state: state, present: true}
	blocker := &blockingConfigurationExecutor{started: make(chan struct{}), release: make(chan struct{})}
	factory := blockingConfigurationFactory{executor: blocker}
	metadata := &testCapabilityMetadata{snapshot: CapabilityEvidenceSnapshot{ContractVersion: 1, Entries: []CapabilityEvidenceEntry{
		testEvidence("openai_compatible", "model-a", "https://example.invalid", ChatCapability, CapabilitySupported),
	}}}
	engine, err := NewService(selectionTestTrust{}, metadata)
	if err != nil {
		t.Fatal(err)
	}
	owner, err := OpenConfiguration(context.Background(), repository, engine, selectionTestTrust{}, credentials, factory)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(owner.Close)
	inventory, err := engine.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	revision := inventory.Get(QuickResponse).CapabilityRevision
	ctx, cancel := context.WithCancel(context.Background())
	invocation := AgentInvocation{Purpose: QuickResponse, CapabilityRevision: revision, AttemptID: trust.NewID(), DataClasses: []string{"synthetic"}, Instructions: "synthetic lifecycle test", Input: AgentInput{Messages: []Message{{Role: "user", Content: stringPointer(`{"run_instructions":{"output_format":{"kind":"text"}}}`)}}, Tools: []Tool{}}, OutputFormat: OutputFormat{Kind: "text"}, MaxOutputBytes: 128}
	invokeDone := make(chan error, 1)
	go func() {
		_, invokeErr := engine.InvokeAgent(ctx, trust.Principal{}, invocation)
		invokeDone <- invokeErr
	}()
	select {
	case <-blocker.started:
	case invokeErr := <-invokeDone:
		t.Fatalf("inference did not reach mock provider: %v", invokeErr)
	case <-time.After(time.Second):
		t.Fatal("inference did not start mock provider")
	}
	cancel()
	result := owner.UpdateProvider(context.Background(), trust.OperatorPrincipal{}, ProviderUpdate{OperationID: trust.NewID(), Provider: "openai_compatible", BaseURL: "https://example.invalid", APIKey: "synthetic-new-key", Purposes: map[string]PurposeModel{"quick_response": {Model: "model-b", ReasoningEffort: "medium"}}})
	if result != nil {
		close(blocker.release)
		t.Fatalf("replacement failed: %#v", result)
	}
	if credentials.credential(oldSlot) != "synthetic-old-key" || engine.GenerationDrained(2) {
		close(blocker.release)
		t.Fatal("cancellation or adoption falsely drained pinned generation")
	}
	select {
	case err := <-invokeDone:
		close(blocker.release)
		t.Fatalf("blocked executor returned before completion: %v", err)
	default:
	}
	close(blocker.release)
	select {
	case <-invokeDone:
	case <-time.After(2 * time.Second):
		t.Fatal("inference did not return after executor completion")
	}
	deadline := time.Now().Add(3 * time.Second)
	for time.Now().Before(deadline) {
		if credentials.credential(oldSlot) == "" {
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
	if credentials.credential(oldSlot) != "" || !engine.GenerationDrained(2) {
		t.Fatal("retired credential did not clean up after exact generation drained")
	}
}

type blockingConfigurationFactory struct{ executor ModelExecutor }

func (factory blockingConfigurationFactory) ValidateTarget(target ProviderTarget) error {
	return (configurationTestFactory{}).ValidateTarget(target)
}
func (factory blockingConfigurationFactory) Open(ctx context.Context, targets map[string]ProviderTarget) (map[string]ModelAccount, ModelExecutor, error) {
	accounts, _, err := (configurationTestFactory{}).Open(ctx, targets)
	return accounts, factory.executor, err
}

type blockingConfigurationExecutor struct {
	started chan struct{}
	release chan struct{}
	once    sync.Once
}

func (executor *blockingConfigurationExecutor) InvokeAgent(context.Context, ResolvedModelTarget, AgentInvocation) (AgentResult, error) {
	executor.once.Do(func() { close(executor.started) })
	<-executor.release
	return AgentResult{Output: []Step{{Kind: "answer", Text: "synthetic complete"}}}, nil
}
func (*blockingConfigurationExecutor) InvokeStructured(context.Context, ResolvedModelTarget, StructuredInvocation) (StructuredResult, error) {
	return StructuredResult{}, errors.New("not used")
}

func TestEmptyProviderUpdateDeletesOnlyRetiredOwnedSlot(t *testing.T) {
	const slot = "FLOE_KEY_PROVIDER_OWNED"
	state := providerProfileState(slot)
	state.OwnedSlots = []string{slot}
	repository := &configurationTestRepository{state: state, present: true}
	credentials := &configurationTestCredentialAccess{values: map[string]string{slot: "synthetic-provider-key"}}
	owner, _ := openConfigurationTestOwner(t, repository, credentials)
	result := owner.UpdateProvider(context.Background(), trust.OperatorPrincipal{}, ProviderUpdate{OperationID: trust.NewID(), Provider: "openai_compatible", Purposes: map[string]PurposeModel{}})
	if result != nil {
		t.Fatalf("empty provider update failed: %#v", result)
	}
	deadline := time.Now().Add(3 * time.Second)
	for time.Now().Before(deadline) {
		if credentials.credential(slot) == "" && len(configurationSnapshot(owner).Cleanup) == 0 {
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
	state = configurationSnapshot(owner)
	if credentials.credential(slot) != "" || credentials.deleteCount() != 1 || len(state.Providers) != 0 || len(state.Cleanup) != 0 || len(state.OwnedSlots) != 0 {
		t.Fatalf("provider removal did not settle exact owned cleanup: deletes=%d state=%#v", credentials.deleteCount(), state)
	}
}

func TestInvalidTargetConfigurationDoesNotCreateCredential(t *testing.T) {
	repository := &configurationTestRepository{state: configurationTestState("target-a", "model-a"), present: true}
	credentials := &configurationTestCredentialAccess{}
	owner, _ := openConfigurationTestOwner(t, repository, credentials)
	input := providerTargetUpdate(trust.NewID(), "model-b", "synthetic-secret")
	input.Provider = "unknown_provider"
	result := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, input)
	if !matchesOperationError(result, operation.Invalid, "") || credentials.writeCount() != 0 || repository.writes != 0 || owner.state.Targets["target-a"].Provider != "openai_compatible" {
		t.Fatalf("invalid provider config reached credential storage: result=%#v writes=%d saves=%d", result, credentials.writeCount(), repository.writes)
	}
}

func TestCandidateKnownRejectKeepsIntentAndBlocksNewTransition(t *testing.T) {
	repository := &configurationTestRepository{state: configurationTestState("target-a", "model-a"), present: true,
		writeSequence: []ConfigWriteOutcome{{Disposition: ConfigWriteCommitted}, {Disposition: ConfigWriteRejected, Cause: errors.New("synthetic candidate reject")}}}
	credentials := &configurationTestCredentialAccess{}
	owner, engine := openConfigurationTestOwner(t, repository, credentials)
	input := providerTargetUpdate(trust.NewID(), "model-b", "synthetic-secret")
	result := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, input)
	if !matchesOperationError(result, operation.Unavailable, "") || owner.state.Pending == nil || credentials.writeCount() != 1 {
		t.Fatalf("known candidate rejection lost exact pending intent: result=%#v state=%#v", result, owner.state)
	}
	live, _, _, err := engine.current(context.Background(), QuickResponse)
	if err != nil || live.TargetID() != "target-a" {
		t.Fatalf("rejected candidate partially adopted: target=%q err=%v", live.TargetID(), err)
	}
	other := providerTargetUpdate(trust.NewID(), "model-c", "synthetic-other-key")
	if result = owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, other); !matchesOperationError(result, operation.Conflict, "") || credentials.writeCount() != 1 {
		t.Fatalf("new operation erased a pending transition: result=%#v writes=%d", result, credentials.writeCount())
	}
	if result = owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, input); result != nil || credentials.writeCount() != 1 {
		t.Fatalf("exact retry did not finish pending candidate: result=%#v writes=%d", result, credentials.writeCount())
	}
}

func TestOpenConfigurationResolvesDurablePendingBeforeAdmission(t *testing.T) {
	for _, keyExists := range []bool{false, true} {
		t.Run(map[bool]string{false: "confirmed absence aborts", true: "matching key commits"}[keyExists], func(t *testing.T) {
			old := configurationTestState("target-a", "model-a")
			candidate := cloneConfigurationState(old)
			target := candidate.Targets["target-a"]
			target.Model = "model-b"
			target.APIKeyEnv = "FLOE_KEY_PENDING_RESTART"
			candidate.Targets["target-a"] = target
			operationID := trust.NewID()
			pending := CredentialTransition{OperationID: operationID, Fingerprint: trust.Digest("restart-command"), Slot: target.APIKeyEnv, Digest: trust.Digest("synthetic-restart-key"), Candidate: configurationContent(candidate)}
			old.Pending = &pending
			old.Revision++
			repository := &configurationTestRepository{state: old, present: true}
			credentials := &configurationTestCredentialAccess{}
			if keyExists {
				credentials.values = map[string]string{pending.Slot: "synthetic-restart-key"}
			}
			engine, err := NewService(selectionTestTrust{}, emptyCapabilityMetadata())
			if err != nil {
				t.Fatal(err)
			}
			owner, err := OpenConfiguration(context.Background(), repository, engine, selectionTestTrust{}, credentials, configurationTestFactory{})
			if err != nil {
				t.Fatalf("startup failed to resolve durable pending transition: %v", err)
			}
			t.Cleanup(owner.Close)
			if owner.state.Pending != nil {
				t.Fatal("startup admitted inference with unresolved transition")
			}
			live, _, _, currentErr := engine.current(context.Background(), QuickResponse)
			if currentErr != nil {
				t.Fatalf("startup did not configure resolved state: %v", currentErr)
			}
			if keyExists {
				if owner.state.Targets["target-a"].Model != "model-b" || live.modelIdentity.ModelID != "model-b" {
					t.Fatalf("matching exact slot did not commit candidate: %#v", owner.state)
				}
			} else if owner.state.Targets["target-a"].Model != "model-a" || live.modelIdentity.ModelID != "model-a" {
				t.Fatalf("confirmed absence did not retain prior config: %#v", owner.state)
			}
		})
	}
}

func TestConfigurationSnapshotByteLimitRejectsBeforeCredentialWrite(t *testing.T) {
	state := configurationTestState("target-a", "model-a")
	longBaseURL := "https://example.invalid/" + strings.Repeat("x", 1200)
	for i := 0; i < 31; i++ {
		id := fmt.Sprintf("target-%02d", i)
		state.Targets[id] = ProviderTarget{Provider: "openai_compatible", BaseURL: longBaseURL, Model: "model"}
	}
	if size := len(mustJSON(t, state)); size >= MaxConfigSnapshotBytes {
		t.Fatalf("test initial snapshot too large: %d", size)
	}
	repository := &configurationTestRepository{state: state, present: true}
	credentials := &configurationTestCredentialAccess{}
	owner, _ := openConfigurationTestOwner(t, repository, credentials)
	result := owner.UpdateTarget(context.Background(), trust.OperatorPrincipal{}, providerTargetUpdate(trust.NewID(), "model-b", "synthetic-secret"))
	if !matchesOperationError(result, operation.Limited, "configuration_snapshot_capacity") || credentials.writeCount() != 0 || repository.writes != 0 {
		t.Fatalf("oversized pending snapshot was persisted or key was written: result=%#v writes=%d saves=%d", result, credentials.writeCount(), repository.writes)
	}
}

func mustJSON(t *testing.T, value any) []byte {
	t.Helper()
	encoded, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	return encoded
}
