package testsupport

import (
	"os"
	"testing"

	privatefiles "floe/server/internal/adapters/storage/privatefiles"
)

func TestPrivateDirectoryUsesExplicitOwnerOnlyMode(t *testing.T) {
	first, second := PrivateDirectory(t), PrivateDirectory(t)
	if first == second {
		t.Fatal("fixture directories must be independent")
	}
	for _, directory := range []string{first, second} {
		info, err := os.Lstat(directory)
		if err != nil {
			t.Fatal(err)
		}
		if !info.IsDir() || info.Mode().Perm() != 0700 {
			t.Fatalf("fixture mode = %v, want private directory", info.Mode())
		}
		if err := privatefiles.PrivateDirectory(directory); err != nil {
			t.Fatalf("production metadata check: %v", err)
		}
	}
}
