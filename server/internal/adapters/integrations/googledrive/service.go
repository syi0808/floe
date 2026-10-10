package googledrive

import (
	"context"
	"errors"
	"floe/server/internal/integrations"
	"floe/server/internal/views"
	"sync"
	"time"
)

type Service struct {
	client    *Client
	folderID  string
	clock     func() time.Time
	operation chan struct{}

	statusMu            sync.RWMutex
	observedAtUnixMS    int64
	lastSuccessAtUnixMS *int64
	lastFailure         *integrations.Failure
	lastView            *views.ViewSnapshot
}

func NewService(client *Client, folderID string) (*Service, error) {
	if client == nil || !idPattern.MatchString(folderID) {
		return nil, ErrInvalidInput
	}
	return &Service{client: client, folderID: folderID, clock: time.Now, operation: make(chan struct{}, 1)}, nil
}

func (service *Service) Read(ctx context.Context, request views.ReadRequest) (views.Result, error) {
	if ctx == nil {
		return views.Result{}, readError(views.InvalidQuery)
	}
	if err := ctx.Err(); err != nil {
		return views.Result{}, err
	}
	query := request.Query.Work
	if request.Query.ViewID != views.WorkContext || query == nil || request.Query.Calendar != nil || request.Query.Mail != nil || request.Query.Logistics != nil || query.SchemaVersion != 1 || !validReadBounds(request.Bounds) {
		return views.Result{}, readError(views.InvalidQuery)
	}
	itemLimit := min(int(request.Bounds.MaxItems), maxFiles)
	byteLimit := min(int(request.Bounds.MaxBytes), 65_536)
	if err := service.acquire(ctx); err != nil {
		return service.readFailure(ctx, err)
	}
	defer service.release()
	if err := ctx.Err(); err != nil {
		return service.readFailure(ctx, err)
	}

	view, err := service.client.workContext(ctx, service.folderID, service.clock(), itemLimit)
	if err != nil {
		return service.readFailure(ctx, err)
	}
	if err := ctx.Err(); err != nil {
		return service.readFailure(ctx, err)
	}
	result := views.Result{ViewID: views.WorkContext, Work: &view}
	encoded, _, err := views.EncodeBounded(result, views.Bounds{MaxItems: uint32(itemLimit), MaxBytes: uint32(byteLimit)})
	if err != nil || len(encoded) > byteLimit || len(view.Items) > itemLimit {
		return service.readFailure(ctx, ErrInvalidResponse)
	}
	if err := ctx.Err(); err != nil {
		return service.readFailure(ctx, err)
	}
	metadata := views.ViewSnapshot{
		SchemaVersion:    1,
		ViewID:           view.ViewID,
		SourceHandle:     view.SourceHandle,
		ObservedAtUnixMS: view.ObservedAtUnixMS,
		ExpiresAtUnixMS:  view.ExpiresAtUnixMS,
		ItemCount:        len(view.Items),
		ByteCount:        len(encoded),
		ProvenanceCount:  len(view.Items),
	}
	if err := ctx.Err(); err != nil {
		return service.readFailure(ctx, err)
	}
	if err := service.recordSuccess(ctx, metadata, view.ObservedAtUnixMS); err != nil {
		return service.readFailure(ctx, err)
	}
	return result, nil
}

