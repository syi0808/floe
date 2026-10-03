package integrations

type ConnectRequest struct {
	SchemaVersion           int            `json:"schema_version"`
	OperationID             string         `json:"operation_id"`
	ExpectedCatalogRevision uint64         `json:"expected_catalog_revision"`
	Scope                   map[string]any `json:"scope"`
}
type ScopeRequest struct {
	SchemaVersion      int            `json:"schema_version"`
	ConnectionID       string         `json:"connection_id"`
	ConnectionRevision uint64         `json:"connection_revision"`
	Scope              map[string]any `json:"scope"`
}
type DisconnectRequest struct {
	OperationID        string `json:"operation_id"`
	SchemaVersion      int    `json:"schema_version"`
	ConnectionID       string `json:"connection_id"`
	ConnectionRevision uint64 `json:"connection_revision"`
}

type CancelSetupRequest struct {
	SchemaVersion    int    `json:"schema_version"`
	OperationID      string `json:"operation_id"`
	ExpectedRevision uint64 `json:"expected_revision"`
}
