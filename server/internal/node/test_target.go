package node

import (
	"context"
	"floe/server/internal/operation"
	httptransport "floe/server/internal/transport/http"
	"floe/server/internal/trust"
)

func (c *Console) testTarget(ctx context.Context, operator trust.OperatorPrincipal, in httptransport.TestRequest) operation.Result {
	result, err := c.gateway.ProbeTarget(ctx, operator, in.ID)
	if err != nil {
		return operation.Reject(operation.Upstream, "model_unavailable")
	}
	return operation.Accept(map[string]any{"ok": true, "elapsed_ms": result.ElapsedMS, "trace_id": result.TraceID})
}
