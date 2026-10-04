package credentials

import (
	"context"
	"errors"
)

var ErrUnavailable = errors.New("credential store unavailable")
var ErrLocked = errors.New("credential store locked")
var ErrBusy = errors.New("credential store busy")

// Store is the private credential boundary. A missing value is returned only
// after a successful exact-slot read; cancellation and locked stores are errors.
type Store interface {
	Get(context.Context, string) (string, error)
	Put(context.Context, string, string) error
	Delete(context.Context, string) error
}
