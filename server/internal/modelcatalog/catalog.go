// Package modelcatalog owns operator-facing model suggestions. Catalog entries
// are descriptive data only; inference configuration remains the authority for
// provider endpoints, credentials, selected models, and declared capabilities.
package modelcatalog

import (
	"context"
	"embed"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"unicode"
	"unicode/utf8"

	"floe/server/internal/adapters/storage/privatefiles"
)

const (
	FileName               = "model-catalog.json"
	LastGoodSuffix         = ".last-good"
	MaxCatalogBytes        = 1 << 20
	MaxProviders           = 32
	MaxModelsPerProvider   = 1000
	MaxModelsTotal         = 5000
	MaxCapabilities        = 32
	MinRefreshInterval     = time.Second
	MaxRefreshInterval     = 24 * time.Hour
	DefaultRefreshInterval = 5 * time.Minute
	RefreshErrorMissing    = "missing"
	RefreshErrorInvalid    = "invalid_catalog"
	RefreshErrorStale      = "stale_revision"
	RefreshErrorRead       = "read_failed"
	RefreshErrorPersist    = "persist_failed"
	RefreshErrorLock       = "writer_lock_unavailable"
	RefreshErrorRecovery   = "rollback_recovery_failed"
)

var (
	ErrStaleCatalog               = errors.New("model catalog revision is not newer")
	ErrInvalidCatalog             = errors.New("invalid model catalog")
	ErrCatalogPersistence         = errors.New("model catalog last-good snapshot could not be persisted")
	ErrCatalogLockUnavailable     = errors.New("model catalog writer lock unavailable")
	ErrPendingRollback            = errors.New("pending model catalog rollback could not be recovered")
	ErrUnsupportedCatalogPlatform = errors.New("catalog file operations are unsupported on this platform")
	providerIDPattern             = regexp.MustCompile(`^[a-z][a-z0-9_]{0,63}$`)
	capabilityPattern             = regexp.MustCompile(`^[a-z][a-z0-9_]{0,63}$`)
	installMu                     sync.Mutex
)

//go:embed bootstrap.json
var embedded embed.FS

// Catalog is a forward-compatible, versioned projection of suggestion data.
// Unknown JSON fields are intentionally ignored by this reader and preserved
// by the installer when it stores original input bytes.
type Catalog struct {
	SchemaVersion int        `json:"schema_version"`
	Revision      uint64     `json:"revision"`
	Version       string     `json:"version"`
	Providers     []Provider `json:"providers"`
}

// ProviderID is a catalog namespace, independent of model IDs and target IDs.
type Provider struct {
	ProviderID string  `json:"provider_id"`
	Models     []Model `json:"models"`
}

type Model struct {
	ModelID     string         `json:"model_id"`
	DisplayName string         `json:"display_name,omitempty"`
	Deprecated  bool           `json:"deprecated,omitempty"`
	Source      string         `json:"source"`
	Metadata    *ModelMetadata `json:"metadata,omitempty"`
}

// ModelMetadata is optional and informational only. It is never copied into
// provider configuration or used to authorize a model capability.
type ModelMetadata struct {
	ContextWindow   *uint32             `json:"context_window,omitempty"`
	MaxOutputTokens *uint32             `json:"max_output_tokens,omitempty"`
	Capabilities    []string            `json:"capabilities,omitempty"`
	Provenance      *MetadataProvenance `json:"provenance,omitempty"`
}

type MetadataProvenance struct {
	Source     string `json:"source"`
	VerifiedAt string `json:"verified_at"`
}

// RefreshStatus contains stable error categories only. It deliberately omits
// paths, source values, and raw parser or filesystem error text.
type RefreshStatus struct {
	Source        string     `json:"source"`
	Version       string     `json:"version"`
	Revision      uint64     `json:"revision"`
	LastAttemptAt *time.Time `json:"last_attempt_at,omitempty"`
	LastSuccessAt *time.Time `json:"last_success_at,omitempty"`
	LastError     string     `json:"last_error,omitempty"`
	LastErrorAt   *time.Time `json:"last_error_at,omitempty"`
}

type Projection struct {
	Catalog Catalog       `json:"catalog"`
	Status  RefreshStatus `json:"status"`
}

type published struct {
	catalog Catalog
	status  RefreshStatus
}

type Store struct {
	path     string
	reloadMu sync.Mutex
	readFile func(string) ([]byte, error)
	current  atomic.Pointer[published]
}

