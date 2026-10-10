package microsoftteams

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
	team          string
	channel       string
	clock         func() time.Time
	readGate      chan struct{}
	cache         sync.Mutex
	connection    integrations.Connection
	viewSnapshots []views.ViewSnapshot
}

func NewService(client *Client, team, channel string) (*Service, error) {
	if client == nil || !selectionPattern.MatchString(team) || !selectionPattern.MatchString(channel) {
		return nil, ErrInvalidInput
	}
	return &Service{
		client:        client,
		team:          team,
		channel:       channel,
		clock:         time.Now,
		readGate:      make(chan struct{}, 1),
		connection:    integrations.Connection{SchemaVersion: 1, ConnectorID: "microsoft.teams", State: "unavailable", GrantedScopes: []string{}},
		viewSnapshots: []views.ViewSnapshot{},
	}, nil
}

func (service *Service) Read(ctx context.Context, request views.ReadRequest) (views.Result, error) {
	query := request.Query.Work
	if request.Query.ViewID != views.WorkContext || query == nil || query.SchemaVersion != 1 || request.Query.Calendar != nil || request.Query.Mail != nil || request.Query.Logistics != nil || !validReadBounds(request.Bounds) {
		return views.Result{}, views.ReadError{Kind: views.InvalidQuery}
	}
	if err := service.acquireRead(ctx); err != nil {
		service.recordFailure(err, service.clock())
		return views.Result{}, err
	}
	defer service.releaseRead()
	now := service.clock()
	if err := ctx.Err(); err != nil {
		service.recordFailure(err, service.clock())
		return views.Result{}, err
	}
	view, err := service.client.WorkContext(ctx, service.team, service.channel, now)
	if ctxErr := ctx.Err(); ctxErr != nil {
		err = ctxErr
	}
	if err != nil {
		service.recordFailure(err, service.clock())
		return views.Result{}, normalizeReadError(err)
	}
	result := views.Result{ViewID: views.WorkContext, Work: &view}
	if err := ctx.Err(); err != nil {
		service.recordFailure(err, service.clock())
		return views.Result{}, err
	}
	if _, _, err = views.EncodeBounded(result, request.Bounds); err != nil {
		service.recordFailure(err, service.clock())
		return views.Result{}, normalizeReadError(err)
	}
	if err := ctx.Err(); err != nil {
		service.recordFailure(err, service.clock())
		return views.Result{}, err
	}
	snapshot, err := ConnectionSnapshot(view)
	if err != nil {
		service.recordFailure(err, service.clock())
		return views.Result{}, normalizeReadError(err)
	}
	if err := ctx.Err(); err != nil {
		service.recordFailure(err, service.clock())
		return views.Result{}, err
	}
	if err := service.recordSuccess(ctx, snapshot); err != nil {
		return views.Result{}, err
	}
	return result, nil
}

func (service *Service) Snapshot(ctx context.Context) (integrations.Snapshot, error) {
	if err := ctx.Err(); err != nil {
		return integrations.Snapshot{}, err
	}
	service.cache.Lock()
	defer service.cache.Unlock()
	if err := ctx.Err(); err != nil {
		return integrations.Snapshot{}, err
	}
	now := service.clock()
	connection := cloneConnection(service.connection)
	viewSnapshots := append([]views.ViewSnapshot{}, service.viewSnapshots...)
	if len(viewSnapshots) > 0 && viewSnapshots[0].ExpiresAtUnixMS <= now.UnixMilli() {
		viewSnapshots = []views.ViewSnapshot{}
		if connection.LastSuccessAtUnixMS != nil {
			connection.State = "degraded"
		}
	}
	snapshot := integrations.Snapshot{Descriptor: ConnectorDescriptor(), Connection: connection, Views: viewSnapshots}
	if err := ctx.Err(); err != nil {
		return integrations.Snapshot{}, err
	}
	return snapshot, nil
}

func (service *Service) recordSuccess(ctx context.Context, snapshot integrations.Snapshot) error {
	service.cache.Lock()
	defer service.cache.Unlock()
	if err := ctx.Err(); err != nil {
		service.recordFailureLocked(err, service.clock())
		return err
	}
	service.connection = cloneConnection(snapshot.Connection)
	service.viewSnapshots = append([]views.ViewSnapshot{}, snapshot.Views...)
	return nil
}

func (service *Service) acquireRead(ctx context.Context) error {
	select {
	case service.readGate <- struct{}{}:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (service *Service) releaseRead() {
	<-service.readGate
}

func (service *Service) recordFailure(err error, observedAt time.Time) {
	service.cache.Lock()
	defer service.cache.Unlock()
	service.recordFailureLocked(err, observedAt)
}

func (service *Service) recordFailureLocked(err error, observedAt time.Time) {
	state, kind := readFailureMetadata(err)
	if service.connection.LastSuccessAtUnixMS != nil {
		state = "degraded"
	}
	observed := observedAt.UnixMilli()
	service.connection.State = state
	service.connection.ObservedAtUnixMS = observed
	service.connection.LastFailure = &integrations.Failure{Kind: kind, ObservedAtUnixMS: observed}
}

func validReadBounds(bounds views.Bounds) bool {
	return bounds.MaxItems > 0 && bounds.MaxItems <= maxMessages && bounds.MaxBytes > 0 && bounds.MaxBytes <= 65_536
}

func normalizeReadError(err error) error {
	switch {
	case errors.Is(err, context.Canceled):
		return context.Canceled
	case errors.Is(err, context.DeadlineExceeded):
		return context.DeadlineExceeded
	case errors.Is(err, ErrInvalidInput):
		return views.ReadError{Kind: views.InvalidQuery}
	case errors.Is(err, ErrCredentialExpired):
		return views.ReadError{Kind: views.CredentialExpired}
	case errors.Is(err, ErrPermissionDenied):
		return views.ReadError{Kind: views.PermissionDenied}
	case errors.Is(err, ErrRateLimited):
		return views.ReadError{Kind: views.RateLimited}
	case errors.Is(err, ErrInvalidResponse), errors.Is(err, views.ErrInvalid):
		return views.ReadError{Kind: views.InvalidProviderResponse}
	default:
		return views.ReadError{Kind: views.Unavailable}
	}
}

func readFailureMetadata(err error) (string, string) {
	switch {
	case errors.Is(err, ErrCredentialExpired):
		return "revoked", "credential_expired"
	case errors.Is(err, ErrPermissionDenied):
		return "unavailable", "permission_denied"
	case errors.Is(err, ErrRateLimited):
		return "unavailable", "rate_limited"
	case errors.Is(err, ErrInvalidResponse), errors.Is(err, views.ErrInvalid):
		return "unavailable", "partial_fetch"
	case errors.Is(err, ErrInvalidInput):
		return "unavailable", "invalid_query"
	default:
		return "unavailable", "unavailable"
	}
}

func cloneConnection(connection integrations.Connection) integrations.Connection {
	connection.GrantedScopes = append([]string{}, connection.GrantedScopes...)
	if connection.LastSuccessAtUnixMS != nil {
		lastSuccess := *connection.LastSuccessAtUnixMS
		connection.LastSuccessAtUnixMS = &lastSuccess
	}
	if connection.LastFailure != nil {
		lastFailure := *connection.LastFailure
		connection.LastFailure = &lastFailure
	}
	if connection.DeviceBinding != nil {
		deviceBinding := *connection.DeviceBinding
		connection.DeviceBinding = &deviceBinding
	}
	return connection
}
