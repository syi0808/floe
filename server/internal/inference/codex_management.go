package inference

import (
	"context"
	"floe/server/internal/operation"
	"floe/server/internal/trust"
	"time"
)

type AccountCommand string

const (
	AccountStatus AccountCommand = "status"
	AccountLogin  AccountCommand = "login"
	AccountCancel AccountCommand = "cancel"
	AccountLogout AccountCommand = "logout"
)

type AccountProgress struct {
	Status           string
	AuthorizationURL string
	InferenceEnabled bool
}
type AccountAuthorization interface {
	Authorize(context.Context, AccountCommand) (AccountProgress, error)
}
type AccountManagement struct {
	trust   Trust
	runtime AccountAuthorization
}

func NewAccountManagement(t Trust, runtime AccountAuthorization) *AccountManagement {
	return &AccountManagement{t, runtime}
}
func (m *AccountManagement) Execute(ctx context.Context, p trust.OperatorPrincipal, command AccountCommand) (AccountProgress, error) {
	if command != AccountStatus && command != AccountLogin && command != AccountCancel && command != AccountLogout {
		return AccountProgress{}, operation.Fail(operation.Missing, "not_found")
	}
	if err := m.trust.WithCurrentOperator(p, func() error { return nil }); err != nil {
		return AccountProgress{}, operation.Fail(operation.Unauthenticated, "unauthorized")
	}
	ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	progress, err := m.runtime.Authorize(ctx, command)
	if err != nil {
		return AccountProgress{}, operation.Fail(operation.Unavailable, "codex_unavailable")
	}
	if err = m.trust.WithCurrentOperator(p, func() error { return ctx.Err() }); err != nil {
		return AccountProgress{}, operation.Fail(operation.Unauthenticated, "unauthorized")
	}
	return progress, nil
}
