// Package testsupport provides synthetic fixture setup for server tests.
package testsupport

import (
	"os"
	"testing"
)

// PrivateDirectory creates a private fixture directory without relying on the
// process umask or testing.TempDir's child permissions. The outer directory is
// owned and cleaned up by the test; production metadata checks remain unchanged.
func PrivateDirectory(t testing.TB) string {
	t.Helper()
	directory, err := os.MkdirTemp(t.TempDir(), "private-")
	if err != nil {
		t.Fatal(err)
	}
	return directory
}
