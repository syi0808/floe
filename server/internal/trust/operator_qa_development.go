//go:build floe_dev

package trust

import (
	"context"
	"time"

	"floe/server/internal/operation"
)

// CreateDevelopmentQAOperatorSession creates a normal operator session for the
// explicitly enabled local floe_dev dashboard. Production builds do not include it.
func (s *Service) CreateDevelopmentQAOperatorSession(ctx context.Context) (string, OperatorSession, error) {
	if err := ctx.Err(); err != nil {
		return "", OperatorSession{}, err
	}
	if err := s.RequiredSecurityError(); err != nil {
		return "", OperatorSession{}, err
	}
	s.operators.mu.Lock()
	defer s.operators.mu.Unlock()
	token, result := s.operators.issue(time.Now(), true)
	if result.Code != "" {
		return "", OperatorSession{}, operation.Fail(result.Category, result.Code)
	}
	session, ok := s.operators.active[Digest(token)]
	if !ok {
		return "", OperatorSession{}, operation.Fail(operation.Internal, "operator_unavailable")
	}
	return token, session, nil
}
