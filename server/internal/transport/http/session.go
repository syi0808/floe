// HTTP and admin-UI transport for the local server.

package httptransport

import (
	"sync"
	"time"
)

// Sessions owns admin-UI session and login-attempt state.
//
// This is transport state: it never decides a connection, pairing or authority
// outcome, and no business owner shares this mutex.
type Sessions struct {
	mu            sync.Mutex
	adminHash     string
	internalToken string
	active        map[string]sessionRecord
	loginAttempts int
	loginWindow   time.Time
}

type sessionRecord struct {
	Expires time.Time
	Scope   clientScope
}

func NewSessions(adminHash, internalToken string) *Sessions {
	return &Sessions{
		adminHash:     adminHash,
		internalToken: internalToken,
		active:        map[string]sessionRecord{},
	}
}

// Lookup returns the session for a token.
func (sessions *Sessions) Lookup(token string) (sessionRecord, bool) {
	sessions.mu.Lock()
	defer sessions.mu.Unlock()
	record, ok := sessions.active[token]
	return record, ok
}

// Put records a session.
func (sessions *Sessions) Put(token string, record sessionRecord) {
	sessions.mu.Lock()
	defer sessions.mu.Unlock()
	sessions.active[token] = record
}

// Delete ends a session.
func (sessions *Sessions) Delete(token string) {
	sessions.mu.Lock()
	defer sessions.mu.Unlock()
	delete(sessions.active, token)
}

// RecordLoginAttempt counts a failed login inside the current window and
// reports the running count.
func (sessions *Sessions) RecordLoginAttempt(now time.Time, window time.Duration) int {
	sessions.mu.Lock()
	defer sessions.mu.Unlock()
	if now.Sub(sessions.loginWindow) > window {
		sessions.loginWindow = now
		sessions.loginAttempts = 0
	}
	sessions.loginAttempts++
	return sessions.loginAttempts
}

// ResetLoginAttempts clears the failure count after a successful login.
func (sessions *Sessions) ResetLoginAttempts() {
	sessions.mu.Lock()
	defer sessions.mu.Unlock()
	sessions.loginAttempts = 0
}
