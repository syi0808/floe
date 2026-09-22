package application

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

	"floe/server/internal/authorization"
	"floe/server/internal/connections"
	"floe/server/internal/credentials"
	"floe/server/internal/inference"
)

type Vault interface {
	Get(string) (string, error)
	Put(string, string) error
	Delete(string) error
}

const maxRetainedIssuerIdentities = 128
const trustSchemaVersion = 1

type indeterminatePrivateWriteError struct{ err error }

func (writeError indeterminatePrivateWriteError) Error() string { return writeError.err.Error() }
func (writeError indeterminatePrivateWriteError) Unwrap() error { return writeError.err }

func isIndeterminatePrivateWrite(err error) bool {
	var writeError indeterminatePrivateWriteError
	return errors.As(err, &writeError)
}

var syncPrivateDirectory = func(directory string) error {
	directoryFile, err := os.Open(directory)
	if err != nil {
		return err
	}
	if err := directoryFile.Sync(); err != nil {
		_ = directoryFile.Close()
		return err
	}
	return directoryFile.Close()
}

type diskState struct {
	Targets            map[string]inference.Target        `json:"targets"`
	Routes             map[string]inference.Route         `json:"routes"`
	Providers          map[string]providerProfile         `json:"providers,omitempty"`
	Connections        map[string]connections.Record      `json:"connections,omitempty"`
	Clients            map[string]pairedClient            `json:"clients"`
	Cleanups           map[string]personCleanup           `json:"person_cleanups"`
	Attempts           map[string]connectionAttemptRecord `json:"connection_attempts"`
	InstanceID         string                             `json:"instance_id"`
	ExecutionOwnerID   string                             `json:"execution_owner_id"`
	TrustSchemaVersion int                                `json:"trust_schema_version"`
	TrustedIssuers     map[string]trustedIssuerRecord     `json:"trusted_issuers"`
	RevokedIssuerKeys  map[string]bool                    `json:"revoked_issuer_keys"`
	TrustCorrupt       bool                               `json:"trust_corrupt,omitempty"`
	TrustQuarantine    map[string]json.RawMessage         `json:"trust_quarantine,omitempty"`
}

