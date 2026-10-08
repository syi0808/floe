package modelcatalog

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"floe/server/internal/storage"
)

func catalogBytes(t *testing.T, revision uint64, version, modelID string) []byte {
	t.Helper()
	data, err := json.Marshal(Catalog{
		SchemaVersion: 1,
		Revision:      revision,
		Version:       version,
		Providers: []Provider{{
			ProviderID: "codex_oauth",
			Models: []Model{{
				ModelID: modelID,
				Source:  "migrated_repository_suggestions",
			}},
		}},
	})
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func writeCatalog(t *testing.T, path string, data []byte) {
	t.Helper()
	if err := storage.WritePrivate(path, data); err != nil {
		t.Fatalf("write private catalog: %v", err)
	}
}

func readCatalog(t *testing.T, path string) []byte {
	t.Helper()
	data, err := readCatalogFile(path)
	if err != nil {
		t.Fatalf("read private catalog: %v", err)
	}
	return data
}

func TestBootstrapAndExampleAreDataOnlyMigratedSuggestions(t *testing.T) {
	bootstrapBytes, err := os.ReadFile("bootstrap.json")
	if err != nil {
		t.Fatal(err)
	}
	bootstrap, err := Parse(bootstrapBytes)
	if err != nil {
		t.Fatalf("parse bootstrap: %v", err)
	}
	if bootstrap.Revision != 1 || len(bootstrap.Providers) != 1 || bootstrap.Providers[0].ProviderID != "codex_oauth" {
		t.Fatalf("unexpected bootstrap catalog identity: %#v", bootstrap)
	}
	for _, model := range bootstrap.Providers[0].Models {
		if model.Source != "migrated_repository_suggestions" || model.Metadata != nil {
			t.Fatalf("bootstrap implies unverified metadata: %#v", model)
		}
	}

	exampleBytes, err := os.ReadFile("example.json")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := Parse(exampleBytes); err != nil {
		t.Fatalf("parse example catalog: %v", err)
	}

	schemaBytes, err := os.ReadFile("schema.json")
	if err != nil {
		t.Fatal(err)
	}
	var schema map[string]any
	if err := json.Unmarshal(schemaBytes, &schema); err != nil {
		t.Fatalf("parse JSON Schema: %v", err)
	}
	if schema["$schema"] != "https://json-schema.org/draft/2020-12/schema" {
		t.Fatalf("unexpected JSON Schema dialect: %#v", schema["$schema"])
	}
}

func TestParseAcceptsAdditiveUnknownFields(t *testing.T) {
	data := []byte(`{
      "schema_version": 1,
      "revision": 2,
      "version": "additive-fields",
      "future_catalog_field": {"shape": "unknown"},
      "providers": [{
        "provider_id": "codex_oauth",
        "future_provider_field": true,
        "models": [{
          "model_id": "gpt-5.5",
          "source": "migrated_repository_suggestions",
          "future_model_field": [1, 2, 3]
        }]
      }]
    }`)
	catalog, err := Parse(data)
	if err != nil {
		t.Fatalf("additive fields should be ignored by schema v1: %v", err)
	}
	if catalog.Providers[0].Models[0].ModelID != "gpt-5.5" {
		t.Fatalf("known fields were not decoded: %#v", catalog)
	}

	path := filepath.Join(t.TempDir(), FileName)
	if err := Install(path, data); err != nil {
		t.Fatalf("install unknown additive fields: %v", err)
	}
	if got := readCatalog(t, path); !bytes.Equal(got, data) {
		t.Fatalf("installer rewrote additive input fields: %s", got)
	}
}

func TestParseRejectsInvalidSchemaDuplicatesAndUnverifiedMetadata(t *testing.T) {
	valid := string(catalogBytes(t, 2, "v2", "model-a"))
	timestamp := "2030-01-02T03:04:05Z"
	provenance := `"provenance":{"source":"official-provider-documentation","verified_at":"` + timestamp + `"}`
	tests := []struct {
		name string
		data []byte
	}{
		{name: "wrong schema version", data: []byte(strings.Replace(valid, `"schema_version":1`, `"schema_version":2`, 1))},
		{name: "duplicate provider id", data: []byte(`{"schema_version":1,"revision":2,"version":"dup","providers":[{"provider_id":"codex_oauth","models":[{"model_id":"a","source":"migrated"}]},{"provider_id":"codex_oauth","models":[{"model_id":"b","source":"migrated"}]}]}`)},
		{name: "duplicate model id", data: []byte(`{"schema_version":1,"revision":2,"version":"dup","providers":[{"provider_id":"codex_oauth","models":[{"model_id":"a","source":"migrated"},{"model_id":"a","source":"migrated"}]}]}`)},
		{name: "metadata without provenance", data: []byte(`{"schema_version":1,"revision":2,"version":"unverified","providers":[{"provider_id":"codex_oauth","models":[{"model_id":"a","source":"migrated","metadata":{"context_window":100}}]}]}`)},
		{name: "output exceeds known context", data: []byte(`{"schema_version":1,"revision":2,"version":"inconsistent","providers":[{"provider_id":"codex_oauth","models":[{"model_id":"a","source":"migrated","metadata":{"context_window":100,"max_output_tokens":101,` + provenance + `}}]}]}`)},
		{name: "duplicate capability", data: []byte(`{"schema_version":1,"revision":2,"version":"duplicate-capability","providers":[{"provider_id":"codex_oauth","models":[{"model_id":"a","source":"migrated","metadata":{"capabilities":["chat","chat"],` + provenance + `}}]}]}`)},
		{name: "invalid JSON", data: []byte(`{"schema_version":1`)},
		{name: "oversized", data: bytes.Repeat([]byte("x"), MaxCatalogBytes+1)},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if _, err := Parse(test.data); !errors.Is(err, ErrInvalidCatalog) {
				t.Fatalf("Parse error = %v, want ErrInvalidCatalog", err)
			}
		})
	}
}

