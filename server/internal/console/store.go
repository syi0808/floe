package console

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"

	"floe/server/internal/inference"
)

type Vault interface {
	Get(string) (string, error)
	Put(string, string) error
	Delete(string) error
}

type diskState struct {
	Targets     map[string]inference.Target `json:"targets"`
	Routes      map[string]inference.Route  `json:"routes"`
	Providers   map[string]providerProfile  `json:"providers,omitempty"`
	Connectors  connectorConfigState        `json:"connectors,omitempty"`
	Connections map[string]connectionRecord `json:"connections,omitempty"`
	Clients     map[string]pairedClient     `json:"clients"`
}

type connectionRecord struct {
	ConnectionID string         `json:"connection_id"`
	ConnectorID  string         `json:"connector_id"`
	PersonID     string         `json:"person_id"`
	Device       *deviceBinding `json:"device_binding,omitempty"`
}

type deviceBinding struct {
	DeviceID string `json:"device_id"`
}

type pairedClient struct {
	TokenHash string `json:"token_hash"`
	PersonID  string `json:"person_id,omitempty"`
	DeviceID  string `json:"device_id,omitempty"`
	Legacy    bool   `json:"legacy_unscoped,omitempty"`
}

func validScopedCredential(namespace, value string) bool {
	if value == "" {
		return true
	}
	prefix := namespace + ":"
	if !strings.HasPrefix(value, prefix) || len(value) != len(prefix)+sha256.Size*2 {
		return false
	}
	_, err := hex.DecodeString(strings.TrimPrefix(value, prefix))
	return err == nil
}

func (client *pairedClient) UnmarshalJSON(data []byte) error {
	var legacy string
	if json.Unmarshal(data, &legacy) == nil {
		*client = pairedClient{TokenHash: legacy, Legacy: true}
		return nil
	}
	type wire pairedClient
	var value wire
	if err := json.Unmarshal(data, &value); err != nil {
		return err
	}
	*client = pairedClient(value)
	return nil
}

type connectorConfigState struct {
	GitHub            *githubConnectorConfig            `json:"github,omitempty"`
	Slack             *slackConnectorConfig             `json:"slack,omitempty"`
	GoogleDrive       *googleDriveConnectorConfig       `json:"google_drive,omitempty"`
	GoogleCalendar    *googleCalendarConnectorConfig    `json:"google_calendar,omitempty"`
	MicrosoftCalendar *microsoftCalendarConnectorConfig `json:"microsoft_calendar,omitempty"`
	MicrosoftTeams    *microsoftTeamsConnectorConfig    `json:"microsoft_teams,omitempty"`
	HomeAssistant     *homeAssistantConnectorConfig     `json:"home_assistant,omitempty"`
}

type githubConnectorConfig struct {
	Owner      string `json:"owner"`
	Repository string `json:"repository"`
	Credential string `json:"credential,omitempty"`
}

type slackConnectorConfig struct {
	Channel    string `json:"channel"`
	Thread     string `json:"thread,omitempty"`
	Credential string `json:"credential,omitempty"`
}

type googleDriveConnectorConfig struct {
	FolderID string `json:"folder_id"`
}

type googleCalendarConnectorConfig struct {
	CalendarID string `json:"calendar_id"`
}

type microsoftCalendarConnectorConfig struct {
	CalendarID string `json:"calendar_id"`
}

type microsoftTeamsConnectorConfig struct {
	TeamID    string `json:"team_id"`
	ChannelID string `json:"channel_id"`
}

type homeAssistantConnectorConfig struct {
	BaseURL    string   `json:"base_url"`
	Entities   []string `json:"entities"`
	Credential string   `json:"credential,omitempty"`
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
	state := diskState{Targets: map[string]inference.Target{}, Routes: map[string]inference.Route{}, Providers: map[string]providerProfile{}, Connections: map[string]connectionRecord{}, Clients: map[string]pairedClient{}}
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
		for _, client := range state.Clients {
			if len(client.TokenHash) != sha256.Size*2 || (!client.Legacy && (!validPersonID(client.PersonID) || !validDeviceID(client.DeviceID))) {
				return state, "", errors.New("invalid server state")
			}
		}
		for key, connection := range state.Connections {
			if key != connection.ConnectionID || !connectionIDPattern.MatchString(connection.ConnectionID) || !connectionIDPattern.MatchString(connection.ConnectorID) || !validPersonID(connection.PersonID) || connection.Device != nil && !validDeviceID(connection.Device.DeviceID) {
				return state, "", errors.New("invalid server state")
			}
		}
		if state.Connectors.GitHub != nil && !validScopedCredential(githubTokenKey, state.Connectors.GitHub.Credential) ||
			state.Connectors.Slack != nil && !validScopedCredential(slackTokenKey, state.Connectors.Slack.Credential) ||
			state.Connectors.HomeAssistant != nil && !validScopedCredential(homeTokenKey, state.Connectors.HomeAssistant.Credential) {
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
	copy := diskState{Targets: map[string]inference.Target{}, Routes: map[string]inference.Route{}, Providers: map[string]providerProfile{}, Connections: map[string]connectionRecord{}, Clients: map[string]pairedClient{}, Connectors: state.Connectors}
	if state.Connectors.GitHub != nil {
		configured := *state.Connectors.GitHub
		copy.Connectors.GitHub = &configured
	}
	if state.Connectors.HomeAssistant != nil {
		configured := *state.Connectors.HomeAssistant
		configured.Entities = append([]string(nil), configured.Entities...)
		copy.Connectors.HomeAssistant = &configured
	}
	if state.Connectors.Slack != nil {
		configured := *state.Connectors.Slack
		copy.Connectors.Slack = &configured
	}
	if state.Connectors.GoogleDrive != nil {
		configured := *state.Connectors.GoogleDrive
		copy.Connectors.GoogleDrive = &configured
	}
	if state.Connectors.GoogleCalendar != nil {
		configured := *state.Connectors.GoogleCalendar
		copy.Connectors.GoogleCalendar = &configured
	}
	if state.Connectors.MicrosoftCalendar != nil {
		configured := *state.Connectors.MicrosoftCalendar
		copy.Connectors.MicrosoftCalendar = &configured
	}
	if state.Connectors.MicrosoftTeams != nil {
		configured := *state.Connectors.MicrosoftTeams
		copy.Connectors.MicrosoftTeams = &configured
	}
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
		copy.Connections[key] = value
	}
	return copy
}
