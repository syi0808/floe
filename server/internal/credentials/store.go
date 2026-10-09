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

// Creator is the create-only capability used when an owner issues an
// immutable credential slot. It must fail if the slot already exists and
// must not report settled absence until any write attempt has completed.
type Creator interface {
	Create(context.Context, string, string) error
}
