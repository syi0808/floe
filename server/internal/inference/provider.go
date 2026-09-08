package inference

import (
	"bytes"
	"context"
	"crypto/tls"
	"encoding/json"
	"errors"
	"io"
	"net"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"
)

var errInvalidOutput = errors.New("invalid model output")
var errProvider = errors.New("model unavailable")
var environmentName = regexp.MustCompile(`^[A-Z_][A-Z0-9_]*$`)

type provider struct {
	target     Target
	credential string
	external   bool
	client     *http.Client
	codex      CodexClient
}

func (adapter *provider) disclosure() (string, string) {
	if !adapter.external {
		return "server_local", ""
	}
	if adapter.target.Provider == "codex_oauth" {
		return "external", "OpenAI (Codex OAuth)"
	}
	endpoint, _ := url.Parse(adapter.target.BaseURL)
	return "external", endpoint.Hostname()
}

func newProvider(target Target, lookup func(string) string, codex CodexClient) (*provider, error) {
	if target.Provider == "codex_oauth" {
		if target.BaseURL != "https://chatgpt.com/backend-api/codex" || target.APIKeyEnv != "" || codex == nil ||
			strings.TrimSpace(target.Model) == "" || len(target.Model) > 128 || strings.ContainsAny(target.Model, "\r\n") {
			return nil, errors.New("invalid Codex OAuth target")
		}
		return &provider{target: target, external: true, codex: codex}, nil
	}
	endpoint, err := url.Parse(target.BaseURL)
	if err != nil {
		return nil, errors.New("invalid provider endpoint")
	}
	ip := net.ParseIP(endpoint.Hostname())
	loopback := ip != nil && ip.IsLoopback()
	if endpoint.Hostname() == "" || endpoint.User != nil || endpoint.RawQuery != "" || endpoint.Fragment != "" ||
		(endpoint.Scheme != "https" && !(endpoint.Scheme == "http" && loopback)) ||
		strings.TrimSpace(target.Model) == "" || len(target.Model) > 128 || strings.ContainsAny(target.Model, "\r\n") {
		return nil, errors.New("invalid provider endpoint or model")
	}
	if target.Provider != "ollama" && target.Provider != "openai_compatible" {
		return nil, errors.New("unsupported provider")
	}
	if target.Provider == "ollama" && (!loopback || strings.Contains(strings.ToLower(target.Model), "cloud")) {
		return nil, errors.New("Ollama requires a local model on a loopback address")
	}
	credential := ""
	if target.APIKeyEnv != "" {
		if !environmentName.MatchString(target.APIKeyEnv) {
			return nil, errors.New("invalid credential reference")
		}
		credential = lookup(target.APIKeyEnv)
		if credential == "" || len(credential) > 8192 || strings.ContainsAny(credential, "\r\n") {
			return nil, errors.New("configured provider credential is unavailable")
		}
	}
	target.BaseURL = strings.TrimRight(target.BaseURL, "/")
	transport := &http.Transport{
		Proxy:               nil,
		DialContext:         (&net.Dialer{Timeout: 5 * time.Second, KeepAlive: 30 * time.Second}).DialContext,
		TLSClientConfig:     &tls.Config{MinVersion: tls.VersionTLS12},
		TLSHandshakeTimeout: 5 * time.Second,
		MaxIdleConns:        8,
		IdleConnTimeout:     30 * time.Second,
	}
	return &provider{target: target, credential: credential, external: target.Provider != "ollama", client: &http.Client{
		Transport:     transport,
		CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
	}}, nil
}

func (adapter *provider) generate(ctx context.Context, request Request, reasoningEffort string) (string, error) {
	if adapter.target.Provider == "codex_oauth" {
		output, err := adapter.codex.Generate(ctx, adapter.target.Model, reasoningEffort, request.Instructions, request.Input, request.OutputSchema)
		if err != nil {
			return "", errProvider
		}
		if !validOutput(output) {
			return "", errInvalidOutput
		}
		return output, nil
	}
	if adapter.target.Provider == "ollama" {
		return adapter.ollama(ctx, request)
	}
	return adapter.openAI(ctx, request, reasoningEffort)
}

