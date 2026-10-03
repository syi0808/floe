package googlecalendar

import (
	"context"
	"errors"
	"floe/server/internal/integrations"
	"floe/server/internal/views"
	"strings"
	"sync"
	"time"
)

const maxCalendarRangeMS = int64(32 * 24 * time.Hour / time.Millisecond)

type Service struct {
	pager     *views.CalendarPager
	clock     func() time.Time
	operation chan struct{}

	statusMu            sync.RWMutex
	observedAtUnixMS    int64
	lastSuccessAtUnixMS *int64
	lastFailure         *integrations.Failure
	lastView            *views.ViewSnapshot
}

func NewService(clients ...*Client) (*Service, error) {
	if len(clients) == 0 {
		return nil, ErrInvalidInput
	}
	leaves := make([]views.CalendarLeaf, len(clients))
	for index, client := range clients {
		if client == nil || client.connectionID != clients[0].connectionID {
			return nil, ErrInvalidInput
		}
		leaves[index] = views.CalendarLeaf{ResourceID: client.calendarID, Read: client.Calendar}
	}
	pager, err := views.NewCalendarPager(clients[0].connectionID, leaves)
	if err != nil {
		return nil, ErrInvalidInput
	}
	return &Service{pager: pager, clock: time.Now, operation: make(chan struct{}, 1)}, nil
}

func (service *Service) Read(ctx context.Context, request views.ReadRequest) (views.Result, error) {
	if ctx == nil {
		return views.Result{}, readError(views.InvalidQuery)
	}
	if err := ctx.Err(); err != nil {
		return views.Result{}, err
	}
	if request.Query.ViewID != views.Calendar || request.Query.Calendar == nil || request.Query.Mail != nil || request.Query.Work != nil || request.Query.Logistics != nil || !validReadBounds(request.Bounds) {
		return views.Result{}, readError(views.InvalidQuery)
	}
	query := *request.Query.Calendar
	if !validCalendarQuery(query) {
		return views.Result{}, readError(views.InvalidQuery)
	}
	itemLimit := min(int(request.Bounds.MaxItems), maxItems)
	byteLimit := min(int(request.Bounds.MaxBytes), 65_536)
	limit := min(query.Limit, itemLimit)
	if err := service.acquire(ctx); err != nil {
		return service.readFailure(ctx, err)
	}
	defer service.release()
	if err := ctx.Err(); err != nil {
		return service.readFailure(ctx, err)
	}

	start := time.UnixMilli(query.RangeStartUnixMS)
	end := time.UnixMilli(query.RangeEndUnixMS)
	now := service.clock()
	view, err := service.pager.Read(ctx, start, end, query.Cursor, limit, now)
	if err != nil {
		return service.readFailure(ctx, err)
	}
	if err := ctx.Err(); err != nil {
		return service.readFailure(ctx, err)
	}
	result := views.Result{ViewID: views.Calendar, Calendar: &view}
	encoded, _, err := views.EncodeBounded(result, views.Bounds{MaxItems: uint32(itemLimit), MaxBytes: uint32(byteLimit)})
	if err != nil {
		return service.readFailure(ctx, views.ErrInvalid)
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
		ConnectorID:      "calendar.google",
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

func validCalendarQuery(query views.CalendarQuery) bool {
	return query.RangeStartUnixMS >= 0 && query.RangeEndUnixMS > query.RangeStartUnixMS &&
		query.RangeEndUnixMS-query.RangeStartUnixMS <= maxCalendarRangeMS &&
		len(query.Cursor) <= 2048 && !strings.ContainsAny(query.Cursor, "\r\n\x00") &&
		query.Limit >= 1 && query.Limit <= maxItems
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
	case errors.Is(err, ErrInvalidInput), errors.Is(err, views.ErrCalendarCursor):
		return readError(views.InvalidQuery)
	case errors.Is(err, ErrCredentialExpired):
		return readError(views.CredentialExpired)
	case errors.Is(err, ErrPermissionDenied):
		return readError(views.PermissionDenied)
	case errors.Is(err, ErrRateLimited):
		return readError(views.RateLimited)
	case errors.Is(err, ErrInvalidResponse), errors.Is(err, views.ErrCalendarResult), errors.Is(err, views.ErrInvalid):
		return readError(views.InvalidProviderResponse)
	default:
		return readError(views.Unavailable)
	}
}

func failureKind(err error) string {
	switch {
	case errors.Is(err, ErrInvalidInput), errors.Is(err, views.ErrCalendarCursor):
		return "invalid_query"
	case errors.Is(err, ErrCredentialExpired):
		return "credential_expired"
	case errors.Is(err, ErrPermissionDenied):
		return "permission_denied"
	case errors.Is(err, ErrRateLimited):
		return "rate_limited"
	case errors.Is(err, ErrInvalidResponse), errors.Is(err, views.ErrCalendarResult), errors.Is(err, views.ErrInvalid):
		return "partial_fetch"
	default:
		return "unavailable"
	}
}

func readError(kind views.ReadErrorKind) error { return views.ReadError{Kind: kind} }
