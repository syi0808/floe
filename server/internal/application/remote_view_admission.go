package application

import (
	"context"

	"floe/server/internal/connections"
)

type legacyCommunicationAdapter struct {
	runtime connections.ConnectorAuthRuntime
}

func (adapter legacyCommunicationAdapter) ConnectionSnapshot(context.Context) (any, error) {
	return adapter.runtime.ConnectionSnapshot()
}
func (adapter legacyCommunicationAdapter) ReadCommunicationView(_ context.Context, query string, cursor, limit int) (any, error) {
	return adapter.runtime.ReadCommunicationView(query, cursor, limit)
}
