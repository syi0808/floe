package node

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/subtle"
	"encoding/hex"
	"encoding/json"
	"errors"
	"floe/server/internal/credentials"
	"floe/server/internal/storage"
	"io"
	"os"
	"path/filepath"
	"sync"
	"syscall"
)

const rootIdentityName = "storage-identity.json"
const rootSealName = "storage-seal"
const rootLayout = 2 // Owner-scoped authenticated payloads.
const rootSealContent = "Floe encrypted server root owner-scoped v2\n"

const (
	operationProfileOpen   = "profile_open"
	operationProfileReady  = "profile_ready_publish"
	operationAdminToken    = "admin_token_read"
	operationNodeStartup   = "node_startup"
	stageInput             = "input_validation"
	stageProfileDirectory  = "profile_directory"
	stageParentSync        = "parent_directory_sync"
	stageLeaseOpen         = "process_lease_open"
	stageLeaseMetadata     = "process_lease_metadata"
	stageLeaseAcquire      = "process_lease_acquire"
	stageFreshProfile      = "fresh_profile_validation"
	stageIdentity          = "profile_identity"
	stageRootKey           = "root_key"
	stageRootSeal          = "root_seal"
	stageReadyMarker       = "ready_marker"
	stageOwnerStorage      = "owner_storage"
	stageTrustStorage      = "trust_storage"
	stageAdminToken        = "administrator_token"
	stageBuildProfileCheck = "build_profile_check"
)

type StorageFailure struct {
	code      string
	operation string
	stage     string
	cause     error
}

func (e *StorageFailure) Error() string {
	if e == nil {
		return "server encrypted profile unavailable"
	}
	return "server encrypted profile unavailable (" + e.code + "; operation=" + e.operation + "; stage=" + e.stage + "); existing data and keys were preserved"
}
func (e *StorageFailure) Unwrap() error     { return e.cause }
func (e *StorageFailure) Code() string      { return e.code }
func (e *StorageFailure) Operation() string { return e.operation }
func (e *StorageFailure) Stage() string     { return e.stage }
func storageFailure(code, operation, stage string, cause error) error {
	return &StorageFailure{code: code, operation: operation, stage: stage, cause: cause}
}

var errStorage = errors.New("invalid encrypted profile metadata")

func profileStorageFailure(err error, operation, stage string) error {
	switch {
	case errors.Is(err, syscall.EACCES), errors.Is(err, syscall.EPERM):
		return storageFailure("profile_access_denied", operation, stage, err)
	case errors.Is(err, errStorage), errors.Is(err, storage.ErrUnsafePrivateFile), errors.Is(err, storage.ErrIntegrity):
		return storageFailure("profile_invalid", operation, stage, err)
	default:
		return storageFailure("profile_io_failed", operation, stage, err)
	}
}

func profileLeaseFailure(err error) error {
	if errors.Is(err, syscall.EWOULDBLOCK) || errors.Is(err, syscall.EAGAIN) {
		return storageFailure("profile_in_use", operationProfileOpen, stageLeaseAcquire, err)
	}
	return profileStorageFailure(err, operationProfileOpen, stageLeaseAcquire)
}

func profileLeaseOpenFailure(err error) error {
	if errors.Is(err, syscall.ELOOP) {
		return storageFailure("profile_invalid", operationProfileOpen, stageLeaseOpen, err)
	}
	return profileStorageFailure(err, operationProfileOpen, stageLeaseOpen)
}

func rootIdentityCreateFailure(err error) error {
	if storage.IsIndeterminate(err) {
		return storageFailure("creation_incomplete", operationProfileOpen, stageIdentity, err)
	}
	return profileStorageFailure(err, operationProfileOpen, stageIdentity)
}

func readyMarkerFailure(err error) error {
	if storage.IsIndeterminate(err) {
		return storageFailure("creation_incomplete", operationProfileReady, stageReadyMarker, err)
	}
	return profileStorageFailure(err, operationProfileReady, stageReadyMarker)
}

