package inference

import (
	"context"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"floe/server/internal/codexauth"
	"strings"
)

type AgentInput struct {
	ReplaySource string           `json:"replay_source,omitempty"`
	Messages     []map[string]any `json:"messages"`
	Tools        []map[string]any `json:"tools"`
}

func validAgentInput(raw json.RawMessage) bool {
	var input AgentInput
	if json.Unmarshal(raw, &input) != nil || len(input.Messages) == 0 || len(input.Messages) > 256 || len(input.Tools) > 64 {
		return false
	}
	names := map[string]bool{}
	for _, tool := range input.Tools {
		function, ok := tool["function"].(map[string]any)
		if !ok || tool["type"] != "function" {
			return false
		}
		name, ok := function["name"].(string)
		if !ok || !targetID.MatchString(name) || names[name] {
			return false
		}
		names[name] = true
		parameters, ok := function["parameters"].(map[string]any)
		if !ok || parameters["type"] != "object" {
			return false
		}
	}
	pending := map[string]bool{}
	seen := map[string]bool{}
	for _, message := range input.Messages {
		if items, exists := message["provider_items"]; exists {
			if input.ReplaySource == "" || message["role"] != "assistant" {
				return false
			}
			if _, ok := items.([]any); !ok {
				return false
			}
		}
		role, _ := message["role"].(string)
		if role != "user" && role != "assistant" && role != "tool" {
			return false
		}
		if role == "tool" {
			identifier, _ := message["tool_call_id"].(string)
			if !pending[identifier] {
				return false
			}
			delete(pending, identifier)
			if _, ok := message["content"].(string); !ok {
				return false
			}
			continue
		}
		if len(pending) != 0 {
			return false
		}
		if rawCalls, exists := message["tool_calls"]; exists {
			calls, ok := rawCalls.([]any)
			if !ok || role != "assistant" || (len(calls) == 0 || len(calls) > 8) {
				return false
			}
			for _, value := range calls {
				call, ok := value.(map[string]any)
				if !ok {
					return false
				}
				identifier, _ := call["id"].(string)
				if identifier == "" || len(identifier) > 128 || seen[identifier] || call["type"] != "function" {
					return false
				}
				function, ok := call["function"].(map[string]any)
				if !ok {
					return false
				}
				name, _ := function["name"].(string)
				if !targetID.MatchString(name) {
					return false
				}
				arguments, ok := function["arguments"].(string)
				var object map[string]any
				if !ok || json.Unmarshal([]byte(arguments), &object) != nil || object == nil {
					return false
				}
				pending[identifier], seen[identifier] = true, true
			}
		} else if _, ok := message["content"].(string); !ok {
			return false
		}
	}
	if len(pending) != 0 {
		return false
	}
	return true
}

func (gateway *Gateway) replaySource(configured route, purpose, identity string) string {
	encoded, _ := json.Marshal([]any{configured.provider.target, configured.effort, configured.provider.credential, purpose, identity})
	digest := hmac.New(sha256.New, gateway.tokenHash[:])
	_, _ = digest.Write(encoded)
	return hex.EncodeToString(digest.Sum(nil))
}

type AgentOutput struct {
	Output     []map[string]any `json:"output"`
	UsedTokens uint64           `json:"used_tokens"`
	Replay     any              `json:"replay,omitempty"`
	CallIDs    []string         `json:"call_ids,omitempty"`
}

