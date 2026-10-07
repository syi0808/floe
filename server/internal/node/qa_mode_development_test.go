//go:build floe_dev

package node

import (
	"path/filepath"
	"testing"
)

func TestNodeAcceptsDevelopmentQAModeInDevelopmentBuild(t *testing.T) {
	server, err := New(Config{
		Directory:           filepath.Join(t.TempDir(), "synthetic-qa-profile"),
		Address:             "127.0.0.1:18431",
		DevelopmentQANoAuth: true,
	})
	if err != nil {
		t.Fatalf("development QA mode was rejected: %v", err)
	}
	server.Close()
}
