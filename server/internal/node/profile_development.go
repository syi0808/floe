//go:build floe_dev

package node

import (
	"errors"
	"floe/server/internal/credentials"
	"floe/server/internal/storage"
	"os"
	"path/filepath"
	"syscall"
)

func DefaultDataDirectory(base string) string { return filepath.Join(base, "FloeServerDevelopment") }
func DefaultAddress() string                  { return "127.0.0.1:18431" }

const developmentMarker = "Floe isolated development storage v1\n"

const storageProfile = "development"

func profileStore(directory string, initialize bool) (credentials.Store, error) {
	invalid := errors.New("development storage profile unavailable; existing data was preserved")
	if !filepath.IsAbs(directory) || filepath.Clean(directory) != directory {
		return nil, invalid
	}
	// No adoption of a normal profile, including through FLOE_SERVER_DATA.
	if initialize {
		if err := os.Mkdir(directory, 0700); err != nil && !os.IsExist(err) {
			return nil, invalid
		}
	}
	info, err := os.Lstat(directory)
	if err != nil || !info.IsDir() || info.Mode().Perm()&0077 != 0 {
		return nil, invalid
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || int(stat.Uid) != os.Geteuid() {
		return nil, invalid
	}
	marker := filepath.Join(directory, "floe-development-profile")
	data, err := storage.ReadPrivate(marker, 64)
	if os.IsNotExist(err) {
		if !initialize {
			return nil, invalid
		}
		entries, e := os.ReadDir(directory)
		if e != nil || len(entries) != 0 {
			return nil, invalid
		}
		// Create-only marker: a concurrent initializer cannot replace it.
		f, e := os.OpenFile(marker, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0600)
		if e != nil {
			return nil, invalid
		}
		_, e = f.WriteString(developmentMarker)
		if e == nil {
			e = f.Sync()
		}
		closeErr := f.Close()
		if e != nil || closeErr != nil {
			return nil, invalid
		}
		parent, e := os.Open(directory)
		if e != nil {
			return nil, invalid
		}
		e = parent.Sync()
		closeErr = parent.Close()
		if e != nil || closeErr != nil {
			return nil, invalid
		}
	} else if err != nil || string(data) != developmentMarker {
		return nil, invalid
	}
	return credentials.NewDevelopmentFileStore(directory, initialize)
}

func freshProfile(directory string) error {
	entries, err := os.ReadDir(directory)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		switch entry.Name() {
		case "storage.lock", "floe-development-profile":
		case "development-credentials":
			path := filepath.Join(directory, entry.Name())
			if storage.PrivateDirectory(path) != nil {
				return errStorage
			}
			children, e := os.ReadDir(path)
			if e != nil || len(children) != 0 {
				return errStorage
			}
		default:
			return errStorage
		}
	}
	marker, err := storage.ReadPrivate(filepath.Join(directory, "floe-development-profile"), 64)
	if err != nil || string(marker) != developmentMarker {
		return errStorage
	}
	return nil
}