func Open(path string) (*Store, error) {
	bootstrapBytes, err := embedded.ReadFile("bootstrap.json")
	if err != nil {
		return nil, errors.New("model catalog bootstrap unavailable")
	}
	bootstrap, err := Parse(bootstrapBytes)
	if err != nil {
		return nil, fmt.Errorf("invalid embedded model catalog: %w", err)
	}
	store := &Store{path: path, readFile: readCatalogFile}
	store.current.Store(&published{catalog: bootstrap, status: RefreshStatus{
		Source: "bootstrap", Version: bootstrap.Version, Revision: bootstrap.Revision,
	}})
	if path == "" {
		return store, nil
	}

	var startupErr error
	unlock, lockErr := lockCatalogFile(path)
	if lockErr != nil {
		startupErr = fmt.Errorf("%w: %v", ErrCatalogLockUnavailable, lockErr)
		store.loadStartupFallback(bootstrap, bootstrapBytes, startupErr, false)
		return store, nil
	} else {
		startupErr = recoverRollbackLocked(path)
	}
	store.loadStartupFallback(bootstrap, bootstrapBytes, startupErr, startupErr == nil)
	unlock()
	return store, nil
}

type catalogSnapshot struct {
	catalog Catalog
	data    []byte
	source  string
}

func (s *Store) loadStartupFallback(bootstrap Catalog, bootstrapData []byte, startupErr error, canPersist bool) {
	now := time.Now().UTC()
	selected := &catalogSnapshot{catalog: bootstrap, data: bootstrapData, source: "bootstrap"}
	var currentSnapshot, lastGoodSnapshot *catalogSnapshot
	currentSnapshot, currentErr := loadCatalogSnapshot(s.path, "file", bootstrap)
	lastGoodSnapshot, lastGoodErr := loadCatalogSnapshot(LastGoodPath(s.path), "last_good", bootstrap)
	previousSnapshot, _ := loadCatalogSnapshot(PreviousPath(s.path), "previous", bootstrap)
	for _, candidate := range []*catalogSnapshot{lastGoodSnapshot, previousSnapshot, currentSnapshot} {
		if candidate != nil && betterSnapshot(candidate, selected) {
			selected = candidate
		}
	}
	if currentSnapshot != nil && (selected.catalog.Revision > currentSnapshot.catalog.Revision ||
		selected.catalog.Revision == currentSnapshot.catalog.Revision && !reflect.DeepEqual(selected.catalog, currentSnapshot.catalog)) {
		currentErr = ErrStaleCatalog
	}

	if startupErr == nil && canPersist {
		if err := persistLastGoodLocked(s.path, selected.data, selected.catalog); err != nil {
			startupErr = err
			if lastGoodSnapshot != nil {
				selected = lastGoodSnapshot
			} else {
				selected = &catalogSnapshot{catalog: bootstrap, data: bootstrapData, source: "bootstrap"}
			}
		}
	}
	degradedErr := startupErr
	if degradedErr == nil {
		degradedErr = currentErr
	}
	if degradedErr == nil && selected.source == "bootstrap" && lastGoodErr != nil && !errors.Is(lastGoodErr, os.ErrNotExist) {
		degradedErr = lastGoodErr
	}
	s.publishLoaded(selected.catalog, selected.source, now, degradedErr)
}

func loadCatalogSnapshot(path, source string, bootstrap Catalog) (*catalogSnapshot, error) {
	data, err := readCatalogFile(path)
	if err != nil {
		return nil, err
	}
	catalog, err := Parse(data)
	if err != nil {
		return nil, err
	}
	if !acceptableOver(catalog, bootstrap) {
		return nil, ErrStaleCatalog
	}
	return &catalogSnapshot{catalog: catalog, data: data, source: source}, nil
}

func betterSnapshot(candidate, current *catalogSnapshot) bool {
	if candidate.catalog.Revision != current.catalog.Revision {
		return candidate.catalog.Revision > current.catalog.Revision
	}
	if reflect.DeepEqual(candidate.catalog, current.catalog) {
		return equalRevisionPriority(candidate.source) > equalRevisionPriority(current.source)
	}
	return conflictingRevisionPriority(candidate.source) > conflictingRevisionPriority(current.source)
}

func equalRevisionPriority(source string) int {
	switch source {
	case "file":
		return 4
	case "last_good":
		return 3
	case "previous":
		return 2
	default:
		return 1
	}
}

