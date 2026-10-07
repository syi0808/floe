//go:build floe_dev

package httptransport

import (
	"context"

	"floe/server/internal/trust"
)

func createDevelopmentQASession(ctx context.Context, owner *trust.Service) (string, trust.OperatorSession, error) {
	return owner.CreateDevelopmentQAOperatorSession(ctx)
}
