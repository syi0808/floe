package envfile

import (
	"os"
	"path/filepath"
	"testing"
)

func TestLoadUsesFileWithoutOverwritingProcessEnvironment(t *testing.T) {
	directory := t.TempDir()
	path := filepath.Join(directory, "oauth.env")
	if err := os.WriteFile(path, []byte("FLOE_ENVFILE_FROM_FILE='file value'\nFLOE_ENVFILE_OVERRIDE=file\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("FLOE_ENV_FILE", path)
	t.Setenv("FLOE_ENVFILE_OVERRIDE", "process")
	t.Cleanup(func() { _ = os.Unsetenv("FLOE_ENVFILE_FROM_FILE") })

	if err := Load(); err != nil {
		t.Fatal(err)
	}
	if value := os.Getenv("FLOE_ENVFILE_FROM_FILE"); value != "file value" {
		t.Fatalf("unexpected file value %q", value)
	}
	if value := os.Getenv("FLOE_ENVFILE_OVERRIDE"); value != "process" {
		t.Fatalf("process environment was overwritten with %q", value)
	}
}

func TestLoadIgnoresMissingDefaultFile(t *testing.T) {
	directory := t.TempDir()
	previous, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Chdir(directory); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chdir(previous) })
	t.Setenv("FLOE_ENV_FILE", "")

	if err := Load(); err != nil {
		t.Fatal(err)
	}
}

func TestParseRejectsInvalidName(t *testing.T) {
	if _, err := parse("NOT-A-NAME=value\n"); err == nil {
		t.Fatal("expected invalid name error")
	}
}