func conflictingRevisionPriority(source string) int {
	switch source {
	case "last_good":
		return 4
	case "previous":
		return 3
	case "file":
		return 2
	default:
		return 1
	}
}

func acceptableOver(candidate, bootstrap Catalog) bool {
	if candidate.Revision > bootstrap.Revision {
		return true
	}
	return candidate.Revision == bootstrap.Revision && reflect.DeepEqual(candidate, bootstrap)
}

func (s *Store) publishLoaded(catalog Catalog, source string, now time.Time, degradedErr error) {
	status := RefreshStatus{Source: source, Version: catalog.Version, Revision: catalog.Revision}
	status.LastSuccessAt = timePtr(now)
	status.LastAttemptAt = timePtr(now)
	if degradedErr != nil {
		status.LastError = refreshErrorCode(degradedErr)
		status.LastErrorAt = timePtr(now)
	}
	s.current.Store(&published{catalog: *cloneCatalog(catalog), status: status})
}

func (s *Store) recordStartupFailure(code string, now time.Time) {
	previous := s.current.Load()
	if previous == nil {
		return
	}
	status := cloneStatus(previous.status)
	status.LastAttemptAt = timePtr(now)
	status.LastError = code
	status.LastErrorAt = timePtr(now)
	s.current.Store(&published{catalog: *cloneCatalog(previous.catalog), status: status})
}

// Parse validates v1 fields and allows additive unknown fields. Incompatible
// changes require a schema_version change; unknown additions are ignored here.
func Parse(data []byte) (Catalog, error) {
	var catalog Catalog
	if len(data) == 0 || len(data) > MaxCatalogBytes || !utf8.Valid(data) {
		return catalog, ErrInvalidCatalog
	}
	if err := json.Unmarshal(data, &catalog); err != nil {
		return Catalog{}, ErrInvalidCatalog
	}
	if catalog.SchemaVersion != 1 || catalog.Revision == 0 || !validText(catalog.Version, 128) || len(catalog.Providers) == 0 || len(catalog.Providers) > MaxProviders {
		return Catalog{}, ErrInvalidCatalog
	}
	providerIDs := make(map[string]struct{}, len(catalog.Providers))
	modelCount := 0
	for i := range catalog.Providers {
		provider := &catalog.Providers[i]
		if !providerIDPattern.MatchString(provider.ProviderID) || len(provider.Models) == 0 || len(provider.Models) > MaxModelsPerProvider {
			return Catalog{}, ErrInvalidCatalog
		}
		if _, exists := providerIDs[provider.ProviderID]; exists {
			return Catalog{}, ErrInvalidCatalog
		}
		providerIDs[provider.ProviderID] = struct{}{}
		modelIDs := make(map[string]struct{}, len(provider.Models))
		for j := range provider.Models {
			model := &provider.Models[j]
			if !validModelID(model.ModelID) || !validText(model.Source, 512) || model.DisplayName != "" && !validText(model.DisplayName, 256) || !validMetadata(model.Metadata) {
				return Catalog{}, ErrInvalidCatalog
			}
			if _, exists := modelIDs[model.ModelID]; exists {
				return Catalog{}, ErrInvalidCatalog
			}
			modelIDs[model.ModelID] = struct{}{}
			modelCount++
			if modelCount > MaxModelsTotal {
				return Catalog{}, ErrInvalidCatalog
			}
		}
	}
	return catalog, nil
}

func validModelID(value string) bool {
	if value == "" || len(value) > 128 || strings.TrimSpace(value) != value || !utf8.ValidString(value) {
		return false
	}
	for _, r := range value {
		if unicode.IsControl(r) || unicode.IsSpace(r) {
			return false
		}
	}
	return true
}

func validText(value string, limit int) bool {
	if value == "" || len(value) > limit || strings.TrimSpace(value) != value || !utf8.ValidString(value) {
		return false
	}
	for _, r := range value {
		if unicode.IsControl(r) {
			return false
		}
	}
	return true
}

