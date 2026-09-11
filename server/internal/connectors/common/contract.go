package common

type Capability struct {
	SchemaVersion  int      `json:"schema_version"`
	ID             string   `json:"id"`
	Version        string   `json:"version"`
	Authority      string   `json:"authority"`
	RequiredScopes []string `json:"required_scopes"`
	OutputViewID   string   `json:"output_view_id,omitempty"`
}

type ViewDescriptor struct {
	SchemaVersion      int    `json:"schema_version"`
	ID                 string `json:"id"`
	Version            string `json:"version"`
	DataClass          string `json:"data_class"`
	Retention          string `json:"retention"`
	FreshnessTTLMS     int64  `json:"freshness_ttl_ms"`
	MaxItems           int    `json:"max_items"`
	MaxBytes           int    `json:"max_bytes"`
	ProvenanceRequired bool   `json:"provenance_required"`
}

type Descriptor struct {
	SchemaVersion int              `json:"schema_version"`
	ID            string           `json:"id"`
	Version       string           `json:"version"`
	Provider      string           `json:"provider"`
	Execution     map[string]any   `json:"execution"`
	Capabilities  []Capability     `json:"capabilities"`
	Views         []ViewDescriptor `json:"views"`
}

type Failure struct {
	Kind             string `json:"kind"`
	ObservedAtUnixMS int64  `json:"observed_at_unix_ms"`
}

type Connection struct {
	SchemaVersion       int            `json:"schema_version"`
	ConnectorID         string         `json:"connector_id"`
	ConnectionID        string         `json:"connection_id,omitempty"`
	PersonID            string         `json:"person_id,omitempty"`
	DeviceBinding       *DeviceBinding `json:"device_binding,omitempty"`
	State               string         `json:"state"`
	GrantedScopes       []string       `json:"granted_scopes"`
	ObservedAtUnixMS    int64          `json:"observed_at_unix_ms"`
	LastSuccessAtUnixMS *int64         `json:"last_success_at_unix_ms,omitempty"`
	LastFailure         *Failure       `json:"last_failure,omitempty"`
}

type DeviceBinding struct {
	DeviceID string `json:"device_id"`
}

type ViewSnapshot struct {
	SchemaVersion    int    `json:"schema_version"`
	ViewID           string `json:"view_id"`
	SourceHandle     string `json:"source_handle"`
	ObservedAtUnixMS int64  `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS  int64  `json:"expires_at_unix_ms"`
	ItemCount        int    `json:"item_count"`
	ByteCount        int    `json:"byte_count"`
	ProvenanceCount  int    `json:"provenance_count"`
}

type Snapshot struct {
	Descriptor Descriptor     `json:"descriptor"`
	Connection Connection     `json:"connection"`
	Views      []ViewSnapshot `json:"views"`
}
