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

func profileStore(directory string) (credentials.Store, error) {
	// A production binary never adopts development credentials or state.
	if _, err := os.Lstat(filepath.Join(directory, "floe-development-profile")); !os.IsNotExist(err) {
		return nil, errors.New("storage profile does not match this build")
	}
	return credentials.Keychain{}, nil
}
