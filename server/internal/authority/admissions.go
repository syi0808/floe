package authority

import (
	"sync"
)

// Admissions owns bounded recorded source admissions.
//
// Persistence and the real external authority are separate from any local app
// state: an admission recorded here is this server's own record, not a mirror
// of the client's view.
type Admissions struct {
	mu         sync.Mutex
	remoteView map[string]remoteViewAdmissionState
}

func NewAdmissions() *Admissions {
	return &Admissions{
		remoteView: map[string]remoteViewAdmissionState{},
	}
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
