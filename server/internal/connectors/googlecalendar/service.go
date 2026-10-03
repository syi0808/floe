package googlecalendar

import (
 "floe/server/internal/integrations"
 "floe/server/internal/views"
	"context"
	"errors"
	"sync"
	"time"

	
)

type Service struct {
	pager     *views.CalendarPager
	clock     func() time.Time
	operation sync.Mutex
	last      *views.CalendarView
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
	return &Service{pager: pager, clock: time.Now}, nil
}

func (service *Service) ReadCalendarView(ctx context.Context, rangeStart, rangeEnd time.Time, cursor string, limit int) (any, error) {
	service.operation.Lock()
	defer service.operation.Unlock()
	view, err := service.pager.Read(ctx, rangeStart, rangeEnd, cursor, limit, service.clock())
	if errors.Is(err, views.ErrCalendarCursor) {
		return nil, ErrInvalidInput
	}
	if errors.Is(err, views.ErrCalendarResult) {
		return nil, ErrInvalidResponse
	}
	if err == nil {
		service.last = &view
	}
	return view, err
}

func (service *Service) ConnectionSnapshot(ctx context.Context) (any, error) {
	service.operation.Lock()
	defer service.operation.Unlock()
	now := service.clock()
	view, err := service.pager.Read(ctx, now.Add(-24*time.Hour), now.Add(31*24*time.Hour), "", 25, now)
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
	case errors.Is(err, ErrInvalidResponse), errors.Is(err, views.ErrCalendarResult):
		kind = "partial_fetch"
	}
	observed := now.UnixMilli()
	snapshot := integrations.Snapshot{Descriptor: ConnectorDescriptor(), Connection: integrations.Connection{SchemaVersion: 1, ConnectorID: "calendar.google", State: state, ObservedAtUnixMS: observed, LastFailure: &integrations.Failure{Kind: kind, ObservedAtUnixMS: observed}}, Views: []views.ViewSnapshot{}}
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
