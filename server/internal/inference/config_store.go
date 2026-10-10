package inference

import (
	"encoding/json"
	"errors"
	"fmt"
	"strings"

	"floe/server/internal/trust"
)

var ErrUnsupportedConfigVersion = errors.New("unsupported inference configuration schema version")

func unsupportedConfigVersion(version int) error {
	return fmt.Errorf("%w: %d", ErrUnsupportedConfigVersion, version)
}

// ProviderProfile is the Inference-owned persisted provider configuration.
type ProviderProfile struct {
	BaseURL   string                  `json:"base_url"`
	APIKeyEnv string                  `json:"api_key_env,omitempty"`
	Purposes  map[string]PurposeModel `json:"purposes"`
}

// ConfigState is the complete Inference persistence unit. A repository commit
// replaces it atomically; Inference validates and adopts it only after commit.
type ConfigState struct {
	SchemaVersion int                        `json:"schema_version"`
	Revision      uint64                     `json:"revision"`
	Targets       map[string]ProviderTarget  `json:"targets"`
	Routes        map[Purpose]PurposeRoute   `json:"routes"`
	Providers     map[string]ProviderProfile `json:"providers"`
	Pending       *CredentialTransition      `json:"pending,omitempty"`
	OwnedSlots    []string                   `json:"owned_slots"`
	Cleanup       []CredentialCleanup        `json:"cleanup"`
	Receipts      []ConfigurationReceipt     `json:"receipts"`
}

// UnmarshalJSON delegates to the strict current-schema decoder.
func (state *ConfigState) UnmarshalJSON(data []byte) error {
	decoded, err := DecodeConfigState(data)
	if err != nil {
		return err
	}
	*state = decoded
	return nil
}

// DecodeConfigState accepts only the current schema. Unsupported persisted
// versions are left untouched and fail closed for explicit operator recovery.
func DecodeConfigState(data []byte) (ConfigState, error) {
	if len(data) == 0 || len(data) > MaxConfigSnapshotBytes {
		return ConfigState{}, errors.New("invalid inference configuration")
	}
	var marker struct {
		SchemaVersion int `json:"schema_version"`
	}
	if json.Unmarshal(data, &marker) != nil {
		return ConfigState{}, errors.New("invalid inference configuration")
	}
	if marker.SchemaVersion != 3 {
		return ConfigState{}, unsupportedConfigVersion(marker.SchemaVersion)
	}
	type currentConfigState ConfigState
	var current currentConfigState
	if err := trust.DecodeStrict(data, &current, MaxConfigSnapshotBytes, 32); err != nil {
		return ConfigState{}, err
	}
	return ConfigState(current), nil
}

// ConfigContent is a candidate complete live configuration within the owner
// snapshot. It contains references only, never credential values.
type ConfigContent struct {
	Targets   map[string]ProviderTarget  `json:"targets"`
	Routes    map[Purpose]PurposeRoute   `json:"routes"`
	Providers map[string]ProviderProfile `json:"providers"`
}

type CredentialTransition struct {
	OperationID string        `json:"operation_id"`
	Fingerprint string        `json:"fingerprint"`
	Slot        string        `json:"slot"`
	Digest      string        `json:"digest"`
	Candidate   ConfigContent `json:"candidate"`
}

type CredentialCleanup struct {
	Slot              string `json:"slot"`
	RetiredGeneration uint64 `json:"retired_generation"`
}

type ConfigurationReceipt struct {
	OperationID string `json:"operation_id"`
	Fingerprint string `json:"fingerprint"`
	Category    string `json:"category"`
	Code        string `json:"code"`
}

const (
	maxConfigurationReceipts = 64
	maxCredentialCleanup     = 64
	maxOwnedCredentialSlots  = 64
)

func emptyConfigurationState() ConfigState {
	return ConfigState{
		SchemaVersion: 3,
		Targets:       map[string]ProviderTarget{},
		Routes:        map[Purpose]PurposeRoute{},
		Providers:     map[string]ProviderProfile{},
		OwnedSlots:    []string{},
		Cleanup:       []CredentialCleanup{},
		Receipts:      []ConfigurationReceipt{},
	}
}

