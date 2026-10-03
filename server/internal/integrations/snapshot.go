package integrations

import "floe/server/internal/views"

type Capability struct {
	SchemaVersion  int      `json:"schema_version"`
	ID             string   `json:"id"`
	Version        string   `json:"version"`
	Authority      string   `json:"authority"`
	RequiredScopes []string `json:"required_scopes"`
	OutputViewID   string   `json:"output_view_id,omitempty"`
}

type Descriptor struct {
	SchemaVersion int                    `json:"schema_version"`
	ID            string                 `json:"id"`
	Version       string                 `json:"version"`
	Provider      string                 `json:"provider"`
	Execution     map[string]any         `json:"execution"`
	Capabilities  []Capability           `json:"capabilities"`
	Views         []views.ViewDescriptor `json:"views"`
}

type SourceAuthority struct {
	ExecutionOwner     string `json:"execution_owner"`
	Incarnation        string `json:"incarnation"`
	Epoch              uint64 `json:"epoch"`
	IdentityUnverified bool   `json:"identity_unverified"`
}

type Connection struct {
	Authority           *SourceAuthority `json:"authority,omitempty"`
	SchemaVersion       int              `json:"schema_version"`
	ConnectorID         string           `json:"connector_id"`
	ConnectionID        string           `json:"connection_id,omitempty"`
	PersonID            string           `json:"person_id,omitempty"`
	DeviceBinding       *DeviceBinding   `json:"device_binding,omitempty"`
	State               string           `json:"state"`
	GrantedScopes       []string         `json:"granted_scopes"`
	ObservedAtUnixMS    int64            `json:"observed_at_unix_ms"`
	LastSuccessAtUnixMS *int64           `json:"last_success_at_unix_ms,omitempty"`
	LastFailure         *Failure         `json:"last_failure,omitempty"`
}

type Failure struct {
	Kind             string `json:"kind"`
	ObservedAtUnixMS int64  `json:"observed_at_unix_ms"`
}

type DeviceBinding struct {
	DeviceID string `json:"device_id"`
}

type Snapshot struct {
	Descriptor Descriptor           `json:"descriptor"`
	Connection Connection           `json:"connection"`
	Views      []views.ViewSnapshot `json:"views"`
}
