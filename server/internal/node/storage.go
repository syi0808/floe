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
const rootSealContent = "Floe encrypted server root v1\n"

type StorageFailure struct{ code string }

func (e *StorageFailure) Error() string {
	return "server encrypted profile unavailable (" + e.code + "); existing data and keys were preserved"
}
func (e *StorageFailure) Code() string { return e.code }
func storageFailure(code string) error { return &StorageFailure{code: code} }

var errStorage = storageFailure("profile_invalid")

func custodyFailure(err error) error {
	switch {
	case errors.Is(err, credentials.ErrLocked):
		return storageFailure("key_locked_or_denied")
	case errors.Is(err, credentials.ErrBusy):
		return storageFailure("key_busy")
	case errors.Is(err, context.DeadlineExceeded):
		return storageFailure("key_timeout")
	case errors.Is(err, context.Canceled):
		return storageFailure("key_cancelled")
	default:
		return storageFailure("key_unavailable")
	}
}

type rootIdentity struct {
	Format  int    `json:"format"`
	ID      string `json:"id"`
	Profile string `json:"profile"`
	State   string `json:"state"`
}
type admittedStorage struct {
	fresh bool
	files *storage.Files
	lock  *os.File
	once  sync.Once
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
	if d.Decode(&id) != nil || d.Decode(new(any)) != io.EOF || id.Format != 1 || id.Profile != storageProfile {
		return id, errStorage
	}
	raw, err := hex.DecodeString(id.ID)
	if err != nil || len(raw) != 32 || hex.EncodeToString(raw) != id.ID {
		return id, errStorage
	}
	if id.State == "initializing" {
		return id, storageFailure("creation_incomplete")
	}
	if id.State != "ready" {
		return id, errStorage
	}
	return id, nil
}
func syncDirectory(path string) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return f.Sync()
}
func createRootIdentity(path string, data []byte) error {
	f, err := os.OpenFile(path, os.O_WRONLY|os.O_CREATE|os.O_EXCL|syscall.O_NOFOLLOW, 0600)
	if err != nil {
		return err
	}
	_, err = f.Write(data)
	if err == nil {
		err = f.Sync()
	}
	closeErr := f.Close()
	if err != nil {
		return err
	}
	if closeErr != nil {
		return closeErr
	}
	return syncDirectory(filepath.Dir(path))
}
func openStorage(ctx context.Context, directory string, vault credentials.Store, writable bool) (out *admittedStorage, err error) {
	if ctx == nil || vault == nil || !filepath.IsAbs(directory) || filepath.Clean(directory) != directory {
		return nil, errStorage
	}
	if writable {
		if err := os.Mkdir(directory, 0700); err != nil && !os.IsExist(err) {
			return nil, errStorage
		}
	}
	if storage.PrivateDirectory(directory) != nil {
		return nil, errStorage
	}
	if writable && syncDirectory(filepath.Dir(directory)) != nil {
		return nil, errStorage
	}
	var lock *os.File
	if writable {
		lock, err = os.OpenFile(filepath.Join(directory, "storage.lock"), os.O_RDWR|os.O_CREATE|syscall.O_NOFOLLOW|syscall.O_NONBLOCK, 0600)
		if err != nil {
			return nil, errStorage
		}
		info, e := lock.Stat()
		st, ok := infoSys(info)
		if e != nil || !ok || !info.Mode().IsRegular() || info.Mode().Perm()&0077 != 0 || int(st.Uid) != os.Geteuid() || st.Nlink != 1 {
			lock.Close()
			return nil, errStorage
		}
		if syscall.Flock(int(lock.Fd()), syscall.LOCK_EX|syscall.LOCK_NB) != nil {
			lock.Close()
			return nil, errStorage
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
	fresh := os.IsNotExist(readErr)
	var id rootIdentity
	if fresh {
		if !writable || freshProfile(directory) != nil {
			return nil, errStorage
		}
		random := make([]byte, 32)
		if _, e := rand.Read(random); e != nil {
			return nil, errStorage
		}
		id = rootIdentity{Format: 1, ID: hex.EncodeToString(random), Profile: storageProfile, State: "initializing"}
	} else {
		if readErr != nil {
			return nil, errStorage
		}
		id, err = decodeRoot(data)
		if err != nil {
			return nil, err
		}
	}
	value, err := vault.Get(ctx, rootSlot(id))
	if err != nil {
		return nil, custodyFailure(err)
	}
	if fresh {
		if value != "" {
			return nil, storageFailure("key_slot_occupied")
		}
		// Get was read-only. Reserve the durable attempt immediately before Put.
		// A locked/busy preflight read cannot strand an untouched profile.
		encoded, _ := json.Marshal(id)
		if createRootIdentity(path, encoded) != nil {
			return nil, storageFailure("creation_incomplete")
		}
		key := make([]byte, 32)
		if _, e := rand.Read(key); e != nil {
			return nil, errStorage
		}
		value = hex.EncodeToString(key)
		clear(key)
		if e := vault.Put(ctx, rootSlot(id), value); e != nil {
			return nil, custodyFailure(e)
		}
		observed, e := vault.Get(ctx, rootSlot(id))
		if e != nil {
			return nil, custodyFailure(e)
		}
		if subtle.ConstantTimeCompare([]byte(observed), []byte(value)) != 1 {
			return nil, storageFailure("key_readback_mismatch")
		}
	}
	if value == "" {
		return nil, storageFailure("key_missing")
	}
	key, err := hex.DecodeString(value)
	if err != nil || len(key) != 32 || hex.EncodeToString(key) != value {
		return nil, storageFailure("key_malformed")
	}
	defer clear(key)
	files, err := storage.NewFiles(directory, id.ID, key, writable)
	if err != nil {
		return nil, errStorage
	}
	defer func() {
		if err != nil {
			files.Close()
		}
	}()
	if fresh {
		if files.Write(rootSealName, []byte(rootSealContent)) != nil {
			return nil, errStorage
		}
		id.State = "ready"
		encoded, _ := json.Marshal(id)
		if storage.WritePrivate(path, encoded) != nil {
			return nil, errStorage
		}
	}
	seal, err := files.Read(rootSealName, 128)
	if err != nil || string(seal) != rootSealContent {
		return nil, storageFailure("root_authentication_failed")
	}
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	return &admittedStorage{files: files, lock: lock, fresh: fresh}, nil
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
		return "", errStorage
	}
	token, err := trustFiles.Read("admin-token", 1024)
	if err != nil || len(token) < 32 {
		return "", errStorage
	}
	return string(token), nil
}
