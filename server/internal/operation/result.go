package operation

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

type Result struct {
	Category Category
	Value    any
	Code     string
}

func Accept(value any) Result {
	return Result{Category: Ready, Value: value}
}

func Reject(category Category, code string) Result {
	return Result{Category: category, Code: code}
}

// Error exposes only a stable category and code; provider and storage details stay private.
type Error struct {
	Category Category
	Code     string
}

func (e Error) Error() string                   { return e.Code }
func Fail(category Category, code string) error { return Error{Category: category, Code: code} }
