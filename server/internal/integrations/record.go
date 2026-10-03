package integrations

type Record struct {
	ConnectionID       string         `json:"connection_id"`
	Revision           uint64         `json:"revision"`
	ConnectorID        string         `json:"connector_id"`
	PersonID           string         `json:"person_id"`
	Device             *DeviceBinding `json:"device_binding,omitempty"`
	Scope              map[string]any `json:"scope"`
	Credential         string         `json:"credential,omitempty"`
	Incarnation        string         `json:"incarnation"`
	Epoch              uint64         `json:"epoch"`
	ProviderIdentity   string         `json:"provider_identity,omitempty"`
	IdentityUnverified bool           `json:"identity_unverified,omitempty"`
}
