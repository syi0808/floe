package console

import (
	"bytes"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"

	"floe/server/internal/credentials"
	"floe/server/internal/inference"
)

type Vault interface {
	Get(string) (string, error)
	Put(string, string) error
	Delete(string) error
}

type diskState struct {
	Targets     map[string]inference.Target        `json:"targets"`
	Routes      map[string]inference.Route         `json:"routes"`
	Providers   map[string]providerProfile         `json:"providers,omitempty"`
	Connections map[string]connectionRecord        `json:"connections,omitempty"`
	Clients     map[string]pairedClient            `json:"clients"`
	Cleanups    map[string]personCleanup           `json:"person_cleanups"`
	Attempts    map[string]connectionAttemptRecord `json:"connection_attempts"`
}

type connectionAttemptRecord struct {
	AttemptID       string         `json:"attempt_id"`
	PersonID        string         `json:"person_id"`
	DeviceID        string         `json:"device_id"`
	ConnectorID     string         `json:"connector_id"`
	ConnectionID    string         `json:"connection_id"`
	Credential      string         `json:"credential"`
	Scope           map[string]any `json:"scope"`
	CreatedAtUnixMs int64          `json:"created_at_unix_ms"`
	CleanupKind     string         `json:"cleanup_kind"`
	RuntimeComplete bool           `json:"runtime_complete"`
	VaultComplete   bool           `json:"vault_complete"`
}

type personCleanup struct {
	PersonID    string                  `json:"person_id"`
	Connections []connectionCleanupStep `json:"connections"`
}

type connectionCleanupStep struct {
	ConnectionID    string `json:"connection_id"`
	ConnectorID     string `json:"connector_id"`
	Credential      string `json:"credential,omitempty"`
	RuntimeComplete bool   `json:"runtime_complete"`
	VaultComplete   bool   `json:"vault_complete"`
}

type connectionRecord struct {
	ConnectionID string         `json:"connection_id"`
	Revision     uint64         `json:"revision"`
	ConnectorID  string         `json:"connector_id"`
	PersonID     string         `json:"person_id"`
	Device       *deviceBinding `json:"device_binding,omitempty"`
	Scope        map[string]any `json:"scope"`
	Credential   string         `json:"credential,omitempty"`
}

type deviceBinding struct {
	DeviceID string `json:"device_id"`
}