func TestMetadataLeavesUnknownLimitsAbsentAndRequiresProvenance(t *testing.T) {
	data := []byte(`{"schema_version":1,"revision":2,"version":"known-output","providers":[{"provider_id":"codex_oauth","models":[{"model_id":"m","source":"migrated","metadata":{"max_output_tokens":100,"provenance":{"source":"official-provider-documentation","verified_at":"2030-01-02T03:04:05Z"}}}]}]}`)
	catalog, err := Parse(data)
	if err != nil {
		t.Fatalf("parse provenance-bound optional output metadata: %v", err)
	}
	metadata := catalog.Providers[0].Models[0].Metadata
	if metadata.ContextWindow != nil || metadata.MaxOutputTokens == nil || *metadata.MaxOutputTokens != 100 {
		t.Fatalf("unknown context limit was not preserved as unknown: %#v", metadata)
	}
}

func TestInstallValidatesBeforeAtomicReplacementAndRetainsPrevious(t *testing.T) {
	path := filepath.Join(t.TempDir(), FileName)
	first := catalogBytes(t, 2, "v2", "model-two")
	if err := Install(path, first); err != nil {
		t.Fatalf("install first catalog: %v", err)
	}
	before := readCatalog(t, path)
	previousBefore := readCatalog(t, PreviousPath(path))
	invalid := []byte(`{"schema_version":1,"revision":3,"version":"invalid","providers":[]}`)
	if err := Install(path, invalid); !errors.Is(err, ErrInvalidCatalog) {
		t.Fatalf("invalid install error = %v, want ErrInvalidCatalog", err)
	}
	if got := readCatalog(t, path); !bytes.Equal(got, before) {
		t.Fatalf("invalid candidate changed current catalog: %s", got)
	}
	if got := readCatalog(t, PreviousPath(path)); !bytes.Equal(got, previousBefore) {
		t.Fatalf("invalid candidate changed previous snapshot: %s", got)
	}

	second := catalogBytes(t, 3, "v3", "model-three")
	if err := Install(path, second); err != nil {
		t.Fatalf("install second catalog: %v", err)
	}
	if got := readCatalog(t, PreviousPath(path)); !bytes.Equal(got, before) {
		t.Fatalf("previous snapshot does not retain prior catalog: %s", got)
	}
	if got := readCatalog(t, path); !bytes.Equal(got, second) {
		t.Fatalf("current snapshot does not contain installed catalog: %s", got)
	}
	if err := Install(path, first); !errors.Is(err, ErrStaleCatalog) {
		t.Fatalf("stale installation error = %v, want ErrStaleCatalog", err)
	}
}

