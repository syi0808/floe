package application

import (
	"context"

	"floe/server/internal/inference"
	"floe/server/internal/operation"
	httptransport "floe/server/internal/transport/http"
)

func (console *Console) testTarget(ctx context.Context, input httptransport.TestRequest) (outcome operation.Result) {

	console.mu.Lock()
	if console.testActive {
		console.mu.Unlock()
		outcome = operation.Reject(operation.Limited, "model_busy")
		return
	}
	console.testActive = true
	target, exists := console.configuredTarget(input.ID)
	lookup := console.lookup
	runtime := console.runtime
	console.mu.Unlock()
	defer func() { console.mu.Lock(); console.testActive = false; console.mu.Unlock() }()
	if !exists {
		outcome = operation.Reject(operation.Missing, "unknown_target")
		return
	}
	return inference.TestTarget(ctx, target, input.AllowExternal, lookup, runtime)
}
