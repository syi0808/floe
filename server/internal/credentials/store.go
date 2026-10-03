package credentials

// Store is the private credential boundary. Slot derivation remains in this package.
type Store interface {
	Get(string) (string, error)
	Put(string, string) error
	Delete(string) error
}
