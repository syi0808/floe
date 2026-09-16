package connections

import (
	"context"
	"time"

	"floe/server/internal/connectors/common"
)

// Owner-defined ports for the concrete connector runtimes.
//
// Connections decides connection intent and state; the real provider calls stay
// in the connector implementations behind these ports.

type ConnectorAuthRuntime interface {
	ConnectorOAuthRuntime
	ConnectionSnapshot() (any, error)
	ReadCommunicationView(string, int, int) (any, error)
	ReadLogisticsView(context.Context) (common.LogisticsView, error)
}

type ConnectorOAuthRuntime interface {
	Action(context.Context, string) (any, error)
	BindCredential(string) error
	Ready() bool
}

type ProviderIdentityRuntime interface {
	ProviderIdentity(context.Context) (string, error)
}

type ProviderIdentityStatusRuntime interface {
	ProviderIdentityStatus() (string, bool)
}

type ProviderIdentityFenceRuntime interface {
	WithVerifiedProviderIdentity(string, string, func() error) error
}

type CommunicationRuntime interface {
	ConnectionSnapshot(context.Context) (any, error)
	ReadCommunicationView(context.Context, string, int, int) (any, error)
}

type CalendarRuntime interface {
	ConnectionSnapshot(context.Context) (any, error)
	ReadCalendarView(context.Context, time.Time, time.Time, string, int) (any, error)
}

type WorkContextRuntime interface {
	ConnectionSnapshot(context.Context) (any, error)
	ReadWorkContextView(context.Context) (common.WorkContextView, error)
}

type LogisticsViewReader interface {
	ReadLogisticsView(context.Context) (common.LogisticsView, error)
}

type LogisticsRuntime interface {
	LogisticsViewReader
	ConnectionSnapshot(context.Context) (any, error)
}

type DriveAuthRuntime interface {
	ConnectorOAuthRuntime
	Token(context.Context) (string, error)
}

// A device bound to a Person-owned connection.
type deviceBinding struct {
	DeviceID string `json:"device_id"`
}

type connectionRecord struct {
	ConnectionID       string         `json:"connection_id"`
	Revision           uint64         `json:"revision"`
	ConnectorID        string         `json:"connector_id"`
	PersonID           string         `json:"person_id"`
	Device             *deviceBinding `json:"device_binding,omitempty"`
	Scope              map[string]any `json:"scope"`
	Credential         string         `json:"credential,omitempty"`
	Incarnation        string         `json:"incarnation"`
	Epoch              uint64         `json:"epoch"`
	ProviderIdentity   string         `json:"provider_identity,omitempty"`
	IdentityUnverified bool           `json:"identity_unverified,omitempty"`
}
