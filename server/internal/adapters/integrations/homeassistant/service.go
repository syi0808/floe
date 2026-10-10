package homeassistant

import (
	"context"
	"errors"
	"floe/server/internal/integrations"
	"floe/server/internal/views"
	"sync"
	"time"
)

type Service struct {
	client        *Client
	entities      []string
	clock         func() time.Time
	readGate      chan struct{}
	cacheMu       sync.Mutex
	hasRead       bool
	lastState     string
	lastReadAt    int64
	lastSuccessAt *int64
	lastView      *views.ViewSnapshot
	lastFailure   *integrations.Failure
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
	return &Service{client: client, entities: selected, clock: time.Now, readGate: make(chan struct{}, 1)}, nil
}

func (service *Service) Read(ctx context.Context, request views.ReadRequest) (views.Result, error) {
	if ctx == nil || !validReadRequest(request) {
		return views.Result{}, views.ReadError{Kind: views.InvalidQuery}
	}

	if err := service.acquireRead(ctx); err != nil {
		return views.Result{}, err
	}
	defer service.releaseRead()

	readAt := service.clock()
	view, err := service.client.Logistics(ctx, service.entities, readAt)
	if err != nil {
		err = normalizeReadError(ctx, err)
		service.recordFailure(readAt, err)
		return views.Result{}, err
	}
	if err := ctx.Err(); err != nil {
		service.recordFailure(readAt, err)
		return views.Result{}, err
	}

	result := views.Result{ViewID: views.Logistics, Logistics: &view}
	if _, _, err := views.EncodeBounded(result, request.Bounds); err != nil {
		readErr := views.ReadError{Kind: views.InvalidProviderResponse}
		service.recordFailure(readAt, readErr)
		return views.Result{}, readErr
	}
	snapshot, err := snapshotFromView(view)
	if err != nil {
		readErr := views.ReadError{Kind: views.InvalidProviderResponse}
		service.recordFailure(readAt, readErr)
		return views.Result{}, readErr
	}
	if err := service.finishSuccess(ctx, readAt, snapshot); err != nil {
		return views.Result{}, err
	}
	return result, nil
}

func (service *Service) Snapshot(ctx context.Context) (integrations.Snapshot, error) {
	if ctx == nil {
		return integrations.Snapshot{}, context.Canceled
	}
	if err := ctx.Err(); err != nil {
		return integrations.Snapshot{}, err
	}

	service.cacheMu.Lock()
	hasRead, lastState, lastReadAt := service.hasRead, service.lastState, service.lastReadAt
	var lastSuccessAt *int64
	if service.lastSuccessAt != nil {
		value := *service.lastSuccessAt
		lastSuccessAt = &value
	}
	var lastView *views.ViewSnapshot
	if service.lastView != nil {
		value := *service.lastView
		lastView = &value
	}
	var lastFailure *integrations.Failure
	if service.lastFailure != nil {
		value := *service.lastFailure
		lastFailure = &value
	}
	service.cacheMu.Unlock()
	if err := ctx.Err(); err != nil {
		return integrations.Snapshot{}, err
	}

	snapshot := integrations.Snapshot{
		Descriptor: ConnectorDescriptor(),
		Connection: integrations.Connection{
			SchemaVersion: 1,
			ConnectorID:   "home_assistant.states",
			State:         "unavailable",
			GrantedScopes: []string{},
		},
		Views: []views.ViewSnapshot{},
	}
	if !hasRead {
		if err := ctx.Err(); err != nil {
			return integrations.Snapshot{}, err
		}
		return snapshot, nil
	}

	snapshot.Connection.State = lastState
	snapshot.Connection.ObservedAtUnixMS = lastReadAt
	if lastSuccessAt != nil {
		lastSuccess := *lastSuccessAt
		snapshot.Connection.LastSuccessAtUnixMS = &lastSuccess
		snapshot.Connection.GrantedScopes = []string{observeScope}
	}
	if lastFailure != nil {
		failure := *lastFailure
		snapshot.Connection.LastFailure = &failure
	}
	if lastView != nil {
		if lastView.ExpiresAtUnixMS > service.clock().UnixMilli() {
			snapshot.Views = append(snapshot.Views, *lastView)
		} else if lastState == "ready" {
			snapshot.Connection.State = "stale"
		}
	}
	if err := ctx.Err(); err != nil {
		return integrations.Snapshot{}, err
	}
	return snapshot, nil
}