func startupStorageFailure(err error, fallbackCode, stage string, fresh bool) error {
	if fresh {
		return storageFailure("creation_incomplete", operationNodeStartup, stage, err)
	}
	var pathErr *os.PathError
	var linkErr *os.LinkError
	var syscallErr *os.SyscallError
	var errno syscall.Errno
	if errors.Is(err, syscall.EACCES) || errors.Is(err, syscall.EPERM) || errors.Is(err, storage.ErrUnsafePrivateFile) ||
		errors.As(err, &pathErr) || errors.As(err, &linkErr) || errors.As(err, &syscallErr) || errors.As(err, &errno) {
		return profileStorageFailure(err, operationNodeStartup, stage)
	}
	return storageFailure(fallbackCode, operationNodeStartup, stage, err)
}

func custodyFailure(err error, stage string) error {
	switch {
	case errors.Is(err, credentials.ErrLocked):
		return storageFailure("key_locked_or_denied", operationProfileOpen, stage, err)
	case errors.Is(err, credentials.ErrBusy):
		return storageFailure("key_busy", operationProfileOpen, stage, err)
	case errors.Is(err, context.DeadlineExceeded):
		return storageFailure("key_timeout", operationProfileOpen, stage, err)
	case errors.Is(err, context.Canceled):
		return storageFailure("key_cancelled", operationProfileOpen, stage, err)
	default:
		return storageFailure("key_unavailable", operationProfileOpen, stage, err)
	}
}

type rootIdentity struct {
	Format  int    `json:"format"`
	ID      string `json:"id"`
	Profile string `json:"profile"`
	State   string `json:"state"`
}
type admittedStorage struct {
	fresh        bool
	identity     rootIdentity
	identityPath string
	files        *storage.Files
	lock         *os.File
	once         sync.Once
}

