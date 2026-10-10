package gmail

import (
	"context"
	"errors"
	"floe/server/internal/adapters/storage/privatefiles"
	"floe/server/internal/integrations"
	"floe/server/internal/views"
	"strings"
	"sync"
	"time"
)

type AuthRuntime interface {
	TokenSource
	Ready() bool
}

type Service struct {
	operation chan struct{}
	statusMu  sync.RWMutex
	auth      AuthRuntime
	index     *Index
	syncer    *Syncer
	clock     func() time.Time

	observedAtUnixMS    int64
	lastSuccessAtUnixMS *int64
	lastFailure         *integrations.Failure
	readViews           map[string]views.ViewSnapshot
}

func NewService(files *storage.Files, connectionID, query string, auth AuthRuntime) (*Service, error) {
	if auth == nil {
		return nil, ErrInvalidInput
	}
	client, err := New(auth)
	if err != nil {
		return nil, err
	}
	return newService(files, connectionID, query, auth, client)
}

func newService(files *storage.Files, connectionID, query string, auth AuthRuntime, client *Client) (*Service, error) {
	if auth == nil || client == nil {
		return nil, ErrInvalidInput
	}
	index, err := OpenIndex(files, connectionID)
	if err != nil {
		return nil, err
	}
	syncer, err := NewSyncer(client, index, query)
	if err != nil {
		return nil, err
	}
	return &Service{operation: make(chan struct{}, 1), auth: auth, index: index, syncer: syncer, clock: time.Now, readViews: map[string]views.ViewSnapshot{}}, nil
}

func (service *Service) Read(ctx context.Context, request views.ReadRequest) (views.Result, error) {
	if ctx == nil {
		return views.Result{}, readError(views.InvalidQuery)
	}
	if err := ctx.Err(); err != nil {
		return views.Result{}, err
	}
	if !validReadBounds(request.Bounds) {
		return views.Result{}, readError(views.InvalidQuery)
	}

	var communication views.MailQuery
	var readID string
	itemLimit := 0
	byteLimit := min(int(request.Bounds.MaxBytes), 65_536)
	switch request.Query.ViewID {
	case views.Communication:
		if request.Query.Mail == nil || request.Query.Calendar != nil || request.Query.Work != nil || request.Query.Logistics != nil || !validMailQuery(*request.Query.Mail) {
			return views.Result{}, readError(views.InvalidQuery)
		}
		communication = *request.Query.Mail
		readID = string(views.Communication)
		itemLimit = min(int(request.Bounds.MaxItems), MaxPageItems)
	case views.Logistics:
		if request.Query.Logistics == nil || request.Query.Logistics.SchemaVersion != 1 || request.Query.Calendar != nil || request.Query.Mail != nil || request.Query.Work != nil {
			return views.Result{}, readError(views.InvalidQuery)
		}
		readID = string(views.Logistics)
		itemLimit = min(int(request.Bounds.MaxItems), maxLogisticsItems)
	default:
		return views.Result{}, readError(views.InvalidQuery)
	}

	if err := service.acquire(ctx); err != nil {
		service.recordFailure("unavailable", service.clock().UnixMilli())
		return views.Result{}, err
	}
	defer service.release()
	if err := ctx.Err(); err != nil {
		return service.readFailure(ctx, err)
	}
	if !service.auth.Ready() {
		return service.readFailure(ctx, ErrCredentialExpired)
	}
	if err := service.syncer.Refresh(ctx); err != nil {
		return service.readFailure(ctx, err)
	}
	if err := ctx.Err(); err != nil {
		return service.readFailure(ctx, err)
	}

	now := service.clock()
	var result views.Result
	switch request.Query.ViewID {
	case views.Communication:
		limit := min(communication.Limit, itemLimit)
		view, err := service.index.Communication(communication.Query, communication.Cursor, limit, now)
		if err != nil {
			return service.readFailure(ctx, err)
		}
		result = views.Result{ViewID: views.Communication, Communication: &view}
	case views.Logistics:
		view, err := service.index.Logistics(now)
		if err != nil {
			return service.readFailure(ctx, err)
		}
		if len(view.Items) > itemLimit {
			view.Items = view.Items[:itemLimit]
			view.CoverageComplete = false
		}
		result = views.Result{ViewID: views.Logistics, Logistics: &view}
	}
	if err := ctx.Err(); err != nil {
		return service.readFailure(ctx, err)
	}
	encoded, _, err := views.EncodeBounded(result, views.Bounds{MaxItems: uint32(itemLimit), MaxBytes: uint32(byteLimit)})
	if err != nil {
		return service.readFailure(ctx, ErrInvalidResponse)
	}
	if err := ctx.Err(); err != nil {
		return service.readFailure(ctx, err)
	}
	if err := service.index.RecordSync(now, ""); err != nil {
		if ctxErr := ctx.Err(); ctxErr != nil {
			service.recordFailure("unavailable", service.clock().UnixMilli())
			return views.Result{}, ctxErr
		}
		service.recordFailure("unavailable", service.clock().UnixMilli())
		return views.Result{}, readError(views.Unavailable)
	}
	if err := ctx.Err(); err != nil {
		return service.readFailure(ctx, err)
	}
	metadata := viewSnapshot(result, len(encoded))
	observed := metadata.ObservedAtUnixMS
	if err := ctx.Err(); err != nil {
		return service.readFailure(ctx, err)
	}
	if err := service.recordSuccess(ctx, readID, metadata, observed); err != nil {
		return service.readFailure(ctx, err)
	}
	return result, nil
}