func validMetadata(metadata *ModelMetadata) bool {
	if metadata == nil {
		return true
	}
	if metadata.ContextWindow == nil && metadata.MaxOutputTokens == nil && len(metadata.Capabilities) == 0 {
		return false
	}
	if metadata.ContextWindow != nil && (*metadata.ContextWindow == 0 || *metadata.ContextWindow > 10_000_000) {
		return false
	}
	if metadata.MaxOutputTokens != nil && (*metadata.MaxOutputTokens == 0 || *metadata.MaxOutputTokens > 10_000_000) {
		return false
	}
	if metadata.ContextWindow != nil && metadata.MaxOutputTokens != nil && *metadata.MaxOutputTokens > *metadata.ContextWindow {
		return false
	}
	if len(metadata.Capabilities) > MaxCapabilities || metadata.Provenance == nil || !validText(metadata.Provenance.Source, 512) || !validTimestamp(metadata.Provenance.VerifiedAt) {
		return false
	}
	seen := make(map[string]struct{}, len(metadata.Capabilities))
	for _, capability := range metadata.Capabilities {
		if !capabilityPattern.MatchString(capability) {
			return false
		}
		if _, exists := seen[capability]; exists {
			return false
		}
		seen[capability] = struct{}{}
	}
	return true
}

func validTimestamp(value string) bool {
	_, err := time.Parse(time.RFC3339, value)
	return err == nil
}

func (s *Store) Projection() Projection {
	if s == nil || s.current.Load() == nil {
		return Projection{}
	}
	value := s.current.Load()
	return Projection{Catalog: *cloneCatalog(value.catalog), Status: cloneStatus(value.status)}
}

func (s *Store) Snapshot() Catalog { return s.Projection().Catalog }

// Reload validates and durably records a candidate before publishing it under
// one mutex so a slower read cannot replace a newer accepted revision.
func (s *Store) Reload() error {
	if s == nil || s.path == "" {
		return errors.New("model catalog path unavailable")
	}
	s.reloadMu.Lock()
	defer s.reloadMu.Unlock()
	now := time.Now().UTC()
	unlock, lockErr := lockCatalogFile(s.path)
	if lockErr != nil {
		s.recordReloadFailure(RefreshErrorLock, now)
		return lockErr
	}
	defer unlock()
	if err := recoverRollbackLocked(s.path); err != nil {
		s.recordReloadFailure(refreshErrorCode(err), now)
		return err
	}
	read := s.readFile
	if read == nil {
		read = readCatalogFile
	}
	data, err := read(s.path)
	if err == nil {
		var candidate Catalog
		candidate, err = Parse(data)
		if err == nil {
			current := s.current.Load()
			if current != nil && candidate.Revision <= current.catalog.Revision {
				if candidate.Revision == current.catalog.Revision && reflect.DeepEqual(candidate, current.catalog) {
					if err = persistLastGoodLocked(s.path, data, candidate); err != nil {
						s.recordReloadFailure(refreshErrorCode(err), now)
						return err
					}
					s.publishLoaded(candidate, "file", now, nil)
					return nil
				}
				err = ErrStaleCatalog
			} else {
				if err = persistLastGoodLocked(s.path, data, candidate); err != nil {
					s.recordReloadFailure(refreshErrorCode(err), now)
					return err
				}
				s.publishLoaded(candidate, "file", now, nil)
				return nil
			}
		}
	}
	s.recordReloadFailure(refreshErrorCode(err), now)
	return err
}

func (s *Store) recordReloadFailure(code string, now time.Time) {
	previous := s.current.Load()
	if previous == nil {
		return
	}
	status := cloneStatus(previous.status)
	status.LastAttemptAt = timePtr(now)
	status.LastError = code
	status.LastErrorAt = timePtr(now)
	s.current.Store(&published{catalog: *cloneCatalog(previous.catalog), status: status})
}

func (s *Store) Run(ctx context.Context, interval time.Duration) error {
	if interval < MinRefreshInterval || interval > MaxRefreshInterval {
		return errors.New("model catalog refresh interval out of bounds")
	}
	ticker := time.NewTicker(interval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
			_ = s.Reload()
		}
	}
}

func ParseRefreshInterval(value string) (time.Duration, error) {
	if value == "" {
		return DefaultRefreshInterval, nil
	}
	interval, err := time.ParseDuration(value)
	if err != nil || interval < MinRefreshInterval || interval > MaxRefreshInterval {
		return 0, errors.New("FLOE_MODEL_CATALOG_REFRESH_INTERVAL must be between 1s and 24h")
	}
	return interval, nil
}

