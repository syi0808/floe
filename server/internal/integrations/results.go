package integrations

type ConnectionsResult struct {
	SchemaVersion int
	PersonID      string
	DeviceID      string
	Connections   []Snapshot
}

type ConnectorCatalogEntry struct {
	ID, Name, AuthKind string
	Available          bool
	Status             string
	RequiredScopes     []string
	ScopeFields        []string
	Capabilities       CapabilitySet
	HasConnection      bool
	ConnectionID       string
	ConnectionRevision uint64
	Incarnation        string
	Epoch              uint64
	ExecutionOwner     string
	IdentityUnverified bool
	Scope              map[string]any
}

type CatalogResult struct {
	SchemaVersion int
	PersonID      string
	DeviceID      string
	Connectors    []ConnectorCatalogEntry
	Revision      uint64
}

type AttemptResult struct {
	SchemaVersion int
	OperationID   string
	ConnectorID   string
	ConnectionID  string
	PersonID      string
	DeviceID      string
	SetupState    string
	ManagementRef string
	Revision      uint64
}

type ScopeResult struct {
	SchemaVersion      int
	PersonID           string
	DeviceID           string
	ConnectionID       string
	ConnectionRevision uint64
	ConnectorID        string
	Scope              map[string]any
}

type DisconnectResult struct {
	SchemaVersion      int
	OperationID        string
	PersonID           string
	DeviceID           string
	ConnectionID       string
	ConnectorID        string
	ConnectionRevision uint64
	CleanupState       string
}
