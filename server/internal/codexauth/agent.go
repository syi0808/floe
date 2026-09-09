package codexauth

import (
	"bufio"
	"encoding/json"
	"io"
	"strings"
)

func nativeInput(raw json.RawMessage) ([]any, []any, error) {
	var input struct {
		Messages []map[string]any `json:"messages"`
		Tools    []map[string]any `json:"tools"`
	}
	if json.Unmarshal(raw, &input) != nil {
		return nil, nil, invalidOutput
	}
	items := []any{}
	tools := []any{}
	for _, tool := range input.Tools {
		function, ok := tool["function"].(map[string]any)
		if !ok {
			return nil, nil, invalidOutput
		}
		tools = append(tools, map[string]any{"type": "function", "name": function["name"], "description": function["description"], "parameters": function["parameters"], "strict": false})
	}
	for _, message := range input.Messages {
		if replay, ok := message["provider_items"].([]any); ok {
			if message["role"] != "assistant" {
				return nil, nil, invalidOutput
			}
			for _, value := range replay {
				item, ok := value.(map[string]any)
				if !ok {
					return nil, nil, invalidOutput
				}
				kind, _ := item["type"].(string)
				if kind != "reasoning" && kind != "function_call" && !(kind == "message" && item["role"] == "assistant") {
					return nil, nil, invalidOutput
				}
				items = append(items, item)
			}
			continue
		}
		if message["role"] == "tool" {
			items = append(items, map[string]any{"type": "function_call_output", "call_id": message["tool_call_id"], "output": message["content"]})
			continue
		}
		if content, ok := message["content"].(string); ok && content != "" {
			items = append(items, map[string]any{"role": message["role"], "content": content})
		}
		if calls, ok := message["tool_calls"].([]any); ok {
			for _, value := range calls {
				call, ok := value.(map[string]any)
				if !ok {
					return nil, nil, invalidOutput
				}
				function, ok := call["function"].(map[string]any)
				if !ok {
					return nil, nil, invalidOutput
				}
				items = append(items, map[string]any{"type": "function_call", "call_id": call["id"], "name": function["name"], "arguments": function["arguments"]})
			}
		}
	}
	return items, tools, nil
}

func readNativeResponse(reader io.Reader) (string, error) {
	scanner := bufio.NewScanner(io.LimitReader(reader, 1048577))
	scanner.Buffer(make([]byte, 4096), 1048576)
	for scanner.Scan() {
		if !strings.HasPrefix(scanner.Text(), "data:") {
			continue
		}
		data := strings.TrimSpace(strings.TrimPrefix(scanner.Text(), "data:"))
		if data == "" || data == "[DONE]" {
			continue
		}
		var event struct {
			Type     string `json:"type"`
			Response struct {
				Status string `json:"status"`
				Output []struct {
					Type      string `json:"type"`
					Name      string `json:"name"`
					CallID    string `json:"call_id"`
					Arguments string `json:"arguments"`
					Content   []struct {
						Type string `json:"type"`
						Text string `json:"text"`
					} `json:"content"`
				} `json:"output"`
			} `json:"response"`
		}
		if json.Unmarshal([]byte(data), &event) != nil {
			return "", invalidOutput
		}
		switch event.Type {
		case "response.failed", "response.incomplete", "error":
			return "", unavailable
		case "response.completed":
			if event.Response.Status != "completed" {
				return "", invalidOutput
			}
			text := ""
			calls := []any{}
			for _, item := range event.Response.Output {
				switch item.Type {
				case "function_call":
					calls = append(calls, map[string]any{"id": item.CallID, "type": "function", "function": map[string]string{"name": item.Name, "arguments": item.Arguments}})
				case "message":
					for _, part := range item.Content {
						if part.Type == "refusal" {
							return "", unavailable
						}
						if part.Type == "output_text" {
							text += part.Text
						}
					}
				case "reasoning":
				default:
					return "", invalidOutput
				}
			}
			if len(calls) == 0 && strings.TrimSpace(text) == "" {
				return "", invalidOutput
			}
			var original struct {
				Response struct {
					Output []json.RawMessage `json:"output"`
					Usage  struct {
						Total uint64 `json:"total_tokens"`
					} `json:"usage"`
				} `json:"response"`
			}
			if json.Unmarshal([]byte(data), &original) != nil {
				return "", invalidOutput
			}
			encoded, err := json.Marshal(map[string]any{"content": text, "tool_calls": calls, "provider_items": original.Response.Output, "used_tokens": original.Response.Usage.Total})
			if err != nil || len(encoded) > 32768 {
				return "", invalidOutput
			}
			return string(encoded), nil
		}
	}
	return "", invalidOutput
}