// InstallFile validates a local source file before replacing the profile
// catalog. No network request is performed by this command or package.
func InstallFile(path, sourcePath string) error {
	data, err := readExternalBounded(sourcePath)
	if err != nil {
		return err
	}
	return Install(path, data)
}

func ValidateFile(path string) (Catalog, error) {
	data, err := readExternalBounded(path)
	if err != nil {
		return Catalog{}, err
	}
	return Parse(data)
}

func Install(path string, data []byte) error {
	installMu.Lock()
	defer installMu.Unlock()
	unlock, err := lockCatalogFile(path)
	if err != nil {
		return err
	}
	defer unlock()
	if err := recoverRollbackLocked(path); err != nil {
		return err
	}
	return installLocked(path, data)
}

func installLocked(path string, data []byte) error {
	candidate, err := Parse(data)
	if err != nil {
		return err
	}
	bootstrap, err := readBootstrap()
	if err != nil {
		return err
	}
	currentData, currentErr := readCatalogFile(path)
	var current *Catalog
	if currentErr == nil {
		parsed, parseErr := Parse(currentData)
		if parseErr == nil {
			current = &parsed
		}
	} else if !errors.Is(currentErr, os.ErrNotExist) && !errors.Is(currentErr, ErrInvalidCatalog) && !errors.Is(currentErr, storage.ErrIntegrity) {
		return currentErr
	}
	lastGoodData, lastGoodErr := readCatalogFile(LastGoodPath(path))
	var lastGood *Catalog
	if lastGoodErr == nil {
		parsed, parseErr := Parse(lastGoodData)
		if parseErr == nil {
			lastGood = &parsed
		}
	} else if !errors.Is(lastGoodErr, os.ErrNotExist) && !errors.Is(lastGoodErr, ErrInvalidCatalog) && !errors.Is(lastGoodErr, storage.ErrIntegrity) {
		return lastGoodErr
	}
	previousData, previousErr := readCatalogFile(PreviousPath(path))
	var previous *Catalog
	if previousErr == nil {
		parsed, parseErr := Parse(previousData)
		if parseErr == nil {
			previous = &parsed
		}
	} else if !errors.Is(previousErr, os.ErrNotExist) && !errors.Is(previousErr, ErrInvalidCatalog) && !errors.Is(previousErr, storage.ErrIntegrity) {
		return previousErr
	}
	minimumRevision := bootstrap.Revision
	if current != nil && current.Revision > minimumRevision {
		minimumRevision = current.Revision
	}
	if lastGood != nil && lastGood.Revision > minimumRevision {
		minimumRevision = lastGood.Revision
	}
	if previous != nil && previous.Revision > minimumRevision {
		minimumRevision = previous.Revision
	}
	if candidate.Revision <= minimumRevision {
		return ErrStaleCatalog
	}
	if current != nil {
		if err := storage.WritePrivate(PreviousPath(path), currentData); err != nil {
			return err
		}
	} else if previous == nil {
		bootstrapBytes, err := embedded.ReadFile("bootstrap.json")
		if err != nil {
			return errors.New("model catalog bootstrap unavailable")
		}
		if err := storage.WritePrivate(PreviousPath(path), bootstrapBytes); err != nil {
			return err
		}
	}
	return storage.WritePrivate(path, data)
}

// persistLastGoodLocked advances the durable acceptance point without ever
// replacing it with a lower or conflicting revision. Callers hold the OS lock.
func persistLastGoodLocked(path string, data []byte, candidate Catalog) error {
	if len(data) == 0 {
		return ErrInvalidCatalog
	}
	lastGoodPath := LastGoodPath(path)
	lastGoodData, err := readCatalogFile(lastGoodPath)
	if err == nil {
		lastGood, parseErr := Parse(lastGoodData)
		if parseErr == nil {
			switch {
			case candidate.Revision < lastGood.Revision:
				return ErrStaleCatalog
			case candidate.Revision == lastGood.Revision && !reflect.DeepEqual(candidate, lastGood):
				return ErrStaleCatalog
			case candidate.Revision == lastGood.Revision:
				return nil
			}
		}
	} else if !errors.Is(err, os.ErrNotExist) && !errors.Is(err, ErrInvalidCatalog) && !errors.Is(err, storage.ErrIntegrity) {
		return fmt.Errorf("%w: %v", ErrCatalogPersistence, err)
	}
	if err := storage.WritePrivate(lastGoodPath, data); err != nil {
		return fmt.Errorf("%w: %v", ErrCatalogPersistence, err)
	}
	return nil
}

