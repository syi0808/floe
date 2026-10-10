package storage

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	privatefiles "floe/server/internal/adapters/storage/privatefiles"
	"floe/server/internal/inference"
)

func inferenceTestFiles(t *testing.T) (*privatefiles.Files, string) {
	t.Helper()
	rootPath := t.TempDir()
	key := []byte("0123456789abcdef0123456789abcdef")
	root, err := privatefiles.NewFiles(rootPath, "inference-config-test", key, true)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(root.Close)
	files, err := root.Scope("inference")
	if err != nil {
		t.Fatal(err)
	}
	return files, rootPath
}

func TestInferenceConfigRepositoryLoadsAbsentStrictSnapshotAndCorruption(t *testing.T) {
	files, rootPath := inferenceTestFiles(t)
	repository := NewInferenceConfigRepository(files)
	if outcome := repository.LoadConfig(); outcome.Disposition != inference.ConfigReadAbsent {
		t.Fatalf("missing config did not remain a valid first-run state: %#v", outcome)
	}

	state := inference.ConfigState{
		SchemaVersion: 3, OwnedSlots: []string{}, Cleanup: []inference.CredentialCleanup{}, Receipts: []inference.ConfigurationReceipt{},
		Targets:   map[string]inference.ProviderTarget{},
		Routes:    map[inference.Purpose]inference.PurposeRoute{},
		Providers: map[string]inference.ProviderProfile{},
	}
	if outcome := repository.SaveConfig(state); outcome.Disposition != inference.ConfigWriteCommitted {
		t.Fatalf("typed config snapshot did not commit: %#v", outcome)
	}
	loaded := repository.LoadConfig()
	if loaded.Disposition != inference.ConfigReadPresent || !reflect.DeepEqual(loaded.State, state) {
		t.Fatalf("typed config snapshot did not round-trip: %#v", loaded)
	}
	legacyData := []byte(`{"schema_version":2,"revision":4,"targets":{"target-a":{"provider":"openai_compatible","base_url":"https://example.invalid","model":"selected-model","capabilities":["chat","tool_proposals"]}},"routes":{},"providers":{},"owned_slots":[],"cleanup":[],"receipts":[]}`)
	if err := files.Write(inferenceConfigurationFile, legacyData); err != nil {
		t.Fatal(err)
	}
	legacy := repository.LoadConfig()
	if legacy.Disposition != inference.ConfigReadInvalid || !errors.Is(legacy.Cause, inference.ErrUnsupportedConfigVersion) {
		t.Fatalf("schema-2 data was not rejected with an explicit unsupported-version cause: %#v", legacy)
	}
	preserved, err := files.Read(inferenceConfigurationFile, inference.MaxConfigSnapshotBytes)
	if err != nil || string(preserved) != string(legacyData) {
		t.Fatalf("unsupported schema-2 file was modified or removed: err=%v bytes=%s", err, preserved)
	}

	if err := files.Write(inferenceConfigurationFile, []byte(`{"schema_version":2,"schema_version":2}`)); err != nil {
		t.Fatal(err)
	}
	if outcome := repository.LoadConfig(); outcome.Disposition != inference.ConfigReadInvalid {
		t.Fatalf("duplicate JSON fields were accepted: %#v", outcome)
	}

	if err := files.Write(inferenceConfigurationFile, []byte(`{"schema_version":2,"targets":{},"routes":{},"providers":{},"unknown":true}`)); err != nil {
		t.Fatal(err)
	}
	if outcome := repository.LoadConfig(); outcome.Disposition != inference.ConfigReadInvalid {
		t.Fatalf("unknown JSON fields were accepted: %#v", outcome)
	}

	if err := files.Write(inferenceConfigurationFile, []byte(`{"schema_version":2,"targets":{},"routes":{},"providers":{},"owned_slots":[],"cleanup":[],"receipts":[]}`)); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(rootPath, "inference", inferenceConfigurationFile), []byte("not an encrypted file"), 0600); err != nil {
		t.Fatal(err)
	}
	if outcome := repository.LoadConfig(); outcome.Disposition != inference.ConfigReadInvalid {
		t.Fatalf("encrypted integrity failure was not classified as invalid: %#v", outcome)
	}
}

func TestInferenceConfigRepositoryWriteOutcomeClassification(t *testing.T) {
	cases := []struct {
		name string
		err  error
		want inference.ConfigWriteDisposition
	}{
		{name: "committed", want: inference.ConfigWriteCommitted},
		{name: "rejected", err: errors.New("synthetic precommit failure"), want: inference.ConfigWriteRejected},
		{name: "indeterminate", err: privatefiles.IndeterminateWrite{Cause: errors.New("synthetic post-rename failure")}, want: inference.ConfigWriteIndeterminate},
		{name: "integrity", err: privatefiles.ErrIntegrity, want: inference.ConfigWriteIntegrityFailure},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			if got := inferenceConfigWriteOutcome(test.err).Disposition; got != test.want {
				t.Fatalf("write error mapped to %v, want %v", got, test.want)
			}
		})
	}
}

func TestInferenceConfigRepositoryJSONRetainsExistingFileShape(t *testing.T) {
	files, _ := inferenceTestFiles(t)
	repository := NewInferenceConfigRepository(files)
	state := inference.ConfigState{
		SchemaVersion: 3, OwnedSlots: []string{}, Cleanup: []inference.CredentialCleanup{}, Receipts: []inference.ConfigurationReceipt{},
		Targets: map[string]inference.ProviderTarget{
			"test-target": {Provider: "openai_compatible", BaseURL: "https://example.invalid", Model: "model", APIKeyEnv: "FLOE_KEY_SYNTHETIC"},
		},
		Routes:    map[inference.Purpose]inference.PurposeRoute{inference.QuickResponse: {TargetID: "test-target", ReasoningEffort: "medium", Enabled: true}},
		Providers: map[string]inference.ProviderProfile{},
	}
	if outcome := repository.SaveConfig(state); outcome.Disposition != inference.ConfigWriteCommitted {
		t.Fatalf("snapshot save failed: %#v", outcome)
	}
	encoded, err := json.Marshal(state)
	if err != nil {
		t.Fatal(err)
	}
	actual, err := files.Read(inferenceConfigurationFile, 65536)
	if err != nil || !reflect.DeepEqual(actual, encoded) {
		t.Fatalf("repository did not preserve the established JSON shape: err=%v json=%s", err, actual)
	}
	if strings.Contains(string(actual), "synthetic-key") {
		t.Fatal("credential value unexpectedly entered the config artifact")
	}
}
