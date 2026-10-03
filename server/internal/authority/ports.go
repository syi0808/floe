package authority

import (
	"context"
	"floe/server/internal/trust"
	"floe/server/internal/views"
)

// SourceResolver and SourceFence are the inward ports for normalized Readers.
type SourceResolver interface {
	ResolveSource(context.Context, trust.Principal, views.SourceTarget) (ResolvedSource, error)
	PreflightSource(context.Context, trust.Principal, views.SourceSnapshot) error
}
type SourceFence interface {
	WithCurrentSource(trust.Principal, views.SourceSnapshot, func(views.SourceSnapshot) error) error
}
type ResolvedSource struct {
	Snapshot views.SourceSnapshot
	Reader   views.Reader
	Limits   views.Bounds
}
