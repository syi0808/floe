package providers

import (
	"bytes"
	"context"
	"crypto/tls"
	"encoding/json"
	"errors"
	codexauth "floe/server/internal/adapters/models/codex"
	"floe/server/internal/inference"
	"floe/server/internal/trust"
	"io"
	"net"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"
)

var environmentName = regexp.MustCompile(`^[A-Z_][A-Z0-9_]*$`)

type provider struct {
	target          inference.ProviderTarget
	credential      string
	credentialError error
	lookup          func(context.Context, string) (string, error)
	client          *http.Client
	codex           CodexClient
}

func newProvider(ctx context.Context, target inference.ProviderTarget, lookup func(context.Context, string) (string, error), codex CodexClient) (*provider, error) {
	target.BudgetOverride = inference.CloneModelBudgetOverride(target.BudgetOverride)
	if strings.TrimSpace(target.Model) == "" || len(target.Model) > 128 || strings.ContainsAny(target.Model, "\r\n") {
		return nil, errors.New("invalid model")
	}
	if target.Provider == "codex_oauth" {
		if target.BaseURL != "https://chatgpt.com/backend-api/codex" || target.APIKeyEnv != "" {
			return nil, errors.New("invalid Codex target")
		}
		return &provider{target: target, codex: codex}, nil
	}
	endpoint, err := url.Parse(target.BaseURL)
	if err != nil {
		return nil, errors.New("invalid provider endpoint")
	}
	ip := net.ParseIP(endpoint.Hostname())
	loopback := ip != nil && ip.IsLoopback()
	if endpoint.Hostname() == "" || endpoint.User != nil || endpoint.RawQuery != "" || endpoint.ForceQuery || strings.Contains(target.BaseURL, "#") || endpoint.Fragment != "" || endpoint.Scheme != "https" && !(endpoint.Scheme == "http" && loopback) {
		return nil, errors.New("invalid provider endpoint")
	}
	if target.Provider != "ollama" && target.Provider != "openai_compatible" {
		return nil, errors.New("unsupported provider")
	}
	if target.Provider == "ollama" && (!loopback || strings.Contains(strings.ToLower(target.Model), "cloud")) {
		return nil, errors.New("Ollama requires a local model")
	}
	p := &provider{target: target, lookup: lookup}
	if target.APIKeyEnv != "" {
		if !environmentName.MatchString(target.APIKeyEnv) {
			return nil, errors.New("invalid credential reference")
		}
		p.credential, p.credentialError = lookup(ctx, target.APIKeyEnv)
		if errors.Is(p.credentialError, context.DeadlineExceeded) || errors.Is(p.credentialError, context.Canceled) {
			return nil, p.credentialError
		}
		if p.credential == "" || len(p.credential) > 8192 || strings.ContainsAny(p.credential, "\r\n") {
			p.credentialError = errors.New("credential unavailable")
		}
	}
	p.target.BaseURL = strings.TrimRight(target.BaseURL, "/")
	p.client = &http.Client{Transport: &http.Transport{Proxy: nil, DialContext: (&net.Dialer{Timeout: 5 * time.Second, KeepAlive: 30 * time.Second}).DialContext, TLSClientConfig: &tls.Config{MinVersion: tls.VersionTLS12}, TLSHandshakeTimeout: 5 * time.Second, MaxIdleConns: 8, IdleConnTimeout: 30 * time.Second}, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	return p, nil
}
func (p *provider) post(ctx context.Context, path string, payload, output any) error {
	data, err := p.postJSON(ctx, path, payload)
	if err != nil {
		return err
	}
	if json.Unmarshal(data, output) != nil {
		return inference.Failure{Code: inference.InvalidOutput}
	}
	return nil
}
func (p *provider) postJSON(ctx context.Context, path string, payload any) ([]byte, error) {
	encoded, err := json.Marshal(payload)
	if err != nil {
		return nil, inference.Failure{Code: inference.RequestRejected}
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, p.target.BaseURL+path, bytes.NewReader(encoded))
	if err != nil {
		return nil, inference.Failure{Code: inference.RequestRejected}
	}
	request.Header.Set("Content-Type", "application/json")
	if p.credential != "" {
		request.Header.Set("Authorization", "Bearer "+p.credential)
	}
	response, err := p.client.Do(request)
	if err != nil {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		return nil, inference.Failure{Code: inference.ModelUnavailable}
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		code := inference.ModelUnavailable
		switch response.StatusCode {
		case 401, 403:
			code = inference.ProviderCredentialsUnavailable
		case 429:
			code = inference.QuotaExceeded
		case 400, 404, 422:
			code = inference.RequestRejected
		}
		return nil, inference.Failure{Code: code}
	}
	data, err := io.ReadAll(io.LimitReader(response.Body, 1048577))
	if err != nil {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		return nil, inference.Failure{Code: inference.ModelUnavailable}
	}
	if trust.StrictJSON(data, 1048576, 32) != nil || !inference.ValidJSONTextEncoding(data) {
		return nil, inference.Failure{Code: inference.InvalidOutput}
	}
	return data, nil
}
func (p *provider) checkLocal(ctx context.Context) error {
	if p.target.Provider != "ollama" {
		return nil
	}
	var info struct {
		RemoteModel string `json:"remote_model"`
		RemoteHost  string `json:"remote_host"`
	}
	if err := p.post(ctx, "/api/show", map[string]string{"model": p.target.Model}, &info); err != nil {
		return err
	}
	if info.RemoteModel != "" || info.RemoteHost != "" {
		return inference.Failure{Code: inference.ModelUnavailable}
	}
	return nil
}
func (p *provider) structured(ctx context.Context, states inference.CapabilityStates, in inference.StructuredInvocation, effort string, outputTokenLimit *uint32) (out inference.StructuredResult, err error) {
	if !inference.SupportsStructuredOutput(states) {
		return out, inference.Failure{Code: inference.RequestRejected}
	}
	if p.target.Provider == "codex_oauth" {
		if outputTokenLimit != nil {
			return out, inference.Failure{Code: inference.Validation}
		}
		identity := p.codex.ReplayIdentity()
		if identity == "" {
			return out, inference.Failure{Code: inference.ProviderCredentialsUnavailable}
		}
		ctx = codexauth.WithAccountIdentity(ctx, identity)
		text, e := p.codex.Generate(ctx, p.target.Model, effort, in.Instructions, in.Input, in.OutputSchema)
		if e != nil {
			return out, classifyCodexError(e)
		}
		out.Output = json.RawMessage(text)
	} else {
		if err = p.checkLocal(ctx); err != nil {
			return out, err
		}
		messages := []map[string]string{{"role": "system", "content": in.Instructions}, {"role": "user", "content": string(in.Input)}}
		if p.target.Provider == "ollama" {
			var response struct {
				Done    bool `json:"done"`
				Message struct {
					Content string            `json:"content"`
					Tools   []json.RawMessage `json:"tool_calls"`
				} `json:"message"`
				Prompt *uint64 `json:"prompt_eval_count"`
				Output *uint64 `json:"eval_count"`
			}
			payload := map[string]any{"model": p.target.Model, "messages": messages, "stream": false, "format": in.OutputSchema}
			if outputTokenLimit != nil {
				payload["options"] = map[string]any{"num_predict": *outputTokenLimit}
			}
			err = p.post(ctx, "/api/chat", payload, &response)
			out.Usage.Tokens = sumUsage(response.Prompt, response.Output)
			if err != nil {
				return out, err
			}
			if !response.Done || len(response.Message.Tools) != 0 {
				return out, inference.Failure{Code: inference.InvalidOutput}
			}
			out.Output = json.RawMessage(response.Message.Content)
		} else {
			var response struct {
				Choices []struct {
					Finish  string `json:"finish_reason"`
					Message struct {
						Content  string            `json:"content"`
						Tools    []json.RawMessage `json:"tool_calls"`
						Function json.RawMessage   `json:"function_call"`
						Refusal  json.RawMessage   `json:"refusal"`
					} `json:"message"`
				} `json:"choices"`
				Usage struct {
					Total *uint64 `json:"total_tokens"`
				} `json:"usage"`
			}
			payload := map[string]any{"model": p.target.Model, "messages": messages, "stream": false, "response_format": map[string]any{"type": "json_schema", "json_schema": map[string]any{"name": "floe_result", "strict": true, "schema": in.OutputSchema}}}
			if outputTokenLimit != nil {
				payload["max_completion_tokens"] = *outputTokenLimit
			}
			if effort != "" {
				payload["reasoning_effort"] = effort
			}
			err = p.post(ctx, "/chat/completions", payload, &response)
			out.Usage.Tokens = response.Usage.Total
			if err != nil {
				return out, err
			}
			if len(response.Choices) != 1 || response.Choices[0].Finish != "stop" {
				return out, inference.Failure{Code: inference.InvalidOutput}
			}
			m := response.Choices[0].Message
			if len(m.Tools) != 0 || nonNull(m.Function) || nonNull(m.Refusal) {
				return out, inference.Failure{Code: inference.InvalidOutput}
			}
			out.Output = json.RawMessage(m.Content)
		}
	}
	if err = inference.ValidateStructuredOutput(in, out.Output); err != nil {
		return out, err
	}
	return out, nil
}
func nonNull(v json.RawMessage) bool { return len(v) > 0 && string(v) != "null" }
func sumUsage(a, b *uint64) *uint64 {
	if a == nil || b == nil || *a > trust.MaxJSONInteger || *b > trust.MaxJSONInteger-*a {
		return nil
	}
	sum := *a + *b
	return &sum
}
func classifyCodexError(err error) error {
	switch {
	case errors.Is(err, codexauth.ErrInvalidOutput):
		return inference.Failure{Code: inference.InvalidOutput}
	case errors.Is(err, codexauth.ErrCredentialExpired):
		return inference.Failure{Code: inference.ProviderCredentialsUnavailable}
	case errors.Is(err, codexauth.ErrQuotaExceeded):
		return inference.Failure{Code: inference.QuotaExceeded}
	case errors.Is(err, codexauth.ErrRequestRejected):
		return inference.Failure{Code: inference.RequestRejected}
	default:
		return err
	}
}
