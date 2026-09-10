package gmail

import (
	"context"
	"encoding/json"
	"errors"
	"sync"
	"time"
)

type AuthRuntime interface {
	TokenSource
	Ready() bool
	Action(context.Context, string) (any, error)
}

type Service struct {
	operation sync.Mutex
	auth      AuthRuntime
	index     *Index
	syncer    *Syncer
	clock     func() time.Time
}

func NewService(directory, connectionID, query string, auth AuthRuntime) (*Service, error) {
	if auth == nil {
		return nil, ErrInvalidInput
	}
	client, err := New(auth)
	if err != nil {
		return nil, err
	}
	return newService(directory, connectionID, query, auth, client)
}

func newService(directory, connectionID, query string, auth AuthRuntime, client *Client) (*Service, error) {
	if auth == nil || client == nil {
		return nil, ErrInvalidInput
	}
	index, err := OpenIndex(directory, connectionID)
	if err != nil {
		return nil, err
	}
	syncer, err := NewSyncer(client, index, query)
	if err != nil {
		return nil, err
	}
	return &Service{auth: auth, index: index, syncer: syncer, clock: time.Now}, nil
}

func (service *Service) Action(ctx context.Context, action string) (any, error) {
	if action == "status" {
		value, err := service.auth.Action(ctx, action)
		if err != nil {
			return nil, err
		}
		status, ok := value.(map[string]any)
		if !ok {
			return nil, ErrInvalidResponse
		}
		copy := make(map[string]any, len(status)+1)
		for key, item := range status {
			copy[key] = item
		}
		snapshot, err := service.Snapshot()
		if err != nil {
			return nil, err
		}
		copy["connection"] = snapshot
		return copy, nil
	}
	if !service.operation.TryLock() {
		return nil, ErrUnavailable
	}
	defer service.operation.Unlock()
	switch action {
	case "login", "cancel":
		return service.auth.Action(ctx, action)
	case "logout":
		value, err := service.auth.Action(ctx, action)
		if err != nil {
			return nil, err
		}
		if err := service.index.Reset(); err != nil {
			return nil, err
		}
		return value, nil
	case "sync":
		if !service.auth.Ready() {
			return nil, ErrCredentialExpired
		}
		err := service.syncer.Refresh(ctx)
		now := service.clock()
		if err != nil {
			_ = service.index.RecordSync(now, failureFor(err))
			return nil, err
		}
		if err := service.index.RecordSync(now, ""); err != nil {
			return nil, err
		}
		return service.Snapshot()
	default:
		return nil, ErrInvalidInput
	}
}

func (service *Service) Run(ctx context.Context, interval time.Duration) error {
	if interval < time.Minute || interval > 24*time.Hour {
		return ErrInvalidInput
	}
	ticker := time.NewTicker(interval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
			if service.auth.Ready() {
				_, _ = service.Action(ctx, "sync")
			}
		}
	}
}

func (service *Service) Snapshot() (Snapshot, error) {
	now := service.clock()
	success, failure, count := service.index.SyncStatus()
	state, failureKind := "pending", ""
	if !service.auth.Ready() {
		if failure != nil && failure.Kind == "credential_expired" {
			state, failureKind = "revoked", failure.Kind
		} else {
			state = "disconnected"
		}
	} else if failure != nil && success != nil {
		state, failureKind = "degraded", failure.Kind
	} else if failure != nil {
		state, failureKind = "unavailable", failure.Kind
	} else if success != nil {
		state = "ready"
	}
	var successTime *time.Time
	if success != nil {
		value := time.UnixMilli(*success)
		successTime = &value
	}
	snapshot, err := ConnectionSnapshot(service.index.connectionID, state, now, successTime, failureKind)
	if err != nil {
		return Snapshot{}, err
	}
	if (state == "ready" || state == "degraded") && count > 0 {
		view, err := service.index.Communication("", 0, min(count, MaxPageItems), now)
		if err != nil {
			return Snapshot{}, err
		}
		encoded, _ := json.Marshal(view)
		snapshot.Views = []ViewSnapshot{{SchemaVersion: 1, ViewID: "mail.communication", SourceHandle: view.SourceHandle, ObservedAtUnixMS: view.ObservedAtUnixMS, ExpiresAtUnixMS: view.ExpiresAtUnixMS, ItemCount: len(view.Items), ByteCount: len(encoded), ProvenanceCount: len(view.Items)}}
	}
	return snapshot, nil
}

func (service *Service) ConnectionSnapshot() (any, error) {
	return service.Snapshot()
}

func failureFor(err error) string {
	switch {
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