func (adapter *provider) post(ctx context.Context, path string, payload any, output any) error {
	encoded, err := json.Marshal(payload)
	if err != nil {
		return errInvalidOutput
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, adapter.target.BaseURL+path, bytes.NewReader(encoded))
	if err != nil {
		return errProvider
	}
	request.Header.Set("Content-Type", "application/json")
	if adapter.credential != "" {
		request.Header.Set("Authorization", "Bearer "+adapter.credential)
	}
	response, err := adapter.client.Do(request)
	if err != nil {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		return errProvider
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return errProvider
	}
	body, err := io.ReadAll(io.LimitReader(response.Body, 1048577))
	if err != nil {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		return errProvider
	}
	if len(body) > 1048576 || json.Unmarshal(body, output) != nil {
		return errInvalidOutput
	}
	return nil
}

func messages(request Request) []map[string]string {
	return []map[string]string{{"role": "system", "content": request.Instructions}, {"role": "user", "content": string(request.Input)}}
}

func (adapter *provider) ollama(ctx context.Context, request Request) (string, error) {
	var info struct {
		RemoteModel string `json:"remote_model"`
		RemoteHost  string `json:"remote_host"`
	}
	if err := adapter.post(ctx, "/api/show", map[string]string{"model": adapter.target.Model}, &info); err != nil {
		return "", err
	}
	if info.RemoteModel != "" || info.RemoteHost != "" {
		return "", errProvider
	}
	var response struct {
		Done    bool `json:"done"`
		Message struct {
			Content string            `json:"content"`
			Tools   []json.RawMessage `json:"tool_calls"`
		} `json:"message"`
	}
	err := adapter.post(ctx, "/api/chat", map[string]any{
		"model": adapter.target.Model, "messages": messages(request), "stream": false,
		"format": request.OutputSchema, "options": map[string]any{"temperature": 0, "num_predict": 512},
	}, &response)
	if err != nil {
		return "", err
	}
	if !response.Done || len(response.Message.Tools) != 0 || !validOutput(response.Message.Content) {
		return "", errInvalidOutput
	}
	return response.Message.Content, nil
}

func (adapter *provider) openAI(ctx context.Context, request Request, reasoningEffort string) (string, error) {
	var response struct {
		Choices []struct {
			FinishReason string `json:"finish_reason"`
			Message      struct {
				Content  string            `json:"content"`
				Tools    []json.RawMessage `json:"tool_calls"`
				Function json.RawMessage   `json:"function_call"`
				Refusal  json.RawMessage   `json:"refusal"`
			} `json:"message"`
		} `json:"choices"`
	}
	payload := map[string]any{
		"model": adapter.target.Model, "messages": messages(request), "stream": false,
		"response_format": map[string]any{"type": "json_schema", "json_schema": map[string]any{
			"name": "floe_result", "strict": true, "schema": request.OutputSchema,
		}},
	}
	if reasoningEffort != "" {
		payload["reasoning_effort"] = reasoningEffort
	}
	err := adapter.post(ctx, "/chat/completions", payload, &response)
	if err != nil {
		return "", err
	}
	if len(response.Choices) != 1 || response.Choices[0].FinishReason != "stop" {
		return "", errInvalidOutput
	}
	message := response.Choices[0].Message
	if len(message.Tools) != 0 || nonNull(message.Function) || nonNull(message.Refusal) || !validOutput(message.Content) {
		return "", errInvalidOutput
	}
	return message.Content, nil
}

func nonNull(value json.RawMessage) bool { return len(value) > 0 && string(value) != "null" }

func validOutput(output string) bool {
	return len(output) > 0 && len(output) <= 8192 && json.Valid([]byte(output))
}