func (r *admittedStorage) Close() {
	if r == nil {
		return
	}
	r.once.Do(func() {
		r.files.Close()
		if r.lock != nil {
			_ = syscall.Flock(int(r.lock.Fd()), syscall.LOCK_UN)
			_ = r.lock.Close()
		}
	})
}
func rootSlot(id rootIdentity) string {
	return "floe.server.storage-root.v1/" + id.Profile + "/" + id.ID
}
func decodeRoot(data []byte) (rootIdentity, error) {
	var id rootIdentity
	d := json.NewDecoder(bytes.NewReader(data))
	d.DisallowUnknownFields()
	if d.Decode(&id) != nil || d.Decode(new(any)) != io.EOF || id.Profile != storageProfile {
		return id, storageFailure("profile_invalid", operationProfileOpen, stageIdentity, errStorage)
	}
	if id.Format != rootLayout {
		return id, storageFailure("unsupported_layout", operationProfileOpen, stageIdentity, nil)
	}
	raw, err := hex.DecodeString(id.ID)
	if err != nil || len(raw) != 32 || hex.EncodeToString(raw) != id.ID {
		return id, storageFailure("profile_invalid", operationProfileOpen, stageIdentity, errStorage)
	}
	if id.State == "initializing" {
		return id, storageFailure("creation_incomplete", operationProfileOpen, stageIdentity, nil)
	}
	if id.State != "ready" {
		return id, storageFailure("profile_invalid", operationProfileOpen, stageIdentity, errStorage)
	}
	return id, nil
}
func syncDirectory(path string) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	if err := f.Sync(); err != nil {
		_ = f.Close()
		return err
	}
	return f.Close()
}
func createRootIdentity(path string, data []byte) error {
	f, err := os.OpenFile(path, os.O_WRONLY|os.O_CREATE|os.O_EXCL|syscall.O_NOFOLLOW, 0600)
	if err != nil {
		return err
	}
	n, err := f.Write(data)
	if err == nil && n != len(data) {
		err = io.ErrShortWrite
	}
	if err == nil {
		err = f.Sync()
	}
	closeErr := f.Close()
	if err != nil {
		return storage.IndeterminateWrite{Cause: err}
	}
	if closeErr != nil {
		return storage.IndeterminateWrite{Cause: closeErr}
	}
	if err := syncDirectory(filepath.Dir(path)); err != nil {
		return storage.IndeterminateWrite{Cause: err}
	}
	return nil
}
func openStorage(ctx context.Context, directory string, vault credentials.Store, writable bool) (out *admittedStorage, err error) {
	if ctx == nil || vault == nil || !filepath.IsAbs(directory) || filepath.Clean(directory) != directory {
		return nil, storageFailure("profile_invalid", operationProfileOpen, stageInput, errStorage)
	}
	if writable {
		if err := os.Mkdir(directory, 0700); err != nil && !os.IsExist(err) {
			return nil, profileStorageFailure(err, operationProfileOpen, stageProfileDirectory)
		}
	}
	if err := storage.PrivateDirectory(directory); err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil, storageFailure("profile_invalid", operationProfileOpen, stageProfileDirectory, err)
		}
		return nil, profileStorageFailure(err, operationProfileOpen, stageProfileDirectory)
	}
	if writable {
		if err := syncDirectory(filepath.Dir(directory)); err != nil {
			return nil, profileStorageFailure(err, operationProfileOpen, stageParentSync)
		}
	}
	var lock *os.File
	if writable {
		lock, err = os.OpenFile(filepath.Join(directory, "storage.lock"), os.O_RDWR|os.O_CREATE|syscall.O_NOFOLLOW|syscall.O_NONBLOCK, 0600)
		if err != nil {
			return nil, profileLeaseOpenFailure(err)
		}
		info, e := lock.Stat()
		if e != nil {
			_ = lock.Close()
			return nil, profileStorageFailure(e, operationProfileOpen, stageLeaseMetadata)
		}
		st, ok := infoSys(info)
		if info == nil || !ok || !info.Mode().IsRegular() || info.Mode().Perm()&0077 != 0 || int(st.Uid) != os.Geteuid() || st.Nlink != 1 {
			_ = lock.Close()
			return nil, storageFailure("profile_invalid", operationProfileOpen, stageLeaseMetadata, errStorage)
		}
		if e := syscall.Flock(int(lock.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); e != nil {
			_ = lock.Close()
			return nil, profileLeaseFailure(e)
		}
	}
	defer func() {
		if err != nil && lock != nil {
			_ = syscall.Flock(int(lock.Fd()), syscall.LOCK_UN)
			_ = lock.Close()
		}
	}()
	path := filepath.Join(directory, rootIdentityName)
	data, readErr := storage.ReadPrivate(path, 1024)
	fresh := errors.Is(readErr, os.ErrNotExist)
	var id rootIdentity
	if fresh {
		if !writable {
			return nil, storageFailure("profile_invalid", operationProfileOpen, stageIdentity, readErr)
		}
		if e := freshProfile(directory); e != nil {
			return nil, profileStorageFailure(e, operationProfileOpen, stageFreshProfile)
		}
		random := make([]byte, 32)
		if _, e := rand.Read(random); e != nil {
			return nil, storageFailure("profile_io_failed", operationProfileOpen, stageIdentity, e)
		}
		id = rootIdentity{Format: rootLayout, ID: hex.EncodeToString(random), Profile: storageProfile, State: "initializing"}
	} else {
		if readErr != nil {
			return nil, profileStorageFailure(readErr, operationProfileOpen, stageIdentity)
		}
		id, err = decodeRoot(data)
		if err != nil {
			return nil, err
		}
	}
	value, err := vault.Get(ctx, rootSlot(id))
	if err != nil {
		return nil, custodyFailure(err, stageRootKey)
	}
	if fresh {
		if value != "" {
			return nil, storageFailure("key_slot_occupied", operationProfileOpen, stageRootKey, nil)
		}
		// Get was read-only. Reserve the durable attempt immediately before Put.
		// A locked/busy preflight read cannot strand an untouched profile.
		encoded, _ := json.Marshal(id)
		if e := createRootIdentity(path, encoded); e != nil {
			return nil, rootIdentityCreateFailure(e)
		}
		key := make([]byte, 32)
		if _, e := rand.Read(key); e != nil {
			return nil, storageFailure("profile_io_failed", operationProfileOpen, stageRootKey, e)
		}
		value = hex.EncodeToString(key)
		clear(key)
		if e := vault.Put(ctx, rootSlot(id), value); e != nil {
			return nil, custodyFailure(e, stageRootKey)
		}
		observed, e := vault.Get(ctx, rootSlot(id))
		if e != nil {
			return nil, custodyFailure(e, stageRootKey)
		}
		if subtle.ConstantTimeCompare([]byte(observed), []byte(value)) != 1 {
			return nil, storageFailure("key_readback_mismatch", operationProfileOpen, stageRootKey, nil)
		}
	}
	if value == "" {
		return nil, storageFailure("key_missing", operationProfileOpen, stageRootKey, nil)
	}
	key, err := hex.DecodeString(value)
	if err != nil || len(key) != 32 || hex.EncodeToString(key) != value {
		return nil, storageFailure("key_malformed", operationProfileOpen, stageRootKey, err)
	}
	defer clear(key)
	files, err := storage.NewFiles(directory, id.ID, key, writable)
	if err != nil {
		return nil, profileStorageFailure(err, operationProfileOpen, stageRootSeal)
	}
	defer func() {
		if err != nil {
			files.Close()
		}
	}()
	if fresh {
		if e := files.Write(rootSealName, []byte(rootSealContent)); e != nil {
			return nil, profileStorageFailure(e, operationProfileOpen, stageRootSeal)
		}
	}
	seal, err := files.Read(rootSealName, 128)
	if err != nil {
		switch {
		case errors.Is(err, syscall.EACCES), errors.Is(err, syscall.EPERM), errors.Is(err, storage.ErrUnsafePrivateFile):
			return nil, profileStorageFailure(err, operationProfileOpen, stageRootSeal)
		case errors.Is(err, storage.ErrIntegrity), errors.Is(err, os.ErrNotExist):
			return nil, storageFailure("root_authentication_failed", operationProfileOpen, stageRootSeal, err)
		default:
			return nil, profileStorageFailure(err, operationProfileOpen, stageRootSeal)
		}
	}
	if string(seal) != rootSealContent {
		return nil, storageFailure("root_authentication_failed", operationProfileOpen, stageRootSeal, nil)
	}
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	return &admittedStorage{files: files, lock: lock, fresh: fresh, identity: id, identityPath: path}, nil
}

