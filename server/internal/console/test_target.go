package console

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"time"

	"floe/server/internal/inference"
)

func (console *Console) testTarget(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		ID            string `json:"id"`
		AllowExternal bool   `json:"allow_external"`
	}
	if !decode(writer, request, &input) {
		failure(writer, 400, "validation")
		return
	}
	console.mu.Lock()
	if console.testActive {
		console.mu.Unlock()
		failure(writer, 429, "model_busy")
		return
	}
	console.testActive = true
	gateway := console.gateway
	console.mu.Unlock()
	defer func() { console.mu.Lock(); console.testActive = false; console.mu.Unlock() }()
	payload, _ := json.Marshal(inference.Request{SchemaVersion: 1, Target: input.ID, AllowExternal: input.AllowExternal, Instructions: `Return only {"ok":true}. This is a synthetic connectivity test without personal data.`, Input: json.RawMessage(`{"test":true}`), OutputSchema: json.RawMessage(`{"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false}`)})
	ctx, cancel := context.WithTimeout(request.Context(), 40*time.Second)
	defer cancel()
	forward, _ := http.NewRequestWithContext(ctx, "POST", "/v1/generate", bytes.NewReader(payload))
	forward.Header.Set("Authorization", "Bearer "+console.internalToken)
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
		failure(writer, response.Code, body.Error.Code)
		return
	}
	var envelope struct {
		Output string `json:"output"`
	}
	var result map[string]any
	if json.Unmarshal(response.Body.Bytes(), &envelope) != nil || json.Unmarshal([]byte(envelope.Output), &result) != nil || len(result) != 1 || result["ok"] != true {
		failure(writer, 502, "invalid_proposal")
		return
	}
	reply(writer, 200, map[string]any{"ok": true, "elapsed_ms": time.Since(started).Milliseconds()})
}