func TestValidateAndInstallReadLocalCatalogFile(t *testing.T) {
	validated, err := ValidateFile("example.json")
	if err != nil {
		t.Fatalf("validate local example file: %v", err)
	}
	if validated.Revision != 2 || validated.Version != "example-revision-2" {
		t.Fatalf("unexpected validated example identity: %#v", validated)
	}
	path := filepath.Join(t.TempDir(), FileName)
	if err := InstallFile(path, "example.json"); err != nil {
		t.Fatalf("install local example file: %v", err)
	}
	installed, err := Parse(readCatalog(t, path))
	if err != nil || installed.Revision != validated.Revision {
		t.Fatalf("installed file differs from validated input: %#v, %v", installed, err)
	}
}

func TestReloadKeepsLastKnownGoodAndReportsFailureTime(t *testing.T) {
	path := filepath.Join(t.TempDir(), FileName)
	installed := catalogBytes(t, 2, "v2", "selected-model")
	if err := Install(path, installed); err != nil {
		t.Fatal(err)
	}
	store, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	initial := store.Projection()
	if initial.Catalog.Revision != 2 || initial.Status.Source != "file" || initial.Status.LastSuccessAt == nil {
		t.Fatalf("unexpected initial projection: %#v", initial)
	}

	partial := []byte(`{"schema_version":1,"revision":3`)
	writeCatalog(t, path, partial)
	if err := store.Reload(); !errors.Is(err, ErrInvalidCatalog) {
		t.Fatalf("partial reload error = %v, want ErrInvalidCatalog", err)
	}
	assertRetainedStatus(t, store, "selected-model", RefreshErrorInvalid, initial.Status.LastSuccessAt)

	duplicate := []byte(`{"schema_version":1,"revision":3,"version":"duplicate","providers":[{"provider_id":"codex_oauth","models":[{"model_id":"a","source":"migrated"},{"model_id":"a","source":"migrated"}]}]}`)
	writeCatalog(t, path, duplicate)
	if err := store.Reload(); !errors.Is(err, ErrInvalidCatalog) {
		t.Fatalf("duplicate reload error = %v, want ErrInvalidCatalog", err)
	}
	assertRetainedStatus(t, store, "selected-model", RefreshErrorInvalid, initial.Status.LastSuccessAt)

	writeCatalog(t, path, bytes.Repeat([]byte("x"), MaxCatalogBytes+1))
	if err := store.Reload(); !errors.Is(err, ErrInvalidCatalog) {
		t.Fatalf("oversized reload error = %v, want ErrInvalidCatalog", err)
	}
	assertRetainedStatus(t, store, "selected-model", RefreshErrorInvalid, initial.Status.LastSuccessAt)

	writeCatalog(t, path, catalogBytes(t, 1, "stale", "older-model"))
	if err := store.Reload(); !errors.Is(err, ErrStaleCatalog) {
		t.Fatalf("stale reload error = %v, want ErrStaleCatalog", err)
	}
	assertRetainedStatus(t, store, "selected-model", RefreshErrorStale, initial.Status.LastSuccessAt)

	if err := os.Remove(path); err != nil {
		t.Fatal(err)
	}
	if err := store.Reload(); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("missing reload error = %v, want os.ErrNotExist", err)
	}
	projection := store.Projection()
	if projection.Catalog.Revision != 2 || projection.Status.LastError != RefreshErrorMissing || projection.Status.LastAttemptAt == nil {
		t.Fatalf("missing reload did not retain catalog/report status: %#v", projection)
	}
	statusJSON, err := json.Marshal(projection.Status)
	if err != nil {
		t.Fatal(err)
	}
	if bytes.Contains(statusJSON, []byte(path)) || bytes.Contains(statusJSON, []byte("selected-model")) {
		t.Fatalf("refresh status exposed a path or catalog data: %s", statusJSON)
	}
}