// Rollback installs the prior model set with a fresh revision. A durable
// pending file lets startup finish the operation if the process stops between
// replacing the previous snapshot and the current catalog.
func Rollback(path string) error {
	installMu.Lock()
	defer installMu.Unlock()
	unlock, err := lockCatalogFile(path)
	if err != nil {
		return err
	}
	defer unlock()
	if err := recoverRollbackLocked(path); err != nil {
		return err
	}
	currentData, err := readCatalogFile(path)
	if err != nil {
		return err
	}
	current, err := Parse(currentData)
	if err != nil || current.Revision == ^uint64(0) {
		return ErrInvalidCatalog
	}
	previousData, err := readCatalogFile(PreviousPath(path))
	if err != nil {
		return err
	}
	previous, err := Parse(previousData)
	if err != nil {
		return ErrInvalidCatalog
	}
	maximumRevision := current.Revision
	if previous.Revision > maximumRevision {
		maximumRevision = previous.Revision
	}
	bootstrap, err := readBootstrap()
	if err != nil {
		return err
	}
	if bootstrap.Revision > maximumRevision {
		maximumRevision = bootstrap.Revision
	}
	lastGoodData, lastGoodErr := readCatalogFile(LastGoodPath(path))
	if lastGoodErr == nil {
		lastGood, parseErr := Parse(lastGoodData)
		if parseErr == nil && lastGood.Revision > maximumRevision {
			maximumRevision = lastGood.Revision
		}
	} else if !errors.Is(lastGoodErr, os.ErrNotExist) && !errors.Is(lastGoodErr, ErrInvalidCatalog) && !errors.Is(lastGoodErr, storage.ErrIntegrity) {
		return lastGoodErr
	}
	if maximumRevision == ^uint64(0) {
		return ErrInvalidCatalog
	}
	rollbackData, err := bumpRollbackRevision(previousData, previous, maximumRevision+1)
	if err != nil {
		return err
	}
	rollback, err := Parse(rollbackData)
	if err != nil {
		return ErrInvalidCatalog
	}
	pendingPath := rollbackPendingPath(path)
	if err := storage.WritePrivate(pendingPath, rollbackData); err != nil {
		return err
	}
	if err := storage.WritePrivate(PreviousPath(path), currentData); err != nil {
		return err
	}
	if err := storage.WritePrivate(path, rollbackData); err != nil {
		return err
	}
	if err := persistLastGoodLocked(path, rollbackData, rollback); err != nil {
		return err
	}
	return removePrivateFile(pendingPath)
}

func bumpRollbackRevision(data []byte, previous Catalog, revision uint64) ([]byte, error) {
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return nil, ErrInvalidCatalog
	}
	version := fmt.Sprintf("rollback-r%d-of-%s", revision, previous.Version)
	if len(version) > 128 {
		version = fmt.Sprintf("rollback-r%d", revision)
	}
	fields["revision"], _ = json.Marshal(revision)
	fields["version"], _ = json.Marshal(version)
	rollbackData, err := json.Marshal(fields)
	if err != nil {
		return nil, ErrInvalidCatalog
	}
	rollback, err := Parse(rollbackData)
	if err != nil || rollback.Revision != revision {
		return nil, ErrInvalidCatalog
	}
	return rollbackData, nil
}

func recoverRollbackLocked(path string) error {
	pendingPath := rollbackPendingPath(path)
	pendingData, err := readCatalogFile(pendingPath)
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if err != nil {
		return ErrPendingRollback
	}
	pending, err := Parse(pendingData)
	if err != nil {
		return ErrPendingRollback
	}
	currentData, err := readCatalogFile(path)
	if err != nil {
		return ErrPendingRollback
	}
	current, err := Parse(currentData)
	if err != nil {
		return ErrPendingRollback
	}
	if current.Revision == pending.Revision && reflect.DeepEqual(current, pending) {
		if err := removePrivateFile(pendingPath); err != nil {
			return fmt.Errorf("%w: %v", ErrPendingRollback, err)
		}
		return nil
	}
	if pending.Revision <= current.Revision {
		return ErrPendingRollback
	}
	lastGoodData, lastGoodErr := readCatalogFile(LastGoodPath(path))
	if lastGoodErr == nil {
		lastGood, parseErr := Parse(lastGoodData)
		if parseErr == nil && pending.Revision <= lastGood.Revision {
			return ErrPendingRollback
		}
	} else if !errors.Is(lastGoodErr, os.ErrNotExist) && !errors.Is(lastGoodErr, ErrInvalidCatalog) && !errors.Is(lastGoodErr, storage.ErrIntegrity) {
		return fmt.Errorf("%w: %v", ErrPendingRollback, lastGoodErr)
	}
	if err := storage.WritePrivate(PreviousPath(path), currentData); err != nil {
		return fmt.Errorf("%w: %v", ErrPendingRollback, err)
	}
	if err := storage.WritePrivate(path, pendingData); err != nil {
		return fmt.Errorf("%w: %v", ErrPendingRollback, err)
	}
	if err := removePrivateFile(pendingPath); err != nil {
		return fmt.Errorf("%w: %v", ErrPendingRollback, err)
	}
	return nil
}

