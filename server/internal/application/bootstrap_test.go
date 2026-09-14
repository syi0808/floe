package application

import (
	"errors"
	"os"
	"testing"

	"floe/server/internal/codexauth"
	"floe/server/internal/credentials"
)

func TestConfigureOptionalFailureDoesNotBlockHealthyModule(t *testing.T) {
	failedCalled, healthyCalled := false, false
	failed := ConfigureOptional("broken.connector", func() error {
		failedCalled = true
		return errors.New("credential value must not be exposed")
	})
	healthy := ConfigureOptional("healthy.inference", func() error {
		healthyCalled = true
		return nil
	})
	if !failedCalled || failed.Available || failed.Diagnostic != "optional module unavailable" {
		t.Fatalf("failed module status = %+v", failed)
	}
	if healthyCalled == false || !healthy.Available {
		t.Fatalf("healthy module status = %+v", healthy)
	}
	if failed.Diagnostic == "credential value must not be exposed" {
		t.Fatal("optional diagnostic exposed initialization detail")
	}
}

func TestOptionalFailureLeavesRequiredLocalServerAvailable(t *testing.T) {
	runtime := codexauth.New(credentials.Keychain{})
	defer runtime.Close()
	directory := t.TempDir()
	if err := os.Chmod(directory, 0700); err != nil {
		t.Fatal(err)
	}
	local, err := NewLocal(LocalConfig{Directory: directory, Address: "127.0.0.1:8431", Vault: credentials.Keychain{}, Runtime: runtime})
	if err != nil {
		t.Fatalf("required local server failed: %v", err)
	}
	status := ConfigureOptional("broken.connector", func() error { return errors.New("optional init failed") })
	if status.Available || local.Management == nil {
		t.Fatalf("required server or optional status = server=%v status=%+v", local.Management != nil, status)
	}
}

func TestNewLocalRejectsRequiredSecurityConfiguration(t *testing.T) {
	runtime := codexauth.New(credentials.Keychain{})
	defer runtime.Close()
	_, err := NewLocal(LocalConfig{Directory: t.TempDir(), Address: "0.0.0.0:8431", Vault: credentials.Keychain{}, Runtime: runtime})
	if !errors.Is(err, ErrRequiredSecurity) {
		t.Fatalf("error = %v, want ErrRequiredSecurity", err)
	}
}
