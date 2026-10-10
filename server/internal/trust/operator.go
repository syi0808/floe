package trust

import (
	"context"
	"crypto/rand"
	"crypto/subtle"
	"sync"
	"time"

	"floe/server/internal/operation"
)

type operatorSessions struct {
	mu            sync.Mutex
	adminHash     string
	active        map[string]OperatorSession
	loginAttempts int
	loginWindow   time.Time
}

type OperatorSession struct {
	CSRF    string
	Expires time.Time
}

func newOperatorSessions(adminHash string) *operatorSessions {
	return &operatorSessions{adminHash: adminHash, active: map[string]OperatorSession{}}
}

func (sessions *operatorSessions) Lookup(token string) (OperatorSession, bool) {
	sessions.mu.Lock()
	defer sessions.mu.Unlock()
	record, exists := sessions.active[Digest(token)]
	return record, exists && record.Expires.After(time.Now())
}

func (sessions *operatorSessions) Delete(token string) {
	sessions.mu.Lock()
	defer sessions.mu.Unlock()
	delete(sessions.active, Digest(token))
}

func (sessions *operatorSessions) Login(credential string) (string, error) {
	sessions.mu.Lock()
	defer sessions.mu.Unlock()
	now := time.Now()
	if now.Sub(sessions.loginWindow) > time.Minute {
		sessions.loginWindow, sessions.loginAttempts = now, 0
	}
	sessions.loginAttempts++
	if sessions.loginAttempts > 10 {
		return "", operation.Fail(operation.Limited, "try_later")
	}
	if subtle.ConstantTimeCompare([]byte(Digest(credential)), []byte(sessions.adminHash)) != 1 {
		return "", operation.Fail(operation.Unauthenticated, "unauthorized")
	}
	for key, value := range sessions.active {
		if !value.Expires.After(now) {
			delete(sessions.active, key)
		}
	}
	if len(sessions.active) >= 8 {
		return "", operation.Fail(operation.Limited, "too_many_sessions")
	}
	token := rand.Text() + rand.Text()
	sessions.active[Digest(token)] = OperatorSession{CSRF: rand.Text() + rand.Text(), Expires: now.Add(12 * time.Hour)}
	return token, nil
}

// OperatorPrincipal cannot be used as an app principal or acquire source authority.
type OperatorPrincipal struct {
	owner     *Service
	sessionID string
	expires   time.Time
}

func (s *Service) LoginOperator(credential string) (string, error) {
	return s.operators.Login(credential)
}
func (s *Service) LogoutOperator(cookie string) { s.operators.Delete(cookie) }
func (s *Service) OperatorSession(cookie string) (OperatorSession, bool) {
	return s.operators.Lookup(cookie)
}
func (s *Service) AuthenticateOperatorSession(ctx context.Context, cookie, csrf string, mutation bool) (OperatorPrincipal, error) {
	if err := ctx.Err(); err != nil {
		return OperatorPrincipal{}, err
	}
	if s.RequiredSecurityError() != nil {
		return OperatorPrincipal{}, fail(operation.Unavailable, "trust_unavailable")
	}
	session, ok := s.operators.Lookup(cookie)
	if !ok || mutation && subtle.ConstantTimeCompare([]byte(csrf), []byte(session.CSRF)) != 1 {
		return OperatorPrincipal{}, fail(operation.Unauthenticated, "unauthorized")
	}
	return OperatorPrincipal{s, Digest(cookie), session.Expires}, nil
}
func (s *Service) WithCurrentOperator(p OperatorPrincipal, consume func() error) error {
	if s.RequiredSecurityError() != nil {
		return fail(operation.Unavailable, "trust_unavailable")
	}
	s.operators.mu.Lock()
	defer s.operators.mu.Unlock()
	r, ok := s.operators.active[p.sessionID]
	if p.owner != s || !ok || r.Expires != p.expires || !r.Expires.After(time.Now()) || consume == nil {
		return fail(operation.Unauthenticated, "unauthorized")
	}
	return consume()
}