func normalizeAgentMessage(message map[string]any, usage uint64) (string, error) {
	if message["refusal"] != nil {
		return "", errProvider
	}
	if usage == 0 {
		usage = 4096
	}
	calls, _ := message["tool_calls"].([]any)
	if message["tool_calls"] != nil && calls == nil || len(calls) > 8 {
		return "", errInvalidOutput
	}
	output := AgentOutput{UsedTokens: usage, Replay: message["provider_items"], Output: []map[string]any{}}
	steps := map[string]map[string]any{}
	for _, value := range calls {
		call, ok := value.(map[string]any)
		if !ok {
			return "", errInvalidOutput
		}
		if call["type"] != nil && call["type"] != "function" {
			return "", errInvalidOutput
		}
		function, ok := call["function"].(map[string]any)
		if !ok {
			return "", errInvalidOutput
		}
		name, _ := function["name"].(string)
		identifier, _ := call["id"].(string)
		if identifier == "" {
			var random [16]byte
			if _, err := rand.Read(random[:]); err != nil {
				return "", errProvider
			}
			identifier = hex.EncodeToString(random[:])
		}
		arguments, ok := function["arguments"].(string)
		if !ok {
			encoded, err := json.Marshal(function["arguments"])
			if err != nil {
				return "", errInvalidOutput
			}
			arguments = string(encoded)
		}
		var object map[string]any
		if !targetID.MatchString(name) || len(identifier) > 128 || steps[identifier] != nil ||
			json.Unmarshal([]byte(arguments), &object) != nil || object == nil {
			return "", errInvalidOutput
		}
		steps[identifier] = map[string]any{"kind": "call", "capability_id": name, "input": arguments}
		output.CallIDs = append(output.CallIDs, identifier)
	}
	appendText := func(content string) {
		if strings.TrimSpace(content) != "" {
			output.Output = append(output.Output, map[string]any{"kind": "preamble", "text": content})
		}
	}
	if items, ok := message["provider_items"].([]any); ok {
		orderedIDs := []string{}
		for _, value := range items {
			item, ok := value.(map[string]any)
			if !ok {
				return "", errInvalidOutput
			}
			switch item["type"] {
			case "function_call":
				identifier, _ := item["call_id"].(string)
				step := steps[identifier]
				if step == nil {
					return "", errInvalidOutput
				}
				delete(steps, identifier)
				orderedIDs = append(orderedIDs, identifier)
				output.Output = append(output.Output, step)
			case "message":
				content, ok := item["content"].([]any)
				if !ok || item["role"] != "assistant" {
					return "", errInvalidOutput
				}
				for _, part := range content {
					part, ok := part.(map[string]any)
					if !ok {
						return "", errInvalidOutput
					}
					if part["type"] == "refusal" {
						return "", errProvider
					}
					if part["type"] == "output_text" {
						text, ok := part["text"].(string)
						if !ok {
							return "", errInvalidOutput
						}
						appendText(text)
					}
				}
			case "reasoning":
			default:
				return "", errInvalidOutput
			}
		}
		if len(steps) != 0 {
			return "", errInvalidOutput
		}
		output.CallIDs = orderedIDs
	} else {
		content, _ := message["content"].(string)
		appendText(content)
		for _, identifier := range output.CallIDs {
			output.Output = append(output.Output, steps[identifier])
		}
	}
	if len(output.Output) == 0 || len(output.Output) > 16 {
		return "", errInvalidOutput
	}
	if len(calls) == 0 {
		output.Output[len(output.Output)-1]["kind"] = "answer"
		output.Replay = nil
	}
	encoded, err := json.Marshal(output)
	if err != nil || len(encoded) > 32768 {
		return "", errInvalidOutput
	}
	return string(encoded), nil
}

func classifyCodexError(err error) error {
	switch {
	case errors.Is(err, codexauth.ErrInvalidOutput):
		return errInvalidOutput
	case errors.Is(err, codexauth.ErrCredentialExpired):
		return errCredentialExpired
	case errors.Is(err, codexauth.ErrQuotaExceeded):
		return errQuotaExceeded
	case errors.Is(err, codexauth.ErrRequestRejected):
		return errRequestRejected
	default:
		return err
	}
}

func (adapter *provider) agent(ctx context.Context, request Request, effort string) (string, error) {
	if adapter.target.Provider == "codex_oauth" {
		ctx = codexauth.WithAccountIdentity(ctx, request.ProviderIdentity)
		output, err := adapter.codex.Generate(ctx, adapter.target.Model, effort, request.Instructions, request.Input, nil)
		if err != nil {
			return "", classifyCodexError(err)
		}
		var message map[string]any
		if json.Unmarshal([]byte(output), &message) != nil {
			return "", errInvalidOutput
		}
		usage := uint64(4096)
		if value, ok := message["used_tokens"].(float64); ok && value > 0 {
			usage = uint64(value)
		}
		return normalizeAgentMessage(message, usage)
	}
	var input AgentInput
	if json.Unmarshal(request.Input, &input) != nil {
		return "", errInvalidOutput
	}
	messages := append([]map[string]any{{"role": "system", "content": request.Instructions}}, input.Messages...)
	payload := map[string]any{"model": adapter.target.Model, "messages": messages, "tools": input.Tools, "stream": false}
	if adapter.target.Provider == "ollama" {
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
		for _, message := range messages {
			if calls, ok := message["tool_calls"].([]any); ok {
				for _, value := range calls {
					call := value.(map[string]any)
					function := call["function"].(map[string]any)
					if arguments, ok := function["arguments"].(string); ok {
						var parsed any
						if json.Unmarshal([]byte(arguments), &parsed) != nil {
							return "", errInvalidOutput
						}
						function["arguments"] = parsed
					}
				}
			}
		}
		var response struct {
			Done        bool           `json:"done"`
			Message     map[string]any `json:"message"`
			PromptCount uint64         `json:"prompt_eval_count"`
			Count       uint64         `json:"eval_count"`
		}
		if err := adapter.post(ctx, "/api/chat", payload, &response); err != nil {
			return "", err
		}
		if !response.Done {
			return "", errInvalidOutput
		}
		return normalizeAgentMessage(response.Message, response.PromptCount+response.Count)
	}
	payload["parallel_tool_calls"] = true
	if effort != "" {
		payload["reasoning_effort"] = effort
	}
	var response struct {
		Choices []struct {
			Finish  string         `json:"finish_reason"`
			Message map[string]any `json:"message"`
		} `json:"choices"`
		Usage struct {
			Total uint64 `json:"total_tokens"`
		} `json:"usage"`
	}
	if err := adapter.post(ctx, "/chat/completions", payload, &response); err != nil {
		return "", err
	}
	if len(response.Choices) != 1 || (response.Choices[0].Finish != "stop" && response.Choices[0].Finish != "tool_calls") {
		return "", errInvalidOutput
	}
	return normalizeAgentMessage(response.Choices[0].Message, response.Usage.Total)
}
