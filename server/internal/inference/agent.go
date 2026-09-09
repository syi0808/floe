package inference

import (
	"context"
	"encoding/json"
	"errors"
	"floe/server/internal/codexauth"
	"strings"
)

type AgentInput struct {
	Messages []map[string]any `json:"messages"`
	Tools    []map[string]any `json:"tools"`
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
			if !ok || role != "assistant" || len(calls) != 1 {
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

type AgentOutput struct {
	Step       map[string]any `json:"step"`
	UsedTokens uint64         `json:"used_tokens"`
	Replay     any            `json:"replay,omitempty"`
	CallID     string         `json:"call_id,omitempty"`
}

func normalizeAgentMessage(message map[string]any, usage uint64) (string, error) {
	if message["refusal"] != nil {
		return "", errProvider
	}
	if usage == 0 {
		usage = 4096
	}
	calls, _ := message["tool_calls"].([]any)
	if value := message["tool_calls"]; value != nil && calls == nil {
		return "", errInvalidOutput
	}
	output := AgentOutput{UsedTokens: usage, Replay: message["provider_items"]}
	if len(calls) > 1 {
		return "", errInvalidOutput
	}
	if len(calls) == 1 {
		call, ok := calls[0].(map[string]any)
		if !ok {
			return "", errInvalidOutput
		}
		function, ok := call["function"].(map[string]any)
		if !ok {
			return "", errInvalidOutput
		}
		name, _ := function["name"].(string)
		output.CallID, _ = call["id"].(string)
		arguments, ok := function["arguments"].(string)
		if !ok {
			encoded, err := json.Marshal(function["arguments"])
			if err != nil {
				return "", errInvalidOutput
			}
			arguments = string(encoded)
		}
		var object map[string]any
		if name == "" || json.Unmarshal([]byte(arguments), &object) != nil || object == nil {
			return "", errInvalidOutput
		}
		output.Step = map[string]any{"kind": "call", "capability_id": name, "input": arguments}
	} else {
		content, _ := message["content"].(string)
		if strings.TrimSpace(content) == "" || message["refusal"] != nil {
			return "", errInvalidOutput
		}
		output.Step = map[string]any{"kind": "answer", "text": content}
	}
	encoded, err := json.Marshal(output)
	if err != nil || len(encoded) > 32768 {
		return "", errInvalidOutput
	}
	return string(encoded), nil
}

func (adapter *provider) agent(ctx context.Context, request Request, effort string) (string, error) {
	if adapter.target.Provider == "codex_oauth" {
		output, err := adapter.codex.Generate(ctx, adapter.target.Model, effort, request.Instructions, request.Input, nil)
		if err != nil {
			if errors.Is(err, codexauth.ErrInvalidOutput) {
				return "", errInvalidOutput
			}
			return "", err
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
	payload["parallel_tool_calls"] = false
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
