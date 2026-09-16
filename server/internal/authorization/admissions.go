package authorization

import (
	"sync"
	"sync/atomic"
)

// Admissions owns recorded source admissions and the producer identity this
// server presents.
//
// Persistence and the real external authority are separate from any local app
// state: an admission recorded here is this server's own record, not a mirror
// of the client's view.
type Admissions struct {
	mu         sync.Mutex
	calendar   map[string]calendarAdmissionState
	remoteView map[string]remoteViewAdmissionState

	producer            *producerIdentity
	trustUnavailable    atomic.Bool
	producerUnavailable atomic.Bool
}

func NewAdmissions() *Admissions {
	return &Admissions{
		calendar:   map[string]calendarAdmissionState{},
		remoteView: map[string]remoteViewAdmissionState{},
	}
}

// CalendarAdmission returns the recorded calendar admission for a key.
func (admissions *Admissions) CalendarAdmission(key string) (calendarAdmissionState, bool) {
	admissions.mu.Lock()
	defer admissions.mu.Unlock()
	state, ok := admissions.calendar[key]
	return state, ok
}

// RecordCalendarAdmission stores a calendar admission.
func (admissions *Admissions) RecordCalendarAdmission(key string, state calendarAdmissionState) {
	admissions.mu.Lock()
	defer admissions.mu.Unlock()
	admissions.calendar[key] = state
}

// RemoteViewAdmission returns the recorded view admission for a key.
func (admissions *Admissions) RemoteViewAdmission(key string) (remoteViewAdmissionState, bool) {
	admissions.mu.Lock()
	defer admissions.mu.Unlock()
	state, ok := admissions.remoteView[key]
	return state, ok
}

// RecordRemoteViewAdmission stores a view admission.
func (admissions *Admissions) RecordRemoteViewAdmission(key string, state remoteViewAdmissionState) {
	admissions.mu.Lock()
	defer admissions.mu.Unlock()
	admissions.remoteView[key] = state
}

// LatchTrustUnavailable marks the trust store unusable. It never clears itself;
// an unverifiable trust store must not silently become trusted again.
func (admissions *Admissions) LatchTrustUnavailable() { admissions.trustUnavailable.Store(true) }

// TrustUnavailable reports whether the trust store is unusable.
func (admissions *Admissions) TrustUnavailable() bool { return admissions.trustUnavailable.Load() }

// LatchProducerUnavailable marks the producer identity unusable.
func (admissions *Admissions) LatchProducerUnavailable() {
	admissions.producerUnavailable.Store(true)
}

// ProducerUnavailable reports whether the producer identity is unusable.
func (admissions *Admissions) ProducerUnavailable() bool {
	return admissions.producerUnavailable.Load()
}

// Producer returns the identity this server presents to clients.
func (admissions *Admissions) Producer() *producerIdentity { return admissions.producer }

// SetProducer records the identity this server presents.
func (admissions *Admissions) SetProducer(identity *producerIdentity) {
	admissions.producer = identity
}
