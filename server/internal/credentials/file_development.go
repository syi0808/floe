//go:build floe_dev && (darwin || linux)

package credentials

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"floe/server/internal/storage"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
)

// DevelopmentFileStore is never compiled into a normal production binary.
// Private files remove OS prompts, not the need to protect real credentials.
// It is not an automatic fallback for Keychain failure.
type DevelopmentFileStore struct {
	root string
	mu   sync.Mutex
}

func NewDevelopmentFileStore(profile string, initialize bool) (*DevelopmentFileStore, error) {
	if err := privateDirectory(profile); err != nil {
		return nil, err
	}
	marker, err := storage.ReadPrivate(filepath.Join(profile, "floe-development-profile"), 64)
	if err != nil || string(marker) != "Floe isolated development storage v1\n" {
		return nil, ErrUnavailable
	}
	root := filepath.Join(profile, "development-credentials")
	if initialize {
		if err := os.Mkdir(root, 0700); err != nil && !os.IsExist(err) {
			return nil, ErrUnavailable
		}
	}
	if err := privateDirectory(root); err != nil {
		return nil, err
	}
	return &DevelopmentFileStore{root: root}, nil
}

func privateDirectory(path string) error {
	info, err := os.Lstat(path)
	if err != nil || !info.IsDir() || info.Mode().Perm()&0077 != 0 {
		return ErrUnavailable
	}
	st, ok := info.Sys().(*syscall.Stat_t)
	if !ok || int(st.Uid) != os.Geteuid() {
		return ErrUnavailable
	}
	return nil
}

func (s *DevelopmentFileStore) path(ctx context.Context, name string) (string, error) {
	if ctx == nil || name == "" || len(name) > 256 || strings.ContainsRune(name, 0) {
		return "", ErrUnavailable
	}
	if err := ctx.Err(); err != nil {
		return "", err
	}
	if err := privateDirectory(s.root); err != nil {
		return "", err
	}
	digest := sha256.Sum256([]byte(name))
	return filepath.Join(s.root, hex.EncodeToString(digest[:])), nil
}

func (s *DevelopmentFileStore) Get(ctx context.Context, name string) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	path, err := s.path(ctx, name)
	if err != nil {
		return "", err
	}
	value, err := readDevelopmentSecret(path)
	if err != nil {
		return "", err
	}
	if err := ctx.Err(); err != nil {
		return "", err
	}
	return value, nil
}

func readDevelopmentSecret(path string) (string, error) {
	f, err := os.OpenFile(path, os.O_RDONLY|syscall.O_NOFOLLOW|syscall.O_NONBLOCK, 0)
	if os.IsNotExist(err) {
		return "", nil
	}
	if err != nil {
		return "", ErrUnavailable
	}
	defer f.Close()
	info, err := f.Stat()
	if err != nil || !info.Mode().IsRegular() || info.Mode().Perm()&0077 != 0 || info.Size() > 131072 {
		return "", ErrUnavailable
	}
	st, ok := info.Sys().(*syscall.Stat_t)
	if !ok || int(st.Uid) != os.Geteuid() || st.Nlink != 1 {
		return "", ErrUnavailable
	}
	data, err := io.ReadAll(io.LimitReader(f, 131073))
	if err != nil || len(data) == 0 || len(data) > 131072 || strings.ContainsRune(string(data), 0) {
		return "", ErrUnavailable
	}
	return string(data), nil
}

func (s *DevelopmentFileStore) Put(ctx context.Context, name, value string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	path, err := s.path(ctx, name)
	if err != nil {
		return err
	}
	if value == "" || len(value) > 131072 || strings.ContainsRune(value, 0) {
		return ErrUnavailable
	}
	// Validate an existing slot before replacing it. Never follow a symlink.
	if _, err := readDevelopmentSecret(path); err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := storage.WritePrivate(path, []byte(value)); err != nil {
		return ErrUnavailable
	}
	observed, err := readDevelopmentSecret(path)
	if err != nil || observed != value {
		return ErrUnavailable
	}
	return ctx.Err()
}

func (s *DevelopmentFileStore) Delete(ctx context.Context, name string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	path, err := s.path(ctx, name)
	if err != nil {
		return err
	}
	if _, err := readDevelopmentSecret(path); err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
		return ErrUnavailable
	}
	directory, err := os.Open(s.root)
	if err != nil {
		return ErrUnavailable
	}
	defer directory.Close()
	if err := directory.Sync(); err != nil {
		return ErrUnavailable
	}
	return ctx.Err()
}
