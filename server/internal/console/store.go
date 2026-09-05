package console

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"

	"floe/server/internal/inference"
)

type Vault interface {
	Get(string) (string, error)
	Put(string, string) error
	Delete(string) error
}

type diskState struct {
	Targets   map[string]inference.Target `json:"targets"`
	Routes    map[string]inference.Route  `json:"routes"`
	Providers map[string]providerProfile  `json:"providers,omitempty"`
	Clients   map[string]string           `json:"clients"`
}

type providerProfile struct {
	BaseURL   string                  `json:"base_url"`
	APIKeyEnv string                  `json:"api_key_env,omitempty"`
	Classes   map[string]classProfile `json:"classes"`
}

type classProfile struct {
	Model           string `json:"model"`
	ReasoningEffort string `json:"reasoning_effort,omitempty"`
}

func randomToken() string {
	return rand.Text() + rand.Text()
}

func digest(value string) string {
	hash := sha256.Sum256([]byte(value))
	return hex.EncodeToString(hash[:])
}

func writePrivate(path string, value []byte) error {
	file, err := os.CreateTemp(filepath.Dir(path), ".floe-*")
	if err != nil {
		return err
	}
	defer os.Remove(file.Name())
	if _, err = file.Write(value); err != nil {
		file.Close()
		return err
	}
	if err = file.Sync(); err != nil {
		file.Close()
		return err
	}
	if err = file.Close(); err != nil {
		return err
	}
	return os.Rename(file.Name(), path)
}

func readState(directory string) (diskState, string, error) {
	state := diskState{Targets: map[string]inference.Target{}, Routes: map[string]inference.Route{}, Providers: map[string]providerProfile{}, Clients: map[string]string{}}
	if err := os.MkdirAll(directory, 0700); err != nil {
		return state, "", err
	}
	info, err := os.Lstat(directory)
	if err != nil || !info.IsDir() || info.Mode().Perm()&0077 != 0 {
		return state, "", errors.New("server data directory must be private (0700)")
	}
	data, err := os.ReadFile(filepath.Join(directory, "state.json"))
	if err == nil {
		if json.Unmarshal(data, &state) != nil || state.Targets == nil || state.Clients == nil || len(state.Targets) > 32 || len(state.Routes) > 8 || len(state.Providers) > 3 || len(state.Clients) > 16 {
			return state, "", errors.New("invalid server state")
		}
		if state.Routes == nil {
			state.Routes = map[string]inference.Route{}
		}
		if state.Providers == nil {
			state.Providers = map[string]providerProfile{}
		}
		configuredTargets := len(state.Targets)
		for _, profile := range state.Providers {
			if profile.Classes == nil || len(profile.Classes) > 3 {
				return state, "", errors.New("invalid server state")
			}
			configuredTargets += len(profile.Classes)
		}
		if configuredTargets > 32 {
			return state, "", errors.New("invalid server state")
		}
	} else if !os.IsNotExist(err) {
		return state, "", err
	}
	path := filepath.Join(directory, "admin-token")
	secret, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		secret = []byte(randomToken() + randomToken())
		err = writePrivate(path, secret)
	}
	if err != nil || len(secret) < 32 {
		return state, "", errors.New("admin credential unavailable")
	}
	return state, string(secret), nil
}

func (console *Console) save(state diskState) error {
	data, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return err
	}
	return writePrivate(filepath.Join(console.directory, "state.json"), data)
}

func cloneState(state diskState) diskState {
	copy := diskState{Targets: map[string]inference.Target{}, Routes: map[string]inference.Route{}, Providers: map[string]providerProfile{}, Clients: map[string]string{}}
	for key, value := range state.Targets {
		copy.Targets[key] = value
	}
	for key, value := range state.Routes {
		copy.Routes[key] = value
	}
	for key, value := range state.Providers {
		classes := map[string]classProfile{}
		for class, configured := range value.Classes {
			classes[class] = configured
		}
		value.Classes = classes
		copy.Providers[key] = value
	}
	for key, value := range state.Clients {
		copy.Clients[key] = value
	}
	return copy
}
