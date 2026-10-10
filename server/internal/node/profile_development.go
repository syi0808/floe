//go:build floe_dev

package node

import (
	"errors"
	"floe/server/internal/adapters/credentials"
	"floe/server/internal/adapters/storage/privatefiles"
	"os"
	"path/filepath"
	"syscall"
)

func DefaultDataDirectory(base string) string { return filepath.Join(base, "FloeServerDevelopment") }
func DefaultAddress() string                  { return "127.0.0.1:18431" }

const developmentMarker = "Floe isolated development storage v1\n"

const storageProfile = "development"

func profileStore(directory string, initialize bool) (credentials.Store, error) {
	if !filepath.IsAbs(directory) || filepath.Clean(directory) != directory {
		return nil, storageFailure("profile_invalid", operationProfileOpen, stageInput, errStorage)
	}
	// No adoption of a normal profile, including through FLOE_SERVER_DATA.
	if initialize {
		if err := os.Mkdir(directory, 0700); err != nil && !os.IsExist(err) {
			return nil, profileStorageFailure(err, operationProfileOpen, stageProfileDirectory)
		}
	}
	info, err := os.Lstat(directory)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil, storageFailure("profile_invalid", operationProfileOpen, stageProfileDirectory, err)
		}
		return nil, profileStorageFailure(err, operationProfileOpen, stageProfileDirectory)
	}
	if !info.IsDir() || info.Mode().Perm()&0077 != 0 {
		return nil, storageFailure("profile_invalid", operationProfileOpen, stageProfileDirectory, errStorage)
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || int(stat.Uid) != os.Geteuid() {
		return nil, storageFailure("profile_invalid", operationProfileOpen, stageProfileDirectory, errStorage)
	}
	marker := filepath.Join(directory, "floe-development-profile")
	data, err := storage.ReadPrivate(marker, 64)
	if errors.Is(err, os.ErrNotExist) {
		if !initialize {
			return nil, storageFailure("profile_invalid", operationProfileOpen, stageBuildProfileCheck, err)
		}
		entries, e := os.ReadDir(directory)
		if e != nil {
			return nil, profileStorageFailure(e, operationProfileOpen, stageFreshProfile)
		}
		if len(entries) != 0 {
			return nil, storageFailure("profile_invalid", operationProfileOpen, stageFreshProfile, errStorage)
		}
		// Create-only marker: a concurrent initializer cannot replace it.
		f, e := os.OpenFile(marker, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0600)
		if e != nil {
			return nil, profileStorageFailure(e, operationProfileOpen, stageBuildProfileCheck)
		}
		_, e = f.WriteString(developmentMarker)
		if e == nil {
			e = f.Sync()
		}
		closeErr := f.Close()
		if e != nil {
			return nil, profileStorageFailure(e, operationProfileOpen, stageBuildProfileCheck)
		}
		if closeErr != nil {
			return nil, profileStorageFailure(closeErr, operationProfileOpen, stageBuildProfileCheck)
		}
		parent, e := os.Open(directory)
		if e != nil {
			return nil, profileStorageFailure(e, operationProfileOpen, stageParentSync)
		}
		e = parent.Sync()
		closeErr = parent.Close()
		if e != nil {
			return nil, profileStorageFailure(e, operationProfileOpen, stageParentSync)
		}
		if closeErr != nil {
			return nil, profileStorageFailure(closeErr, operationProfileOpen, stageParentSync)
		}
	} else if err != nil {
		return nil, profileStorageFailure(err, operationProfileOpen, stageBuildProfileCheck)
	} else if string(data) != developmentMarker {
		return nil, storageFailure("profile_invalid", operationProfileOpen, stageBuildProfileCheck, errStorage)
	}
	store, err := credentials.NewDevelopmentFileStore(directory, initialize)
	if err != nil {
		return nil, custodyFailure(err, stageRootKey)
	}
	return store, nil
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
			if err := storage.PrivateDirectory(path); err != nil {
				return err
			}
			children, e := os.ReadDir(path)
			if e != nil {
				return e
			}
			if len(children) != 0 {
				return errStorage
			}
		default:
			return errStorage
		}
	}
	marker, err := storage.ReadPrivate(filepath.Join(directory, "floe-development-profile"), 64)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return errStorage
		}
		return err
	}
	if string(marker) != developmentMarker {
		return errStorage
	}
	return nil
}
