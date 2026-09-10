package homeassistant

import (
	"context"
	"errors"
	"sync"
	"time"

	"floe/server/internal/connectors/common"
)

type Service struct {
	client    *Client
	entities  []string
	clock     func() time.Time
	operation sync.Mutex
	last      *LogisticsView
}

func NewService(client *Client, entities []string) (*Service, error) {
	if client == nil || len(entities) == 0 || len(entities) > maxEntities {
		return nil, ErrInvalidInput
	}
	selected := append([]string(nil), entities...)
	seen := map[string]bool{}
	for _, entity := range selected {
		if !allowedEntity.MatchString(entity) || seen[entity] {
			return nil, ErrInvalidInput
		}
		seen[entity] = true
	}
	return &Service{client: client, entities: selected, clock: time.Now}, nil
}

func (service *Service) ReadLogisticsView(ctx context.Context) (common.LogisticsView, error) {
	service.operation.Lock()
	defer service.operation.Unlock()
	view, err := service.client.Logistics(ctx, service.entities, service.clock())
	if err == nil {
		service.last = &view
	}
	return view, err
}

func (service *Service) ConnectionSnapshot(ctx context.Context) (any, error) {
	service.operation.Lock()
	defer service.operation.Unlock()
	now := service.clock()
	view, err := service.client.Logistics(ctx, service.entities, now)
	if err != nil {
		return failureSnapshot(now, service.last, err), nil
	}
	service.last = &view
	return ConnectionSnapshot(view)
}

func failureSnapshot(now time.Time, last *LogisticsView, err error) common.Snapshot {
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
			ConnectorID:      "home_assistant.states",
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