type pairedClient struct {
	TokenHash string `json:"token_hash"`
	PersonID  string `json:"person_id"`
	DeviceID  string `json:"device_id"`
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

func newConnectionID() (string, error) {
	bytes := make([]byte, 16)
	if _, err := rand.Read(bytes); err != nil {
		return "", err
	}
	bytes[6] = bytes[6]&0x0f | 0x40
	bytes[8] = bytes[8]&0x3f | 0x80
	hexadecimal := hex.EncodeToString(bytes)
	return hexadecimal[:8] + "-" + hexadecimal[8:12] + "-" + hexadecimal[12:16] + "-" + hexadecimal[16:20] + "-" + hexadecimal[20:], nil
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
	state := diskState{Targets: map[string]inference.Target{}, Routes: map[string]inference.Route{}, Providers: map[string]providerProfile{}, Connections: map[string]connectionRecord{}, Clients: map[string]pairedClient{}, Cleanups: map[string]personCleanup{}, Attempts: map[string]connectionAttemptRecord{}}
	if err := os.MkdirAll(directory, 0700); err != nil {
		return state, "", err
	}
	info, err := os.Lstat(directory)
	if err != nil || !info.IsDir() || info.Mode().Perm()&0077 != 0 {
		return state, "", errors.New("server data directory must be private (0700)")
	}
	data, err := os.ReadFile(filepath.Join(directory, "state.json"))
	if err == nil {
		var fields map[string]json.RawMessage
		if json.Unmarshal(data, &fields) != nil || fields["connection_attempts"] == nil {
			return state, "", errors.New("invalid server state")
		}
		decoder := json.NewDecoder(bytes.NewReader(data))
		decoder.DisallowUnknownFields()
		if decoder.Decode(&state) != nil || decoder.Decode(new(any)) != io.EOF || state.Targets == nil || state.Clients == nil || state.Cleanups == nil || state.Attempts == nil || len(state.Targets) > 32 || len(state.Routes) > 8 || len(state.Providers) > 3 || len(state.Clients) > 16 || len(state.Cleanups) > 16 || len(state.Attempts) > 64 {
			return state, "", errors.New("invalid server state")
		}
		if len(state.Cleanups) > 1 {
			return state, "", errors.New("invalid server state")
		}
		if state.Routes == nil {
			state.Routes = map[string]inference.Route{}
		}
		if state.Providers == nil {
			state.Providers = map[string]providerProfile{}
		}
		if state.Connections == nil {
			state.Connections = map[string]connectionRecord{}
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
		personID := ""
		for _, client := range state.Clients {
			if len(client.TokenHash) != sha256.Size*2 || !validPersonID(client.PersonID) || !validDeviceID(client.DeviceID) {
				return state, "", errors.New("invalid server state")
			}
			if personID != "" && personID != client.PersonID {
				return state, "", errors.New("invalid server state")
			}
			personID = client.PersonID
		}
		connectionIdentities := map[string]bool{}
		credentialIdentities := map[string]bool{}
		connectorIdentities := map[string]bool{}
		for key, cleanup := range state.Cleanups {
			if key != cleanup.PersonID || !validPersonID(cleanup.PersonID) || personID != "" && personID != cleanup.PersonID || len(cleanup.Connections) == 0 || len(cleanup.Connections) > len(clientConnectorDefinitions) {
				return state, "", errors.New("invalid server state")
			}
			personID = cleanup.PersonID
			for _, step := range cleanup.Connections {
				definition, exists := clientConnectorDefinitionFor(step.ConnectorID)
				ownerKey := cleanup.PersonID + "\x00" + step.ConnectorID
				if !exists || !validConnectionID(step.ConnectionID) || connectionIdentities[step.ConnectionID] || credentialIdentities[step.Credential] || connectorIdentities[ownerKey] || definition.AuthKind == "secret" && !step.RuntimeComplete || step.Credential == "" && !step.VaultComplete {
					return state, "", errors.New("invalid server state")
				}
				connectionIdentities[step.ConnectionID] = true
				if step.Credential != "" {
					credentialIdentities[step.Credential] = true
				}
				connectorIdentities[ownerKey] = true
				credentialNamespace := definition.CredentialName
				if credentialNamespace == "" {
					credentialNamespace = definition.OAuthCredential
				}
				expectedCredential, _ := credentials.ConnectionName(credentialNamespace, step.ConnectionID, cleanup.PersonID)
				if step.Credential != expectedCredential {
					return state, "", errors.New("invalid server state")
				}
			}
		}
		personOwners := make(map[string]bool, len(state.Clients))
		for _, client := range state.Clients {
			personOwners[client.PersonID] = true
		}
		for key, connection := range state.Connections {
			ownerKey := connection.PersonID + "\x00" + connection.ConnectorID
			if key != connection.ConnectionID || !validConnectionID(connection.ConnectionID) || connectionIdentities[connection.ConnectionID] || credentialIdentities[connection.Credential] || !connectorIDPattern.MatchString(connection.ConnectorID) || !validPersonID(connection.PersonID) || !personOwners[connection.PersonID] || connectorIdentities[ownerKey] || connection.Revision == 0 || connection.Device != nil && !validDeviceID(connection.Device.DeviceID) {
				return state, "", errors.New("invalid server state")
			}
			connectionIdentities[connection.ConnectionID] = true
			if connection.Credential != "" {
				credentialIdentities[connection.Credential] = true
			}
			connectorIdentities[ownerKey] = true
			if definition, exists := clientConnectorDefinitionFor(connection.ConnectorID); exists {
				if _, err := validatedConnectorScope(definition, connection.Scope); err != nil {
					return state, "", errors.New("invalid server state")
				}
				credentialNamespace := definition.CredentialName
				if credentialNamespace == "" {
					credentialNamespace = definition.OAuthCredential
				}
				expectedCredential := ""
				if credentialNamespace != "" {
					expectedCredential, _ = credentials.ConnectionName(credentialNamespace, connection.ConnectionID, connection.PersonID)
				}
				if connection.Credential != expectedCredential {
					return state, "", errors.New("invalid server state")
				}
			}
		}
		for key, attempt := range state.Attempts {
			definition, exists := clientConnectorDefinitionFor(attempt.ConnectorID)
			clientOwned := false
			for _, client := range state.Clients {
				clientOwned = clientOwned || client.PersonID == attempt.PersonID && client.DeviceID == attempt.DeviceID
			}
			credentialNamespace := ""
			if exists {
				credentialNamespace = definition.OAuthCredential
			}
			expectedCredential, _ := credentials.ConnectionName(credentialNamespace, attempt.ConnectionID, attempt.PersonID)
			ownerKey := attempt.PersonID + "\x00" + attempt.ConnectorID
			if key != attempt.AttemptID || len(attempt.AttemptID) < 32 || len(attempt.AttemptID) > 128 || !exists || definition.AuthKind != "oauth_pkce" || !clientOwned || !validConnectionID(attempt.ConnectionID) || connectionIdentities[attempt.ConnectionID] || credentialIdentities[attempt.Credential] || connectorIdentities[ownerKey] || attempt.Credential != expectedCredential || attempt.CleanupKind != "oauth_logout" || attempt.RuntimeComplete || attempt.VaultComplete || attempt.CreatedAtUnixMs <= 0 {
				return state, "", errors.New("invalid server state")
			}
			connectionIdentities[attempt.ConnectionID] = true
			credentialIdentities[attempt.Credential] = true
			connectorIdentities[ownerKey] = true
			if _, err := validatedConnectorScope(definition, attempt.Scope); err != nil {
				return state, "", errors.New("invalid server state")
			}
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
	copy := diskState{Targets: map[string]inference.Target{}, Routes: map[string]inference.Route{}, Providers: map[string]providerProfile{}, Connections: map[string]connectionRecord{}, Clients: map[string]pairedClient{}, Cleanups: map[string]personCleanup{}, Attempts: map[string]connectionAttemptRecord{}}
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
	for key, value := range state.Connections {
		if value.Device != nil {
			binding := *value.Device
			value.Device = &binding
		}
		value.Scope = cloneConnectorScope(value.Scope)
		copy.Connections[key] = value
	}
	for key, value := range state.Cleanups {
		value.Connections = append([]connectionCleanupStep(nil), value.Connections...)
		copy.Cleanups[key] = value
	}
	for key, value := range state.Attempts {
		value.Scope = cloneConnectorScope(value.Scope)
		copy.Attempts[key] = value
	}
	return copy
}