func assertRetainedStatus(t *testing.T, store *Store, modelID, errorCode string, lastSuccess *time.Time) {
	t.Helper()
	projection := store.Projection()
	if projection.Catalog.Revision != 2 || projection.Catalog.Providers[0].Models[0].ModelID != modelID {
		t.Fatalf("failed reload replaced the known-good catalog: %#v", projection.Catalog)
	}
	if projection.Status.LastError != errorCode || projection.Status.LastAttemptAt == nil || projection.Status.LastErrorAt == nil {
		t.Fatalf("failure status missing safe category/timestamp: %#v", projection.Status)
	}
	if lastSuccess == nil || projection.Status.LastSuccessAt == nil || !projection.Status.LastSuccessAt.Equal(*lastSuccess) {
		t.Fatalf("failure changed last successful load time: %#v", projection.Status)
	}
}

func TestRestartUsesValidPreviousWhenCurrentIsMalformed(t *testing.T) {
	path := filepath.Join(t.TempDir(), FileName)
	if err := Install(path, catalogBytes(t, 2, "v2", "last-known-good")); err != nil {
		t.Fatal(err)
	}
	if err := Install(path, catalogBytes(t, 3, "v3", "newer-model")); err != nil {
		t.Fatal(err)
	}
	writeCatalog(t, path, []byte(`{"schema_version":1,"revision":4`))

	store, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	projection := store.Projection()
	if projection.Catalog.Revision != 2 || projection.Catalog.Providers[0].Models[0].ModelID != "last-known-good" {
		t.Fatalf("restart did not restore prior validated catalog: %#v", projection)
	}
	if projection.Status.Source != "previous" || projection.Status.LastError != RefreshErrorInvalid || projection.Status.LastErrorAt == nil {
		t.Fatalf("restart did not expose degraded fallback status: %#v", projection.Status)
	}
}

func TestStartupLockFailureUsesSafeStatusCategory(t *testing.T) {
	path := filepath.Join(t.TempDir(), "missing-profile", FileName)
	store, err := Open(path)
	if err != nil {
		t.Fatalf("open with unavailable profile lock: %v", err)
	}
	projection := store.Projection()
	if projection.Status.LastError != RefreshErrorLock || projection.Status.LastErrorAt == nil {
		t.Fatalf("startup lock failure status = %#v", projection.Status)
	}
	statusJSON, err := json.Marshal(projection.Status)
	if err != nil {
		t.Fatal(err)
	}
	if bytes.Contains(statusJSON, []byte(filepath.Dir(path))) || bytes.Contains(statusJSON, []byte("missing-profile")) {
		t.Fatalf("startup lock failure status exposed the path: %s", statusJSON)
	}
}

func TestRollbackRestoresPriorModelsWithFreshRevision(t *testing.T) {
	path := filepath.Join(t.TempDir(), FileName)
	if err := Install(path, catalogBytes(t, 2, "v2", "prior-model")); err != nil {
		t.Fatal(err)
	}
	if err := Install(path, catalogBytes(t, 3, "v3", "current-model")); err != nil {
		t.Fatal(err)
	}
	if err := Rollback(path); err != nil {
		t.Fatalf("rollback: %v", err)
	}
	current, err := Parse(readCatalog(t, path))
	if err != nil {
		t.Fatal(err)
	}
	previous, err := Parse(readCatalog(t, PreviousPath(path)))
	if err != nil {
		t.Fatal(err)
	}
	if current.Revision != 4 || current.Providers[0].Models[0].ModelID != "prior-model" {
		t.Fatalf("rollback did not restore prior model set at a fresh revision: %#v", current)
	}
	if previous.Revision != 3 || previous.Providers[0].Models[0].ModelID != "current-model" {
		t.Fatalf("rollback did not retain pre-rollback current catalog: %#v", previous)
	}
}

