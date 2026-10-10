package operation

import "errors"

type Category string

const (
	Ready           Category = "ready"
	Created         Category = "created"
	Invalid         Category = "invalid"
	Unauthenticated Category = "unauthenticated"
	Denied          Category = "denied"
	Missing         Category = "missing"
	Conflict        Category = "conflict"
	Limited         Category = "limited"
	Unavailable     Category = "unavailable"
	Upstream        Category = "upstream"
	Internal        Category = "internal"
)

// Error exposes only a stable category and code; provider and storage details stay private.
type Error struct {
	Category Category
	Code     string
}

func (e Error) Error() string                   { return e.Code }
func Fail(category Category, code string) error { return Error{Category: category, Code: code} }

// Normalize preserves an existing operation error and maps an unclassified
// failure to the caller's safe, stable fallback.
func Normalize(err error, category Category, code string) error {
	if err == nil {
		return nil
	}
	var failure Error
	if errors.As(err, &failure) {
		return failure
	}
	return Error{Category: category, Code: code}
}

func ErrorOf(err error) (Error, bool) {
	var failure Error
	if errors.As(err, &failure) {
		return failure, true
	}
	return Error{}, false
}
