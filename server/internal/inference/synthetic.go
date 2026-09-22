package inference

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/json"
	"floe/server/internal/operation"
	"net/http"
	"net/http/httptest"
	"strings"
	"time"
)

func TestTarget(ctx context.Context, target Target, allowExternal bool, lookup func(string) string, runtime CodexClient) (outcome operation.Result) {
	token := rand.Text() + rand.Text()
	gateway, err := New(Config{Targets: map[string]Target{"probe": target}, Routes: map[string]Route{"fast": {Target: "probe"}}}, token, lookup, runtime)
	if err != nil {
		outcome = operation.Reject(operation.Unavailable, "model_unavailable")
		return
	}
	request := Request{SchemaVersion: 1, Purpose: "quick_response", DataClasses: []string{"synthetic"}, AllowExternal: allowExternal, Instructions: "Reply briefly to confirm the Agent route is working. This is a synthetic connectivity test without personal data.", Input: json.RawMessage(`{"messages":[{"role":"user","content":"Confirm the Agent route."}],"tools":[]}`)}
	_, request.ExpectedRecipient = gateway.routes["fast"].provider.disclosure()
	payload, _ := json.Marshal(request)
	ctx, cancel := context.WithTimeout(ctx, 40*time.Second)
	defer cancel()
	forward, _ := http.NewRequestWithContext(ctx, "POST", "/v1/agent", bytes.NewReader(payload))
	forward.Header.Set("Authorization", "Bearer "+token)
	response := httptest.NewRecorder()
	started := time.Now()
	gateway.ServeHTTP(response, forward)
	if response.Code != 200 {
		var body struct {
			Error struct {
				Code string `json:"code"`
			} `json:"error"`
		}
		_ = json.Unmarshal(response.Body.Bytes(), &body)
		outcome = inferenceFailure(response.Code, body.Error.Code)
		return
	}
	var envelope struct {
		Output string `json:"output"`
	}
	var result struct {
		Output []struct {
			Kind string `json:"kind"`
			Text string `json:"text"`
		} `json:"output"`
	}
	if json.Unmarshal(response.Body.Bytes(), &envelope) != nil ||
		json.Unmarshal([]byte(envelope.Output), &result) != nil ||
		len(result.Output) != 1 || result.Output[0].Kind != "answer" || strings.TrimSpace(result.Output[0].Text) == "" {
		outcome = operation.Reject(operation.Upstream, "invalid_proposal")
		return
	}
	outcome = operation.Result{Category: operation.Ready, Value: map[string]any{"ok": true, "elapsed_ms": time.Since(started).Milliseconds()}}
	return
}
func inferenceFailure(status int, code string) operation.Result {
	categories := map[int]operation.Category{400: operation.Invalid, 401: operation.Unauthenticated, 403: operation.Denied, 404: operation.Missing, 409: operation.Conflict, 429: operation.Limited, 500: operation.Internal, 502: operation.Upstream, 503: operation.Unavailable}
	category, found := categories[status]
	if !found {
		category = operation.Upstream
	}
	return operation.Reject(category, code)
}