func TestRestartCompletesPendingRollbackAfterCrashWindow(t *testing.T) {
	path := filepath.Join(t.TempDir(), FileName)
	priorData := catalogBytes(t, 2, "v2", "prior-model")
	currentData := catalogBytes(t, 3, "v3", "current-model")
	if err := Install(path, priorData); err != nil {
		t.Fatal(err)
	}
	if err := Install(path, currentData); err != nil {
		t.Fatal(err)
	}
	prior, err := Parse(priorData)
	if err != nil {
		t.Fatal(err)
	}
	pendingData, err := bumpRollbackRevision(priorData, prior, 4)
	if err != nil {
		t.Fatal(err)
	}
	writeCatalog(t, rollbackPendingPath(path), pendingData)
	writeCatalog(t, PreviousPath(path), currentData)

	store, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	projection := store.Projection()
	if projection.Catalog.Revision != 4 || projection.Catalog.Providers[0].Models[0].ModelID != "prior-model" {
		t.Fatalf("startup did not finish pending rollback: %#v", projection)
	}
	if projection.Status.Source != "file" || projection.Status.LastError != "" {
		t.Fatalf("completed rollback status is unexpected: %#v", projection.Status)
	}
	if _, err := os.Stat(rollbackPendingPath(path)); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("pending rollback marker was not removed: %v", err)
	}
}

func TestRollbackRecoveryFailureRemainsVisibleUntilRecovered(t *testing.T) {
	path := filepath.Join(t.TempDir(), FileName)
	if err := Install(path, catalogBytes(t, 2, "v2", "prior-model")); err != nil {
		t.Fatal(err)
	}
	if err := Install(path, catalogBytes(t, 3, "v3", "current-model")); err != nil {
		t.Fatal(err)
	}
	writeCatalog(t, rollbackPendingPath(path), []byte(`{"schema_version":1,"revision":4`))
	store, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	if got := store.Projection(); got.Catalog.Revision != 3 || got.Status.LastError != RefreshErrorRecovery {
		t.Fatalf("startup lost pending rollback failure status: %#v", got)
	}
	if err := store.Reload(); !errors.Is(err, ErrPendingRollback) {
		t.Fatalf("reload error = %v, want ErrPendingRollback", err)
	}
	if got := store.Projection(); got.Status.LastError != RefreshErrorRecovery || got.Status.LastErrorAt == nil {
		t.Fatalf("failed rollback status was not observable: %#v", got.Status)
	}
	if err := os.Remove(rollbackPendingPath(path)); err != nil {
		t.Fatal(err)
	}
	if err := store.Reload(); err != nil {
		t.Fatalf("reload after removing invalid pending marker: %v", err)
	}
	if got := store.Projection(); got.Status.LastError != "" || got.Status.Source != "file" {
		t.Fatalf("recovered catalog status did not clear: %#v", got.Status)
	}
}

func TestConcurrentReloadsCannotPublishOlderReadAfterNewer(t *testing.T) {
	path := filepath.Join(t.TempDir(), FileName)
	if err := Install(path, catalogBytes(t, 4, "v4", "model-four")); err != nil {
		t.Fatal(err)
	}
	store, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	older := catalogBytes(t, 5, "v5", "model-five")
	newer := catalogBytes(t, 6, "v6", "model-six")
	started := make(chan struct{})
	release := make(chan struct{})
	var reads atomic.Int32
	store.readFile = func(string) ([]byte, error) {
		if reads.Add(1) == 1 {
			close(started)
			<-release
			return older, nil
		}
		return newer, nil
	}
	firstDone := make(chan error, 1)
	secondDone := make(chan error, 1)
	go func() { firstDone <- store.Reload() }()
	<-started
	go func() { secondDone <- store.Reload() }()
	close(release)
	if err := <-firstDone; err != nil {
		t.Fatal(err)
	}
	if err := <-secondDone; err != nil {
		t.Fatal(err)
	}
	projection := store.Projection()
	if projection.Catalog.Revision != 6 || projection.Catalog.Providers[0].Models[0].ModelID != "model-six" {
		t.Fatalf("concurrent reload downgraded the catalog: %#v", projection.Catalog)
	}
}