type connectionAttemptRecord struct {
	AttemptID       string         `json:"attempt_id"`
	ClientID        string         `json:"client_id"`
	PersonID        string         `json:"person_id"`
	DeviceID        string         `json:"device_id"`
	ConnectorID     string         `json:"connector_id"`
	ConnectionID    string         `json:"connection_id"`
	Incarnation     string         `json:"incarnation"`
	Epoch           uint64         `json:"epoch"`
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

type pairedClient struct {
	ClientID            string `json:"client_id,omitempty"`
	TokenHash           string `json:"token_hash"`
	PersonID            string `json:"person_id"`
	DeviceID            string `json:"device_id"`
	ProducerInstanceID  string `json:"producer_instance_id,omitempty"`
	ProducerFingerprint string `json:"producer_fingerprint,omitempty"`
	ProducerAudience    string `json:"producer_audience,omitempty"`
}

type trustedIssuerRecord struct {
	KeyID        string `json:"key_id"`
	EnrollmentID string `json:"enrollment_id,omitempty"`
	ClientID     string `json:"client_id"`
	PersonID     string `json:"person_id"`
	DeviceID     string `json:"device_id"`
	PublicKey    []byte `json:"public_key"`
}

type providerProfile struct {
	BaseURL   string                            `json:"base_url"`
	APIKeyEnv string                            `json:"api_key_env,omitempty"`
	Classes   map[string]inference.ProfileClass `json:"classes"`
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
	if err = os.Rename(file.Name(), path); err != nil {
		return err
	}
	if err = syncPrivateDirectory(filepath.Dir(path)); err != nil {
		return indeterminatePrivateWriteError{err: err}
	}
	return nil
}

func readState(directory string) (diskState, string, error) {
	state := diskState{Targets: map[string]inference.Target{}, Routes: map[string]inference.Route{}, Providers: map[string]providerProfile{}, Connections: map[string]connections.Record{}, Clients: map[string]pairedClient{}, Cleanups: map[string]personCleanup{}, Attempts: map[string]connectionAttemptRecord{}, TrustSchemaVersion: trustSchemaVersion, TrustedIssuers: map[string]trustedIssuerRecord{}, RevokedIssuerKeys: map[string]bool{}, TrustQuarantine: map[string]json.RawMessage{}}
	var err error
	if state.InstanceID, err = newConnectionID(); err != nil {
		return state, "", err
	}
	if state.ExecutionOwnerID, err = newConnectionID(); err != nil {
		return state, "", err
	}
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
		if json.Unmarshal(data, &fields) != nil || fields["connection_attempts"] == nil || fields["instance_id"] == nil || fields["execution_owner_id"] == nil {
			return state, "", errors.New("invalid server state")
		}
		decodeData := data
		trustCorrupt := state.TrustCorrupt
		var marker int
		if raw, exists := fields["trust_schema_version"]; !exists || json.Unmarshal(raw, &marker) != nil || marker != trustSchemaVersion {
			trustCorrupt = true
		}
		decodeTrust := func(raw json.RawMessage, target any) error {
			decoder := json.NewDecoder(bytes.NewReader(raw))
			decoder.DisallowUnknownFields()
			if err := decoder.Decode(target); err != nil {
				return err
			}
			if err := decoder.Decode(new(any)); err != io.EOF {
				if err == nil {
					return errors.New("trailing trust data")
				}
				return err
			}
			return nil
		}
		validateTrustJSON := func(raw json.RawMessage) bool {
			return len(raw) <= 1<<20 && authorization.StrictJSON(raw)
		}
		issuerCount := 0
		revokedCount := 0
		if raw, exists := fields["trusted_issuers"]; !exists || bytes.Equal(bytes.TrimSpace(raw), []byte("null")) {
			trustCorrupt = true
		} else {
			var issuers map[string]trustedIssuerRecord
			if !validateTrustJSON(raw) || decodeTrust(raw, &issuers) != nil || len(issuers) > maxRetainedIssuerIdentities {
				trustCorrupt = true
			}
			issuerCount = len(issuers)
		}
		if raw, exists := fields["revoked_issuer_keys"]; !exists || bytes.Equal(bytes.TrimSpace(raw), []byte("null")) {
			trustCorrupt = true
		} else {
			var revoked map[string]bool
			if !validateTrustJSON(raw) || decodeTrust(raw, &revoked) != nil || len(revoked) > maxRetainedIssuerIdentities {
				trustCorrupt = true
			} else {
				for keyID, tombstone := range revoked {
					if keyID == "" || !tombstone {
						trustCorrupt = true
						break
					}
				}
			}
			revokedCount = len(revoked)
		}
		if issuerCount+revokedCount > maxRetainedIssuerIdentities {
			trustCorrupt = true
		}
		if trustCorrupt {
			var raw map[string]json.RawMessage
			if json.Unmarshal(data, &raw) == nil {
				quarantine := map[string]json.RawMessage{}
				for _, field := range []string{"trusted_issuers", "revoked_issuer_keys"} {
					if value, exists := raw[field]; exists {
						quarantine[field] = append(json.RawMessage(nil), value...)
					}
				}
				delete(raw, "trusted_issuers")
				delete(raw, "revoked_issuer_keys")
				raw["trust_schema_version"] = json.RawMessage("1")
				raw["trust_corrupt"] = json.RawMessage("true")
				if encoded, marshalErr := json.Marshal(quarantine); marshalErr == nil {
					raw["trust_quarantine"] = encoded
				}
				if sanitized, marshalErr := json.Marshal(raw); marshalErr == nil {
					decodeData = sanitized
				}
			}
		}
		decoder := json.NewDecoder(bytes.NewReader(decodeData))
		decoder.DisallowUnknownFields()
		if decoder.Decode(&state) != nil || decoder.Decode(new(any)) != io.EOF || state.TrustSchemaVersion != trustSchemaVersion || !validConnectionID(state.InstanceID) || !validConnectionID(state.ExecutionOwnerID) || state.Targets == nil || state.Clients == nil || state.Cleanups == nil || state.Attempts == nil || len(state.Targets) > 32 || len(state.Routes) > 8 || len(state.Providers) > 3 || len(state.Clients) > 16 || len(state.Cleanups) > 16 || len(state.Attempts) > 64 || len(state.TrustQuarantine) > 2 {
			return state, "", errors.New("invalid server state")
		}
		for field, value := range state.TrustQuarantine {
			if field != "trusted_issuers" && field != "revoked_issuer_keys" || len(value) == 0 {
				return state, "", errors.New("invalid trust quarantine")
			}
		}
		state.TrustCorrupt = state.TrustCorrupt || trustCorrupt || len(state.TrustQuarantine) > 0
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
			state.Connections = map[string]connections.Record{}
		}
		if state.TrustedIssuers == nil {
			state.TrustedIssuers = map[string]trustedIssuerRecord{}
		}
		if state.RevokedIssuerKeys == nil {
			state.RevokedIssuerKeys = map[string]bool{}
		}
		if state.TrustQuarantine == nil {
			state.TrustQuarantine = map[string]json.RawMessage{}
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
			if key != cleanup.PersonID || !validPersonID(cleanup.PersonID) || personID != "" && personID != cleanup.PersonID || len(cleanup.Connections) == 0 || len(cleanup.Connections) > len(connections.Definitions) {
				return state, "", errors.New("invalid server state")
			}
			personID = cleanup.PersonID
			for _, step := range cleanup.Connections {
				definition, exists := connections.DefinitionFor(step.ConnectorID)
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
			if definition, exists := connections.DefinitionFor(connection.ConnectorID); exists {
				if _, err := connections.ValidatedConnectorScope(definition, connection.Scope); err != nil {
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
			definition, exists := connections.DefinitionFor(attempt.ConnectorID)
			client, clientOwned := state.Clients[attempt.ClientID]
			clientOwned = clientOwned && client.PersonID == attempt.PersonID && client.DeviceID == attempt.DeviceID
			credentialNamespace := ""
			if exists {
				credentialNamespace = definition.OAuthCredential
			}
			expectedCredential, _ := credentials.ConnectionName(credentialNamespace, attempt.ConnectionID, attempt.PersonID)
			ownerKey := attempt.PersonID + "\x00" + attempt.ConnectorID
			if key != attempt.AttemptID || len(attempt.AttemptID) < 32 || len(attempt.AttemptID) > 128 || !exists || !connections.IsOAuthAuthKind(definition.AuthKind) || !clientOwned || !validConnectionID(attempt.ConnectionID) || connectionIdentities[attempt.ConnectionID] || credentialIdentities[attempt.Credential] || connectorIdentities[ownerKey] || attempt.Credential != expectedCredential || attempt.CleanupKind != "oauth_logout" || attempt.RuntimeComplete || attempt.VaultComplete || attempt.CreatedAtUnixMs <= 0 {
				return state, "", errors.New("invalid server state")
			}
			connectionIdentities[attempt.ConnectionID] = true
			credentialIdentities[attempt.Credential] = true
			connectorIdentities[ownerKey] = true
			if _, err := connections.ValidatedConnectorScope(definition, attempt.Scope); err != nil {
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
	err = writePrivate(filepath.Join(console.directory, "state.json"), data)
	if isIndeterminatePrivateWrite(err) {
		console.latchTrustUnavailable()
	}
	return err
}

func cloneState(state diskState) diskState {
	copy := diskState{Targets: map[string]inference.Target{}, Routes: map[string]inference.Route{}, Providers: map[string]providerProfile{}, Connections: map[string]connections.Record{}, Clients: map[string]pairedClient{}, Cleanups: map[string]personCleanup{}, Attempts: map[string]connectionAttemptRecord{}, TrustSchemaVersion: state.TrustSchemaVersion, TrustedIssuers: map[string]trustedIssuerRecord{}, RevokedIssuerKeys: map[string]bool{}, TrustQuarantine: map[string]json.RawMessage{}, InstanceID: state.InstanceID, ExecutionOwnerID: state.ExecutionOwnerID, TrustCorrupt: state.TrustCorrupt}
	for key, value := range state.Targets {
		copy.Targets[key] = value
	}
	for key, value := range state.Routes {
		copy.Routes[key] = value
	}
	for key, value := range state.Providers {
		classes := map[string]inference.ProfileClass{}
		for class, configured := range value.Classes {
			classes[class] = configured
		}
		value.Classes = classes
		copy.Providers[key] = value
	}
	for key, value := range state.Clients {
		copy.Clients[key] = value
	}
	for key, value := range state.TrustedIssuers {
		value.PublicKey = append([]byte(nil), value.PublicKey...)
		copy.TrustedIssuers[key] = value
	}
	for key, value := range state.RevokedIssuerKeys {
		copy.RevokedIssuerKeys[key] = value
	}
	for key, value := range state.TrustQuarantine {
		copy.TrustQuarantine[key] = append(json.RawMessage(nil), value...)
	}
	for key, value := range state.Connections {
		if value.Device != nil {
			binding := *value.Device
			value.Device = &binding
		}
		value.Scope = connections.CloneConnectorScope(value.Scope)
		copy.Connections[key] = value
	}
	for key, value := range state.Cleanups {
		value.Connections = append([]connectionCleanupStep(nil), value.Connections...)
		copy.Cleanups[key] = value
	}
	for key, value := range state.Attempts {
		value.Scope = connections.CloneConnectorScope(value.Scope)
		copy.Attempts[key] = value
	}
	return copy
}