func (service *Service) acquireRead(ctx context.Context) error {
	select {
	case service.readGate <- struct{}{}:
		if err := ctx.Err(); err != nil {
			service.releaseRead()
			return err
		}
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (service *Service) releaseRead() {
	<-service.readGate
}

func validReadRequest(request views.ReadRequest) bool {
	query := request.Query
	if query.ViewID != views.Logistics || query.Logistics == nil || query.Logistics.SchemaVersion != 1 || query.Calendar != nil || query.Mail != nil || query.Work != nil {
		return false
	}
	registered := ConnectorDescriptor().Views[0]
	return request.Bounds.MaxItems > 0 && request.Bounds.MaxItems <= uint32(registered.MaxItems) && request.Bounds.MaxBytes > 0 && request.Bounds.MaxBytes <= uint32(registered.MaxBytes)
}

func normalizeReadError(ctx context.Context, err error) error {
	if ctx != nil && ctx.Err() != nil {
		return ctx.Err()
	}
	switch {
	case errors.Is(err, context.Canceled):
		return context.Canceled
	case errors.Is(err, context.DeadlineExceeded):
		return context.DeadlineExceeded
	case errors.Is(err, ErrCredentialExpired):
		return views.ReadError{Kind: views.CredentialExpired}
	case errors.Is(err, ErrRateLimited):
		return views.ReadError{Kind: views.RateLimited}
	case errors.Is(err, ErrInvalidResponse):
		return views.ReadError{Kind: views.InvalidProviderResponse}
	case errors.Is(err, ErrInvalidInput):
		return views.ReadError{Kind: views.InvalidQuery}
	default:
		return views.ReadError{Kind: views.Unavailable}
	}
}

func (service *Service) finishSuccess(ctx context.Context, observedAt time.Time, snapshot integrations.Snapshot) error {
	service.cacheMu.Lock()
	defer service.cacheMu.Unlock()
	if err := ctx.Err(); err != nil {
		service.recordFailureLocked(observedAt, err)
		return err
	}
	service.recordSuccessLocked(snapshot)
	return nil
}

func (service *Service) recordSuccessLocked(snapshot integrations.Snapshot) {
	service.hasRead = true
	service.lastState = "ready"
	service.lastReadAt = snapshot.Connection.ObservedAtUnixMS
	lastSuccess := *snapshot.Connection.LastSuccessAtUnixMS
	service.lastSuccessAt = &lastSuccess
	view := snapshot.Views[0]
	service.lastView = &view
	service.lastFailure = nil
}

func (service *Service) recordFailure(observedAt time.Time, err error) {
	service.cacheMu.Lock()
	defer service.cacheMu.Unlock()
	service.recordFailureLocked(observedAt, err)
}

func (service *Service) recordFailureLocked(observedAt time.Time, err error) {
	state, kind := failureDetails(err)
	if service.lastSuccessAt != nil {
		state = "degraded"
	}
	observed := observedAt.UnixMilli()
	service.hasRead = true
	service.lastState = state
	service.lastReadAt = observed
	service.lastFailure = &integrations.Failure{Kind: kind, ObservedAtUnixMS: observed}
}

func failureDetails(err error) (string, string) {
	state, kind := "unavailable", string(views.Unavailable)
	var readErr views.ReadError
	if errors.As(err, &readErr) {
		switch readErr.Kind {
		case views.CredentialExpired:
			state = "revoked"
			kind = "credential_expired"
		case views.RateLimited:
			kind = "rate_limited"
		case views.InvalidProviderResponse:
			kind = "partial_fetch"
		default:
			kind = string(readErr.Kind)
		}
	}
	return state, kind
}