func TestConcurrentReloadAndInstallPublishesFinalInstalledRevision(t *testing.T) {
	path := filepath.Join(t.TempDir(), FileName)
	if err := Install(path, catalogBytes(t, 2, "v2", "model-two")); err != nil {
		t.Fatal(err)
	}
	store, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	three := filepath.Join(t.TempDir(), "candidate-three.json")
	four := filepath.Join(t.TempDir(), "candidate-four.json")
	if err := os.WriteFile(three, catalogBytes(t, 3, "v3", "model-three"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(four, catalogBytes(t, 4, "v4", "model-four"), 0600); err != nil {
		t.Fatal(err)
	}
	start := make(chan struct{})
	results := make(chan error, 8)
	for _, source := range []string{three, four} {
		source := source
		go func() {
			<-start
			err := InstallFile(path, source)
			if errors.Is(err, ErrStaleCatalog) {
				err = nil
			}
			results <- err
		}()
	}
	for i := 0; i < 6; i++ {
		go func() {
			<-start
			results <- store.Reload()
		}()
	}
	close(start)
	for i := 0; i < 8; i++ {
		if err := <-results; err != nil {
			t.Fatalf("concurrent reload/install failed: %v", err)
		}
	}
	if err := store.Reload(); err != nil {
		t.Fatalf("final reload: %v", err)
	}
	if got := store.Snapshot(); got.Revision != 4 || got.Providers[0].Models[0].ModelID != "model-four" {
		t.Fatalf("concurrent reload/install lost newest revision: %#v", got)
	}
}

func TestPeriodicReloadPublishesLocalFileChanges(t *testing.T) {
	path := filepath.Join(t.TempDir(), FileName)
	store, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	if err := Install(path, catalogBytes(t, 2, "v2", "periodic-model")); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- store.Run(ctx, MinRefreshInterval) }()
	deadline := time.After(4 * time.Second)
	for store.Snapshot().Revision < 2 {
		select {
		case <-deadline:
			cancel()
			t.Fatal("periodic reload did not publish the local catalog")
		case <-time.After(10 * time.Millisecond):
		}
	}
	cancel()
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatalf("Run exit = %v, want context.Canceled", err)
	}
}

func TestCatalogWriterProcessHelper(t *testing.T) {
	if os.Getenv("FLOE_CATALOG_HELPER") != "1" {
		return
	}
	target := os.Getenv("FLOE_CATALOG_TARGET")
	switch os.Getenv("FLOE_CATALOG_MODE") {
	case "hold":
		unlock, err := lockCatalogFile(target)
		if err != nil {
			t.Fatal(err)
		}
		defer unlock()
		if err := os.WriteFile(os.Getenv("FLOE_CATALOG_READY"), []byte("ready"), 0600); err != nil {
			t.Fatal(err)
		}
		deadline := time.Now().Add(8 * time.Second)
		for time.Now().Before(deadline) {
			if _, err := os.Stat(os.Getenv("FLOE_CATALOG_RELEASE")); err == nil {
				return
			}
			time.Sleep(5 * time.Millisecond)
		}
		t.Fatal("test parent did not release catalog lock")
	case "install":
		err := InstallFile(target, os.Getenv("FLOE_CATALOG_SOURCE"))
		if err != nil && !errors.Is(err, ErrStaleCatalog) {
			t.Fatalf("competing install: %v", err)
		}
	default:
		t.Fatalf("unknown catalog helper mode")
	}
}

