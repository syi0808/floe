package connections

import (
	"time"
)

type Attempt struct {
	ID               string
	ClientID         string
	ConnectorID      string
	ConnectionID     string
	PersonID         string
	DeviceID         string
	Status           string
	AuthorizationURL string
	UserCode         string
	ErrorCode        string
	CreatedAt        time.Time
	Scope            map[string]any
	Credential       string
	Polling          bool
}
