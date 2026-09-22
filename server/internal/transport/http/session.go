package httptransport

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"sync"
	"time"

	"floe/server/internal/operation"
)

type Sessions struct {
	mu            sync.Mutex
	adminHash     string
	active        map[string]sessionRecord
	loginAttempts int
	loginWindow   time.Time
}

type sessionRecord struct {
	CSRF    string
	Expires time.Time
}

func NewSessions(adminHash string) *Sessions {
	return &Sessions{adminHash: adminHash, active: map[string]sessionRecord{}}
}

func (sessions *Sessions) Lookup(token string) (sessionRecord, bool) {
	sessions.mu.Lock()
	defer sessions.mu.Unlock()
	record, exists := sessions.active[digest(token)]
	return record, exists && record.Expires.After(time.Now())
}

func (sessions *Sessions) Delete(token string) {
	sessions.mu.Lock()
	defer sessions.mu.Unlock()
	delete(sessions.active, digest(token))
}

func (sessions *Sessions) Login(credential string) (string, operation.Result) {
	sessions.mu.Lock()
	defer sessions.mu.Unlock()
	now := time.Now()
	if now.Sub(sessions.loginWindow) > time.Minute {
		sessions.loginWindow, sessions.loginAttempts = now, 0
	}
	sessions.loginAttempts++
	if sessions.loginAttempts > 10 {
		return "", operation.Reject(operation.Limited, "try_later")
	}
	if digest(credential) != sessions.adminHash {
		return "", operation.Reject(operation.Unauthenticated, "unauthorized")
	}
	for key, value := range sessions.active {
		if !value.Expires.After(now) {
			delete(sessions.active, key)
		}
	}
	if len(sessions.active) >= 8 {
		return "", operation.Reject(operation.Limited, "too_many_sessions")
	}
	token := rand.Text() + rand.Text()
	sessions.active[digest(token)] = sessionRecord{CSRF: rand.Text() + rand.Text(), Expires: now.Add(12 * time.Hour)}
	return token, operation.Accept(map[string]bool{"ok": true})
}

func digest(value string) string {
	hash := sha256.Sum256([]byte(value))
	return hex.EncodeToString(hash[:])
}