func TestCompetingCatalogInstallsSerializeAcrossProcesses(t *testing.T) {
	if runtime.GOOS != "linux" && runtime.GOOS != "darwin" {
		t.Skip("catalog writer lock is enabled on Linux and macOS")
	}
	directory := t.TempDir()
	path := filepath.Join(directory, FileName)
	if err := Install(path, catalogBytes(t, 2, "v2", "base-model")); err != nil {
		t.Fatal(err)
	}
	sourceThree := filepath.Join(directory, "candidate-three.json")
	sourceFour := filepath.Join(directory, "candidate-four.json")
	if err := os.WriteFile(sourceThree, catalogBytes(t, 3, "v3", "model-three"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(sourceFour, catalogBytes(t, 4, "v4", "model-four"), 0600); err != nil {
		t.Fatal(err)
	}
	ready := filepath.Join(directory, "lock-ready")
	release := filepath.Join(directory, "lock-release")
	newHelper := func(mode, source string) (*exec.Cmd, *bytes.Buffer) {
		cmd := exec.Command(os.Args[0], "-test.run=^TestCatalogWriterProcessHelper$")
		cmd.Env = append(os.Environ(),
			"FLOE_CATALOG_HELPER=1",
			"FLOE_CATALOG_MODE="+mode,
			"FLOE_CATALOG_TARGET="+path,
			"FLOE_CATALOG_SOURCE="+source,
			"FLOE_CATALOG_READY="+ready,
			"FLOE_CATALOG_RELEASE="+release,
		)
		var output bytes.Buffer
		cmd.Stdout = &output
		cmd.Stderr = &output
		return cmd, &output
	}
	holder, holderOutput := newHelper("hold", "")
	if err := holder.Start(); err != nil {
		t.Fatal(err)
	}
	defer func() {
		_ = os.WriteFile(release, []byte("release"), 0600)
		_ = holder.Wait()
	}()
	deadline := time.Now().Add(5 * time.Second)
	for {
		if _, err := os.Stat(ready); err == nil {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("writer helper did not acquire lock: %s", holderOutput.String())
		}
		time.Sleep(5 * time.Millisecond)
	}
	first, firstOutput := newHelper("install", sourceThree)
	second, secondOutput := newHelper("install", sourceFour)
	if err := first.Start(); err != nil {
		t.Fatal(err)
	}
	if err := second.Start(); err != nil {
		t.Fatal(err)
	}
	firstDone, secondDone := make(chan error, 1), make(chan error, 1)
	go func() { firstDone <- first.Wait() }()
	go func() { secondDone <- second.Wait() }()
	select {
	case err := <-firstDone:
		t.Fatalf("first installer passed held cross-process lock (err=%v, output=%s)", err, firstOutput.String())
	case err := <-secondDone:
		t.Fatalf("second installer passed held cross-process lock (err=%v, output=%s)", err, secondOutput.String())
	case <-time.After(100 * time.Millisecond):
	}
	if err := os.WriteFile(release, []byte("release"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := holder.Wait(); err != nil {
		t.Fatalf("lock holder failed: %v: %s", err, holderOutput.String())
	}
	if err := <-firstDone; err != nil {
		t.Fatalf("first competing install failed: %v: %s", err, firstOutput.String())
	}
	if err := <-secondDone; err != nil {
		t.Fatalf("second competing install failed: %v: %s", err, secondOutput.String())
	}
	current, err := Parse(readCatalog(t, path))
	if err != nil {
		t.Fatal(err)
	}
	if current.Revision != 4 || current.Providers[0].Models[0].ModelID != "model-four" {
		t.Fatalf("lower concurrent install overwrote newer revision: %#v", current)
	}
}

func TestRefreshIntervalIsBounded(t *testing.T) {
	if got, err := ParseRefreshInterval(""); err != nil || got != DefaultRefreshInterval {
		t.Fatalf("default interval = %v, %v", got, err)
	}
	for _, value := range []string{"1s", "24h"} {
		if _, err := ParseRefreshInterval(value); err != nil {
			t.Errorf("accepted interval %q rejected: %v", value, err)
		}
	}
	for _, value := range []string{"0s", "500ms", "24h1m", "nonsense"} {
		if _, err := ParseRefreshInterval(value); err == nil {
			t.Errorf("out-of-bounds interval %q accepted", value)
		}
	}
}

func TestAdditiveFieldsSurviveRollback(t *testing.T) {
	path := filepath.Join(t.TempDir(), FileName)
	prior := []byte(`{"schema_version":1,"revision":2,"version":"v2","future_root":{"x":1},"providers":[{"provider_id":"codex_oauth","future_provider":"y","models":[{"model_id":"prior-model","source":"migrated","future_model":{"z":true}}]}]}`)
	if err := Install(path, prior); err != nil {
		t.Fatal(err)
	}
	if err := Install(path, catalogBytes(t, 3, "v3", "current-model")); err != nil {
		t.Fatal(err)
	}
	if err := Rollback(path); err != nil {
		t.Fatal(err)
	}
	rolledBack := string(readCatalog(t, path))
	for _, field := range []string{"future_root", "future_provider", "future_model"} {
		if !strings.Contains(rolledBack, fmt.Sprintf("%q", field)) {
			t.Errorf("rollback dropped additive field %q: %s", field, rolledBack)
		}
	}
}
