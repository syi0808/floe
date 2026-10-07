//go:build !floe_dev

package httptransport

import (
	"context"
	"errors"

	"floe/server/internal/trust"
)

func createDevelopmentQASession(context.Context, *trust.Service) (string, trust.OperatorSession, error) {
	return "", trust.OperatorSession{}, errors.New("development QA mode requires a floe_dev build")
}
