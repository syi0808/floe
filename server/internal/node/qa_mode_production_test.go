//go:build !floe_dev

package node

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestNodeRejectsDevelopmentQAModeInProductionBuild(t *testing.T) {
	directory := filepath.Join(t.TempDir(), "must-not-be-created")
	node, err := New(Config{Directory: directory, Address: "127.0.0.1:18431", DevelopmentQANoAuth: true})
	if node != nil || err == nil || !strings.Contains(err.Error(), "floe_dev") {
		t.Fatalf("production QA mode returned node=%v, err=%v", node, err)
	}
	if _, err := os.Stat(directory); !os.IsNotExist(err) {
		t.Fatalf("production QA mode touched profile directory: %v", err)
	}
}