func (service *Service) Snapshot(ctx context.Context) (integrations.Snapshot, error) {
	if err := service.index.checkAvailable(); err != nil {
		return integrations.Snapshot{}, err
	}
	if ctx == nil {
		return integrations.Snapshot{}, ErrInvalidInput
	}
	if err := ctx.Err(); err != nil {
		return integrations.Snapshot{}, err
	}
	now := service.clock()
	nowUnixMS := now.UnixMilli()
	service.statusMu.RLock()
	observed := service.observedAtUnixMS
	var success *time.Time
	if service.lastSuccessAtUnixMS != nil {
		value := time.UnixMilli(*service.lastSuccessAtUnixMS)
		success = &value
	}
	var failure *integrations.Failure
	if service.lastFailure != nil {
		value := *service.lastFailure
		failure = &value
	}
	readViews := make([]views.ViewSnapshot, 0, 2)
	stale := false
	for _, id := range []string{string(views.Communication), string(views.Logistics)} {
		if view, ok := service.readViews[id]; ok {
			if view.ExpiresAtUnixMS > nowUnixMS {
				readViews = append(readViews, view)
			} else {
				stale = true
			}
		}
	}
	service.statusMu.RUnlock()
	if err := ctx.Err(); err != nil {
		return integrations.Snapshot{}, err
	}
	if stale && failure == nil {
		failure = &integrations.Failure{Kind: "stale", ObservedAtUnixMS: nowUnixMS}
	}
	if observed == 0 {
		observed = nowUnixMS
	}

	state, failureKind := "pending", ""
	if failure != nil {
		failureKind = failure.Kind
		if success != nil {
			state = "degraded"
		} else if failure.Kind == "credential_expired" {
			state = "revoked"
		} else {
			state = "unavailable"
		}
	} else if success != nil {
		state = "ready"
	}
	snapshot, err := snapshotForConnection(service.index.connectionID, state, time.UnixMilli(observed), success, failureKind)
	if err != nil {
		return integrations.Snapshot{}, err
	}
	if err := ctx.Err(); err != nil {
		return integrations.Snapshot{}, err
	}
	snapshot.Views = readViews
	return snapshot, nil
}

func (service *Service) Cleanup(ctx context.Context) error {
	if ctx == nil {
		return ErrInvalidInput
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := service.acquire(ctx); err != nil {
		return err
	}
	defer service.release()
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := service.index.Reset(); err != nil {
		return err
	}
	service.clearReadStatus()
	return nil
}

func (service *Service) readFailure(ctx context.Context, err error) (views.Result, error) {
	if ctx != nil && ctx.Err() != nil {
		err = ctx.Err()
	}
	normalized := normalizeReadError(ctx, err)
	kind := failureFor(err)
	observed := service.clock().UnixMilli()
	_ = service.index.RecordSync(time.UnixMilli(observed), kind)
	service.recordFailure(kind, observed)
	return views.Result{}, normalized
}

func (service *Service) recordSuccess(ctx context.Context, id string, view views.ViewSnapshot, observed int64) error {
	service.statusMu.Lock()
	defer service.statusMu.Unlock()
	if err := ctx.Err(); err != nil {
		return err
	}
	service.observedAtUnixMS = observed
	service.lastSuccessAtUnixMS = &observed
	service.lastFailure = nil
	service.readViews[id] = view
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

func (service *Service) clearReadStatus() {
	service.statusMu.Lock()
	defer service.statusMu.Unlock()
	service.observedAtUnixMS = 0
	service.lastSuccessAtUnixMS = nil
	service.lastFailure = nil
	service.readViews = map[string]views.ViewSnapshot{}
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

func validMailQuery(query views.MailQuery) bool {
	return len(query.Query) <= 512 && !strings.ContainsAny(query.Query, "\r\n\x00") && query.Cursor >= 0 && query.Cursor <= 10000 && query.Limit >= 1 && query.Limit <= MaxPageItems
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
	case errors.Is(err, ErrRateLimited):
		return readError(views.RateLimited)
	case errors.Is(err, ErrBodyApproval):
		return readError(views.PermissionDenied)
	case errors.Is(err, ErrInvalidResponse):
		return readError(views.InvalidProviderResponse)
	default:
		return readError(views.Unavailable)
	}
}

func viewSnapshot(result views.Result, byteCount int) views.ViewSnapshot {
	switch result.ViewID {
	case views.Communication:
		view := result.Communication
		return views.ViewSnapshot{SchemaVersion: 1, ViewID: view.ViewID, SourceHandle: view.SourceHandle, ObservedAtUnixMS: view.ObservedAtUnixMS, ExpiresAtUnixMS: view.ExpiresAtUnixMS, ItemCount: len(view.Items), ByteCount: byteCount, ProvenanceCount: len(view.Items)}
	case views.Logistics:
		view := result.Logistics
		return views.ViewSnapshot{SchemaVersion: 1, ViewID: view.ViewID, SourceHandle: view.SourceHandle, ObservedAtUnixMS: view.ObservedAtUnixMS, ExpiresAtUnixMS: view.ExpiresAtUnixMS, ItemCount: len(view.Items), ByteCount: byteCount, ProvenanceCount: len(view.Items)}
	default:
		return views.ViewSnapshot{}
	}
}

func readError(kind views.ReadErrorKind) error { return views.ReadError{Kind: kind} }

func failureFor(err error) string {
	switch {
	case errors.Is(err, context.Canceled), errors.Is(err, context.DeadlineExceeded), errors.Is(err, ErrUnavailable):
		return "unavailable"
	case errors.Is(err, ErrInvalidInput):
		return "invalid_query"
	case errors.Is(err, ErrCredentialExpired):
		return "credential_expired"
	case errors.Is(err, ErrRateLimited):
		return "rate_limited"
	case errors.Is(err, ErrCheckpointExpired):
		return "stale"
	case errors.Is(err, ErrInvalidResponse):
		return "partial_fetch"
	default:
		return "unavailable"
	}
}
