package application

import (
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
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
	request := httptest.NewRequest(http.MethodGet, "http://127.0.0.1:8431/", nil)
	request.Host = "127.0.0.1:8431"
	response := httptest.NewRecorder()
	local.ServeHTTP(response, request)
	if status.Available || response.Code != http.StatusOK || !strings.Contains(response.Body.String(), "Floe") {
		t.Fatalf("required server or optional status = response=%d status=%+v", response.Code, status)
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

func TestNewLocalRejectsCorruptProducerIdentity(t *testing.T) {
	directory, runtime := localFixture(t)
	if _, err := NewLocal(LocalConfig{Directory: directory, Address: "127.0.0.1:8431", Vault: credentials.Keychain{}, Runtime: runtime}); err != nil {
		t.Fatal(err)
	}
	runtime.Close()
	if err := os.WriteFile(filepath.Join(directory, "producer-identity.json"), []byte(`{"schema_version":1}`), 0600); err != nil {
		t.Fatal(err)
	}
	reopenedRuntime := codexauth.New(credentials.Keychain{})
	defer reopenedRuntime.Close()
	if _, err := NewLocal(LocalConfig{Directory: directory, Address: "127.0.0.1:8431", Vault: credentials.Keychain{}, Runtime: reopenedRuntime}); !errors.Is(err, ErrRequiredSecurity) {
		t.Fatalf("corrupt producer identity error = %v", err)
	}
}

func TestNewLocalRejectsCorruptTrustData(t *testing.T) {
	directory, runtime := localFixture(t)
	if _, err := NewLocal(LocalConfig{Directory: directory, Address: "127.0.0.1:8431", Vault: credentials.Keychain{}, Runtime: runtime}); err != nil {
		t.Fatal(err)
	}
	runtime.Close()
	statePath := filepath.Join(directory, "state.json")
	data, err := os.ReadFile(statePath)
	if err != nil {
		t.Fatal(err)
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		t.Fatal(err)
	}
	fields["trusted_issuers"] = json.RawMessage("null")
	data, err = json.Marshal(fields)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(statePath, data, 0600); err != nil {
		t.Fatal(err)
	}
	reopenedRuntime := codexauth.New(credentials.Keychain{})
	defer reopenedRuntime.Close()
	if _, err := NewLocal(LocalConfig{Directory: directory, Address: "127.0.0.1:8431", Vault: credentials.Keychain{}, Runtime: reopenedRuntime}); !errors.Is(err, ErrRequiredSecurity) {
		t.Fatalf("corrupt trust error = %v", err)
	}
}

func localFixture(t *testing.T) (string, *codexauth.Runtime) {
	t.Helper()
	directory := t.TempDir()
	if err := os.Chmod(directory, 0700); err != nil {
		t.Fatal(err)
	}
	return directory, codexauth.New(credentials.Keychain{})
}