func readBootstrap() (Catalog, error) {
	data, err := embedded.ReadFile("bootstrap.json")
	if err != nil {
		return Catalog{}, errors.New("model catalog bootstrap unavailable")
	}
	return Parse(data)
}

func readCatalogFile(path string) ([]byte, error) {
	data, err := storage.ReadPrivate(path, MaxCatalogBytes)
	if err != nil {
		if errors.Is(err, storage.ErrIntegrity) {
			return nil, ErrInvalidCatalog
		}
		return nil, err
	}
	return data, nil
}

func rollbackPendingPath(path string) string { return path + ".rollback-pending" }

func removePrivateFile(path string) error {
	if err := os.Remove(path); err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	dir, err := os.Open(filepath.Dir(path))
	if err != nil {
		return storage.IndeterminateWrite{Cause: err}
	}
	defer dir.Close()
	if err := dir.Sync(); err != nil {
		return storage.IndeterminateWrite{Cause: err}
	}
	return nil
}

func PreviousPath(path string) string { return path + ".previous" }

func LastGoodPath(path string) string { return path + LastGoodSuffix }

func refreshErrorCode(err error) string {
	switch {
	case err == nil:
		return ""
	case errors.Is(err, ErrInvalidCatalog):
		return RefreshErrorInvalid
	case errors.Is(err, ErrCatalogPersistence):
		return RefreshErrorPersist
	case errors.Is(err, ErrStaleCatalog):
		return RefreshErrorStale
	case errors.Is(err, ErrUnsupportedCatalogPlatform):
		return RefreshErrorLock
	case errors.Is(err, ErrCatalogLockUnavailable):
		return RefreshErrorLock
	case errors.Is(err, ErrPendingRollback):
		return RefreshErrorRecovery
	case errors.Is(err, os.ErrNotExist):
		return RefreshErrorMissing
	default:
		return RefreshErrorRead
	}
}

func cloneCatalog(catalog Catalog) *Catalog {
	copy := catalog
	copy.Providers = make([]Provider, len(catalog.Providers))
	for i, provider := range catalog.Providers {
		copy.Providers[i] = Provider{ProviderID: provider.ProviderID, Models: make([]Model, len(provider.Models))}
		for j, model := range provider.Models {
			copy.Providers[i].Models[j] = model
			if model.Metadata != nil {
				metadata := *model.Metadata
				metadata.Capabilities = append([]string(nil), model.Metadata.Capabilities...)
				if model.Metadata.ContextWindow != nil {
					value := *model.Metadata.ContextWindow
					metadata.ContextWindow = &value
				}
				if model.Metadata.MaxOutputTokens != nil {
					value := *model.Metadata.MaxOutputTokens
					metadata.MaxOutputTokens = &value
				}
				if model.Metadata.Provenance != nil {
					provenance := *model.Metadata.Provenance
					metadata.Provenance = &provenance
				}
				copy.Providers[i].Models[j].Metadata = &metadata
			}
		}
	}
	return &copy
}

func cloneStatus(status RefreshStatus) RefreshStatus {
	if status.LastAttemptAt != nil {
		status.LastAttemptAt = timePtr(*status.LastAttemptAt)
	}
	if status.LastSuccessAt != nil {
		status.LastSuccessAt = timePtr(*status.LastSuccessAt)
	}
	if status.LastErrorAt != nil {
		status.LastErrorAt = timePtr(*status.LastErrorAt)
	}
	return status
}

func timePtr(value time.Time) *time.Time { return &value }
