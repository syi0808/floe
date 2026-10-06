//go:build !floe_dev

package node

import (
	"errors"
	"floe/server/internal/credentials"
	"os"
	"path/filepath"
)

func DefaultDataDirectory(base string) string { return filepath.Join(base, "FloeServer") }
func DefaultAddress() string                  { return "127.0.0.1:8431" }

const storageProfile = "production"

func profileStore(directory string, initialize bool) (credentials.Store, error) {
	// A production binary never adopts development credentials or state.
	for _, root := range []string{directory, filepath.Dir(directory)} {
		_, err := os.Lstat(filepath.Join(root, "floe-development-profile"))
		if err == nil {
			return nil, storageFailure("profile_invalid", operationProfileOpen, stageBuildProfileCheck, errStorage)
		}
		if !errors.Is(err, os.ErrNotExist) {
			return nil, profileStorageFailure(err, operationProfileOpen, stageBuildProfileCheck)
		}
	}
	return credentials.Keychain{}, nil
}

func freshProfile(directory string) error {
	entries, err := os.ReadDir(directory)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		if entry.Name() != "storage.lock" {
			return errStorage
		}
	}
	return nil
}
