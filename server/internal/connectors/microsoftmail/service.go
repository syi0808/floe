package microsoftmail

import (
	"context"
	"errors"
	"sync"
	"time"

	"floe/server/internal/connectors/common"
)

type Service struct {
	client    *Client
	clock     func() time.Time
	operation sync.Mutex
	last      *CommunicationView
}

func NewService(client *Client) (*Service, error) {
	if client == nil {
		return nil, ErrInvalidInput
	}
	return &Service{client: client, clock: time.Now}, nil
}

func (service *Service) ReadCommunicationView(ctx context.Context, query string, cursor, limit int) (any, error) {
	service.operation.Lock()
	defer service.operation.Unlock()
	view, err := service.client.Communication(ctx, query, cursor, limit, service.clock())
	if err == nil {
		service.last = &view
	}
	return view, err
}

func (service *Service) ConnectionSnapshot(ctx context.Context) (any, error) {
	service.operation.Lock()
	defer service.operation.Unlock()
	now := service.clock()
	view, err := service.client.Communication(ctx, "", 0, 25, now)
	if err == nil {
		service.last = &view
		return ConnectionSnapshot(view)
	}
	state, kind := "unavailable", "unavailable"
	switch {
	case errors.Is(err, ErrCredentialExpired):
		state, kind = "revoked", "credential_expired"
	case errors.Is(err, ErrPermissionDenied):
		kind = "permission_denied"
	case errors.Is(err, ErrRateLimited):
		kind = "rate_limited"
	case errors.Is(err, ErrInvalidResponse):
		kind = "partial_fetch"
	}
	observed := now.UnixMilli()
	snapshot := common.Snapshot{Descriptor: ConnectorDescriptor(), Connection: common.Connection{SchemaVersion: 1, ConnectorID: "microsoft.mail", State: state, ObservedAtUnixMS: observed, LastFailure: &common.Failure{Kind: kind, ObservedAtUnixMS: observed}}, Views: []common.ViewSnapshot{}}
	if service.last != nil {
		snapshot.Connection.State = "degraded"
		snapshot.Connection.GrantedScopes = []string{observeScope}
		snapshot.Connection.LastSuccessAtUnixMS = &service.last.ObservedAtUnixMS
		if service.last.ExpiresAtUnixMS > observed {
			ready, readyErr := ConnectionSnapshot(*service.last)
			if readyErr == nil {
				snapshot.Views = ready.Views
			}
		}
	}
	return snapshot, nil
}