// Ready means the root seal and the first complete Trust identity are durable.
// Only Node composition can publish it, before exposing any owner to requests.
func (r *admittedStorage) publishReady() error {
	if !r.fresh {
		return nil
	}
	if err := r.files.Available(); err != nil {
		if errors.Is(err, storage.ErrUnavailable) {
			return storageFailure("creation_incomplete", operationProfileReady, stageReadyMarker, err)
		}
		return profileStorageFailure(err, operationProfileReady, stageReadyMarker)
	}
	identity := r.identity
	identity.State = "ready"
	encoded, err := json.Marshal(identity)
	if err != nil {
		return storageFailure("creation_incomplete", operationProfileReady, stageReadyMarker, err)
	}
	if err := storage.WritePrivate(r.identityPath, encoded); err != nil {
		return readyMarkerFailure(err)
	}
	r.identity = identity
	r.fresh = false
	return nil
}

func infoSys(info os.FileInfo) (*syscall.Stat_t, bool) {
	if info == nil {
		return nil, false
	}
	st, ok := info.Sys().(*syscall.Stat_t)
	return st, ok
}

// Explicit operator retrieval is readonly: never start owners or create a profile.
func AdministratorToken(ctx context.Context, directory string) (string, error) {
	vault, err := profileStore(directory, false)
	if err != nil {
		return "", err
	}
	root, err := openStorage(ctx, directory, vault, false)
	if err != nil {
		return "", err
	}
	defer root.Close()
	trustFiles, err := root.files.Scope("trust")
	if err != nil {
		return "", profileStorageFailure(err, operationAdminToken, stageTrustStorage)
	}
	token, err := trustFiles.Read("admin-token", 1024)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) || errors.Is(err, storage.ErrIntegrity) {
			return "", storageFailure("profile_invalid", operationAdminToken, stageAdminToken, err)
		}
		return "", profileStorageFailure(err, operationAdminToken, stageAdminToken)
	}
	if len(token) < 32 {
		return "", storageFailure("profile_invalid", operationAdminToken, stageAdminToken, errStorage)
	}
	return string(token), nil
}
