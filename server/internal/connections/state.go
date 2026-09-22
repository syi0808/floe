package connections

import "sync"

// Registry owns connection intent for this server: in-flight authorization
// attempts, the per-connector lifecycle lock and reserved connection records.
//
// Transport never mutates these directly, and no other owner shares this mutex.
type Registry struct {
	mu           sync.Mutex
	attempts     map[string]*Attempt
	reservations map[string]Record
	unavailable  map[string]bool

	lifecycleMu sync.Mutex
	lifecycles  map[string]*sync.Mutex
}

func NewRegistry() *Registry {
	return &Registry{
		attempts:     map[string]*Attempt{},
		reservations: map[string]Record{},
		unavailable:  map[string]bool{},
		lifecycles:   map[string]*sync.Mutex{},
	}
}

// Lifecycle returns the lock serializing changes for one connector.
//
// The lock is per connector, so two connectors never block each other and two
// owners never share one global connector mutex.
func (registry *Registry) Lifecycle(connectorID string) *sync.Mutex {
	registry.lifecycleMu.Lock()
	defer registry.lifecycleMu.Unlock()
	lock, ok := registry.lifecycles[connectorID]
	if !ok {
		lock = &sync.Mutex{}
		registry.lifecycles[connectorID] = lock
	}
	return lock
}

// Attempt returns the in-flight attempt for a key.
func (registry *Registry) GetAttempt(key string) *Attempt {
	registry.mu.Lock()
	defer registry.mu.Unlock()
	return cloneAttempt(registry.attempts[key])
}

func cloneAttempt(attempt *Attempt) *Attempt {
	if attempt == nil {
		return nil
	}
	snapshot := *attempt
	snapshot.Scope = CloneConnectorScope(attempt.Scope)
	return &snapshot
}

func (registry *Registry) Attempts() map[string]*Attempt {
	registry.mu.Lock()
	defer registry.mu.Unlock()
	result := make(map[string]*Attempt, len(registry.attempts))
	for key, attempt := range registry.attempts {
		result[key] = cloneAttempt(attempt)
	}
	return result
}

func (registry *Registry) Reservations() map[string]Record {
	registry.mu.Lock()
	defer registry.mu.Unlock()
	result := make(map[string]Record, len(registry.reservations))
	for key, record := range registry.reservations {
		result[key] = record
	}
	return result
}

// PutAttempt records or replaces an in-flight attempt.
func (registry *Registry) PutAttempt(key string, attempt *Attempt) {
	registry.mu.Lock()
	defer registry.mu.Unlock()
	registry.attempts[key] = cloneAttempt(attempt)
}

// DeleteAttempt ends an attempt so a late observation cannot settle it.
func (registry *Registry) DeleteAttempt(key string) {
	registry.mu.Lock()
	defer registry.mu.Unlock()
	delete(registry.attempts, key)
}

// Reservation returns the reserved connection record for a key.
func (registry *Registry) Reservation(key string) (Record, bool) {
	registry.mu.Lock()
	defer registry.mu.Unlock()
	record, ok := registry.reservations[key]
	return record, ok
}

// Reserve records a reserved connection.
func (registry *Registry) Reserve(key string, record Record) {
	registry.mu.Lock()
	defer registry.mu.Unlock()
	registry.reservations[key] = record
}

// ReleaseReservation drops a reservation that was not completed.
func (registry *Registry) ReleaseReservation(key string) {
	registry.mu.Lock()
	defer registry.mu.Unlock()
	delete(registry.reservations, key)
}

// MarkUnavailable records that a connector cannot currently be used.
func (registry *Registry) MarkUnavailable(connectorID string, unavailable bool) {
	registry.mu.Lock()
	defer registry.mu.Unlock()
	registry.unavailable[connectorID] = unavailable
}

// Unavailable reports whether a connector is currently unusable.
func (registry *Registry) Unavailable(connectorID string) bool {
	registry.mu.Lock()
	defer registry.mu.Unlock()
	return registry.unavailable[connectorID]
}
