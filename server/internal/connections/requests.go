package connections

type ConnectRequest struct {
	SchemaVersion int            `json:"schema_version"`
	Secret        string         `json:"secret"`
	Scope         map[string]any `json:"scope"`
}
type ScopeRequest struct {
	SchemaVersion      int            `json:"schema_version"`
	ConnectionID       string         `json:"connection_id"`
	ConnectionRevision uint64         `json:"connection_revision"`
	Scope              map[string]any `json:"scope"`
}
type DisconnectRequest struct {
	SchemaVersion      int    `json:"schema_version"`
	ConnectionID       string `json:"connection_id"`
	ConnectionRevision uint64 `json:"connection_revision"`
}