func validateConfigurationState(state ConfigState, factory ProviderFactory) error {
	if state.SchemaVersion != 3 {
		return unsupportedConfigVersion(state.SchemaVersion)
	}
	if factory == nil || state.Targets == nil || state.Routes == nil || state.Providers == nil || state.OwnedSlots == nil || state.Cleanup == nil || state.Receipts == nil ||
		len(state.Targets) > 32 || len(state.Providers) > 3 || ValidateConfig(InferenceConfig{Routes: state.Routes}) != nil {
		return errors.New("inference configuration unavailable")
	}
	if len(state.Receipts) > maxConfigurationReceipts || len(state.Cleanup) > maxCredentialCleanup || len(state.OwnedSlots) > maxOwnedCredentialSlots || state.Pending != nil && !validTransition(*state.Pending) {
		return errors.New("invalid inference lifecycle state")
	}
	if state.Pending != nil {
		candidate := cloneConfigurationState(ConfigState{SchemaVersion: 3, Targets: state.Pending.Candidate.Targets, Routes: state.Pending.Candidate.Routes, Providers: state.Pending.Candidate.Providers, OwnedSlots: []string{}, Cleanup: []CredentialCleanup{}, Receipts: []ConfigurationReceipt{}})
		if validateConfigurationState(candidate, factory) != nil {
			return errors.New("invalid inference transition candidate")
		}
	}
	owned := make(map[string]bool, len(state.OwnedSlots))
	for _, slot := range state.OwnedSlots {
		if !validOwnedSlot(slot) || owned[slot] {
			return errors.New("invalid inference credential ownership")
		}
		owned[slot] = true
	}
	for _, item := range state.Cleanup {
		if !owned[item.Slot] || item.RetiredGeneration == 0 || contentHasSlot(configurationContent(state), item.Slot) {
			return errors.New("invalid inference credential cleanup")
		}
	}
	for _, slot := range state.OwnedSlots {
		if contentHasSlot(configurationContent(state), slot) {
			continue
		}
		found := false
		for _, item := range state.Cleanup {
			if item.Slot == slot {
				found = true
				break
			}
		}
		if !found {
			return errors.New("unsettled owned inference credential")
		}
	}
	for i, item := range state.Cleanup {
		for _, prior := range state.Cleanup[:i] {
			if prior.Slot == item.Slot {
				return errors.New("duplicate inference cleanup")
			}
		}
	}
	if state.Pending != nil {
		if contentHasSlot(configurationContent(state), state.Pending.Slot) || !contentHasSlot(state.Pending.Candidate, state.Pending.Slot) || owned[state.Pending.Slot] {
			return errors.New("invalid pending inference credential ownership")
		}
	}
	receiptIDs := make(map[string]bool, len(state.Receipts))
	for _, receipt := range state.Receipts {
		if !trust.ValidID(receipt.OperationID) || !validDigest(receipt.Fingerprint) ||
			(receipt.Category != "ready" && receipt.Category != "unavailable") ||
			(receipt.Category == "ready" && receipt.Code != "ok") || (receipt.Category == "unavailable" && receipt.Code != "credential_store_unavailable") {
			return errors.New("invalid inference operation receipt")
		}
		if receiptIDs[receipt.OperationID] {
			return errors.New("duplicate inference operation receipt")
		}
		receiptIDs[receipt.OperationID] = true
	}
	if state.Pending != nil && receiptIDs[state.Pending.OperationID] {
		return errors.New("pending inference operation already settled")
	}
	for _, target := range state.Targets {
		if factory.ValidateTarget(target) != nil {
			return errors.New("invalid inference target")
		}
	}
	for _, profile := range state.Providers {
		if profile.Purposes == nil || len(profile.Purposes) > 3 {
			return errors.New("invalid provider configuration")
		}
		for purpose := range profile.Purposes {
			if !ValidPurpose(purpose) {
				return errors.New("invalid purpose")
			}
		}
	}
	return nil
}

func cloneConfigurationState(state ConfigState) ConfigState {
	out := ConfigState{
		SchemaVersion: state.SchemaVersion,
		Revision:      state.Revision,
		Targets:       make(map[string]ProviderTarget, len(state.Targets)),
		Routes:        make(map[Purpose]PurposeRoute, len(state.Routes)),
		Providers:     make(map[string]ProviderProfile, len(state.Providers)),
		OwnedSlots:    append([]string{}, state.OwnedSlots...),
		Cleanup:       append([]CredentialCleanup{}, state.Cleanup...),
		Receipts:      append([]ConfigurationReceipt{}, state.Receipts...),
	}
	if state.Pending != nil {
		pending := *state.Pending
		pending.Candidate = cloneConfigContent(pending.Candidate)
		out.Pending = &pending
	}
	for id, target := range state.Targets {
		target.BudgetOverride = cloneModelBudgetOverride(target.BudgetOverride)
		out.Targets[id] = target
	}
	for purpose, route := range state.Routes {
		out.Routes[purpose] = route
	}
	for name, profile := range state.Providers {
		copy := profile
		copy.Purposes = make(map[string]PurposeModel, len(profile.Purposes))
		for purpose, configured := range profile.Purposes {
			configured.BudgetOverride = cloneModelBudgetOverride(configured.BudgetOverride)
			copy.Purposes[purpose] = configured
		}
		out.Providers[name] = copy
	}
	return out
}

func configurationContent(state ConfigState) ConfigContent {
	return cloneConfigContent(ConfigContent{Targets: state.Targets, Routes: state.Routes, Providers: state.Providers})
}

func cloneConfigContent(content ConfigContent) ConfigContent {
	state := cloneConfigurationState(ConfigState{SchemaVersion: 3, Targets: content.Targets, Routes: content.Routes, Providers: content.Providers, OwnedSlots: []string{}, Cleanup: []CredentialCleanup{}, Receipts: []ConfigurationReceipt{}})
	return ConfigContent{Targets: state.Targets, Routes: state.Routes, Providers: state.Providers}
}

func withConfigurationContent(state ConfigState, content ConfigContent) ConfigState {
	state.Targets = content.Targets
	state.Routes = content.Routes
	state.Providers = content.Providers
	return state
}

func validTransition(pending CredentialTransition) bool {
	return trust.ValidID(pending.OperationID) && validDigest(pending.Fingerprint) && validOwnedSlot(pending.Slot) && validDigest(pending.Digest) &&
		pending.Candidate.Targets != nil && pending.Candidate.Routes != nil && pending.Candidate.Providers != nil
}

func validOwnedSlot(slot string) bool {
	if len(slot) < len("FLOE_KEY_")+1 || len(slot) > 256 || !strings.HasPrefix(slot, "FLOE_KEY_") {
		return false
	}
	for _, r := range slot {
		if !(r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '_') {
			return false
		}
	}
	return true
}

func validDigest(value string) bool {
	if len(value) != 64 {
		return false
	}
	for _, r := range value {
		if !(r >= '0' && r <= '9' || r >= 'a' && r <= 'f') {
			return false
		}
	}
	return true
}

func configurationStateFitsBound(state ConfigState) bool {
	data, err := json.Marshal(state)
	return err == nil && len(data) <= MaxConfigSnapshotBytes
}
