package credentials

import "context"

// Store is the private credential boundary. A missing value is returned only
// after a successful exact-slot read; cancellation and locked stores are errors.
type Store interface {
    Get(context.Context, string) (string, error)
    Put(context.Context, string, string) error
    Delete(context.Context, string) error
}
