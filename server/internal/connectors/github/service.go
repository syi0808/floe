package github

import (
	"context"
	"errors"
	"sync"
	"time"

	"floe/server/internal/connectors/common"
)

type Service struct {
	client     *Client
	owner      string
	repository string
	clock      func() time.Time
	operation  sync.Mutex
	last       *WorkContextView
}

func NewService(client *Client, owner, repository string) (*Service, error) {
	if client == nil || !identifier.MatchString(owner) || !identifier.MatchString(repository) {
		return nil, ErrInvalidInput
	}
	return &Service{client: client, owner: owner, repository: repository, clock: time.Now}, nil
}

func (service *Service) ReadWorkContextView(ctx context.Context) (any, error) {
	service.operation.Lock()
	defer service.operation.Unlock()
	view, err := service.client.WorkContext(ctx, service.owner, service.repository, service.clock())
	if err == nil {
		service.last = &view
	}
	return view, err
}

func (service *Service) ConnectionSnapshot(ctx context.Context) (any, error) {
	service.operation.Lock()
	defer service.operation.Unlock()
	now := service.clock()
	view, err := service.client.WorkContext(ctx, service.owner, service.repository, now)
	if err != nil {
		return failureSnapshot(now, service.last, err), nil
	}
	service.last = &view
	return ConnectionSnapshot(view)
}

func failureSnapshot(now time.Time, last *WorkContextView, err error) common.Snapshot {
	state, kind := "unavailable", "unavailable"
	switch {
	case errors.Is(err, ErrCredentialExpired):
		state, kind = "revoked", "credential_expired"
	case errors.Is(err, ErrRateLimited):
		kind = "rate_limited"
	case errors.Is(err, ErrInvalidResponse):
		kind = "partial_fetch"
	}
	observed := now.UnixMilli()
	snapshot := common.Snapshot{
		Descriptor: ConnectorDescriptor(),
		Connection: common.Connection{
			SchemaVersion:    1,
			ConnectorID:      "github.issues",
			State:            state,
			ObservedAtUnixMS: observed,
			LastFailure:      &common.Failure{Kind: kind, ObservedAtUnixMS: observed},
		},
		Views: []common.ViewSnapshot{},
	}
	if last != nil {
		snapshot.Connection.State = "degraded"
		snapshot.Connection.GrantedScopes = []string{observeScope}
		snapshot.Connection.LastSuccessAtUnixMS = &last.ObservedAtUnixMS
		if last.ExpiresAtUnixMS > observed {
			ready, readyErr := ConnectionSnapshot(*last)
			if readyErr == nil {
				snapshot.Views = ready.Views
			}
		}
	}
	return snapshot
}