func (service *Service) Snapshot(ctx context.Context) (integrations.Snapshot, error) {
	if ctx == nil {
		return integrations.Snapshot{}, ErrInvalidInput
	}
	if err := ctx.Err(); err != nil {
		return integrations.Snapshot{}, err
	}

	service.statusMu.RLock()
	observed := service.observedAtUnixMS
	var lastSuccess *int64
	if service.lastSuccessAtUnixMS != nil {
		value := *service.lastSuccessAtUnixMS
		lastSuccess = &value
	}
	var lastFailure *integrations.Failure
	if service.lastFailure != nil {
		value := *service.lastFailure
		lastFailure = &value
	}
	var lastView *views.ViewSnapshot
	if service.lastView != nil {
		value := *service.lastView
		lastView = &value
	}
	service.statusMu.RUnlock()
	if err := ctx.Err(); err != nil {
		return integrations.Snapshot{}, err
	}
	now := service.clock().UnixMilli()
	stale := lastView != nil && lastView.ExpiresAtUnixMS <= now
	if stale && lastFailure == nil {
		lastFailure = &integrations.Failure{Kind: "stale", ObservedAtUnixMS: now}
	}
	if observed == 0 {
		observed = now
	}

	state := "pending"
	if lastSuccess != nil {
		state = "ready"
	}
	if lastFailure != nil {
		if lastSuccess != nil {
			state = "degraded"
		} else if lastFailure.Kind == "credential_expired" {
			state = "revoked"
		} else {
			state = "unavailable"
		}
	}
	connection := integrations.Connection{
		SchemaVersion:    1,
		ConnectorID:      "google_drive.files",
		State:            state,
		GrantedScopes:    []string{},
		ObservedAtUnixMS: observed,
		LastFailure:      lastFailure,
	}
	if lastSuccess != nil {
		connection.GrantedScopes = []string{observeScope}
		connection.LastSuccessAtUnixMS = lastSuccess
	}
	viewsSnapshot := []views.ViewSnapshot{}
	if lastView != nil && lastView.ExpiresAtUnixMS > now {
		viewsSnapshot = append(viewsSnapshot, *lastView)
	}
	if err := ctx.Err(); err != nil {
		return integrations.Snapshot{}, err
	}
	return integrations.Snapshot{Descriptor: ConnectorDescriptor(), Connection: connection, Views: viewsSnapshot}, nil
}

func (service *Service) readFailure(ctx context.Context, err error) (views.Result, error) {
	if ctx != nil && ctx.Err() != nil {
		err = ctx.Err()
	}
	normalized := normalizeReadError(ctx, err)
	service.recordFailure(failureKind(err), service.clock().UnixMilli())
	return views.Result{}, normalized
}

func (service *Service) recordSuccess(ctx context.Context, view views.ViewSnapshot, observed int64) error {
	service.statusMu.Lock()
	defer service.statusMu.Unlock()
	if err := ctx.Err(); err != nil {
		return err
	}
	service.observedAtUnixMS = observed
	service.lastSuccessAtUnixMS = &observed
	service.lastFailure = nil
	service.lastView = &view
	return nil
}

func (service *Service) recordFailure(kind string, observed int64) {
	service.statusMu.Lock()
	defer service.statusMu.Unlock()
	if service.lastSuccessAtUnixMS != nil && observed < *service.lastSuccessAtUnixMS {
		observed = *service.lastSuccessAtUnixMS
	}
	service.observedAtUnixMS = observed
	service.lastFailure = &integrations.Failure{Kind: kind, ObservedAtUnixMS: observed}
}

func (service *Service) acquire(ctx context.Context) error {
	select {
	case service.operation <- struct{}{}:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (service *Service) release() { <-service.operation }

func validReadBounds(bounds views.Bounds) bool {
	return bounds.MaxItems > 0 && bounds.MaxBytes > 0
}

func normalizeReadError(ctx context.Context, err error) error {
	if ctx != nil && ctx.Err() != nil {
		return ctx.Err()
	}
	if errors.Is(err, context.Canceled) {
		return context.Canceled
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return context.DeadlineExceeded
	}
	switch {
	case errors.Is(err, ErrInvalidInput):
		return readError(views.InvalidQuery)
	case errors.Is(err, ErrCredentialExpired):
		return readError(views.CredentialExpired)
	case errors.Is(err, ErrPermissionDenied):
		return readError(views.PermissionDenied)
	case errors.Is(err, ErrRateLimited):
		return readError(views.RateLimited)
	case errors.Is(err, ErrInvalidResponse):
		return readError(views.InvalidProviderResponse)
	default:
		return readError(views.Unavailable)
	}
}

func failureKind(err error) string {
	switch {
	case errors.Is(err, ErrInvalidInput):
		return "invalid_query"
	case errors.Is(err, ErrCredentialExpired):
		return "credential_expired"
	case errors.Is(err, ErrPermissionDenied):
		return "permission_denied"
	case errors.Is(err, ErrRateLimited):
		return "rate_limited"
	case errors.Is(err, ErrInvalidResponse):
		return "partial_fetch"
	default:
		return "unavailable"
	}
}

func readError(kind views.ReadErrorKind) error { return views.ReadError{Kind: kind} }
