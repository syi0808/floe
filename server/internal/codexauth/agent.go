package codexauth

import (
	"bufio"
	"encoding/json"
	"io"
	"reflect"
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
			calls, ok := message["tool_calls"].([]any)
			if !ok || len(calls) == 0 || len(calls) > 8 {
				return nil, nil, invalidOutput
			}
			identifiers := map[string]bool{}
			for _, value := range calls {
				call, ok := value.(map[string]any)
				if !ok {
					return nil, nil, invalidOutput
				}
				identifier, ok := call["id"].(string)
				if !ok || identifier == "" || len(identifier) > 128 || identifiers[identifier] {
					return nil, nil, invalidOutput
				}
				identifiers[identifier] = true
			}
			callCount := 0
			for _, value := range replay {
				item, ok := value.(map[string]any)
				if !ok {
					return nil, nil, invalidOutput
				}
				kind, _ := item["type"].(string)
				if kind != "reasoning" && kind != "function_call" && !(kind == "message" && item["role"] == "assistant") {
					return nil, nil, invalidOutput
				}
				if kind == "function_call" {
					if callCount >= len(calls) {
						return nil, nil, invalidOutput
					}
					call, ok := calls[callCount].(map[string]any)
					if !ok {
						return nil, nil, invalidOutput
					}
					function, ok := call["function"].(map[string]any)
					if !ok {
						return nil, nil, invalidOutput
					}
					callCount++
					original, originalOK := item["arguments"].(string)
					canonical, canonicalOK := function["arguments"].(string)
					var originalValue, canonicalValue any
					if item["call_id"] != call["id"] || item["name"] != function["name"] ||
						!originalOK || !canonicalOK ||
						json.Unmarshal([]byte(original), &originalValue) != nil ||
						json.Unmarshal([]byte(canonical), &canonicalValue) != nil ||
						!reflect.DeepEqual(originalValue, canonicalValue) {
						return nil, nil, invalidOutput
					}
				}
				items = append(items, item)
			}
			if callCount != len(calls) {
				return nil, nil, invalidOutput
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

type nativeOutputItem struct {
	Type      string `json:"type"`
	Name      string `json:"name"`
	CallID    string `json:"call_id"`
	Arguments string `json:"arguments"`
	Content   []struct {
		Type string `json:"type"`
		Text string `json:"text"`
	} `json:"content"`
}

func readNativeResponse(reader io.Reader) (string, error) {
	scanner := bufio.NewScanner(io.LimitReader(reader, 1048577))
	scanner.Buffer(make([]byte, 4096), 1048576)
	streamed := []json.RawMessage{}
	for scanner.Scan() {
		if !strings.HasPrefix(scanner.Text(), "data:") {
			continue
		}
		data := strings.TrimSpace(strings.TrimPrefix(scanner.Text(), "data:"))
		if data == "" || data == "[DONE]" {
			continue
		}
		var event struct {
			Type     string          `json:"type"`
			Item     json.RawMessage `json:"item"`
			Response struct {
				Status string            `json:"status"`
				Output []json.RawMessage `json:"output"`
				Usage  struct {
					Total uint64 `json:"total_tokens"`
				} `json:"usage"`
			} `json:"response"`
		}
		if json.Unmarshal([]byte(data), &event) != nil {
			return "", invalidOutput
		}
		switch event.Type {
		case "response.failed", "response.incomplete", "error":
			return "", unavailable
		case "response.output_item.done":
			if len(event.Item) == 0 || len(streamed) >= 32 {
				return "", invalidOutput
			}
			streamed = append(streamed, append(json.RawMessage(nil), event.Item...))
		case "response.completed":
			if event.Response.Status != "completed" {
				return "", invalidOutput
			}
			output := event.Response.Output
			if len(output) == 0 {
				output = streamed
			} else if len(streamed) != 0 && !sameNativeOutput(streamed, output) {
				return "", invalidOutput
			}
			return normalizeNativeOutput(output, event.Response.Usage.Total)
		}
	}
	return "", invalidOutput
}

func sameNativeOutput(left, right []json.RawMessage) bool {
	leftEncoded, leftError := json.Marshal(left)
	rightEncoded, rightError := json.Marshal(right)
	if leftError != nil || rightError != nil {
		return false
	}
	var leftValue, rightValue []any
	return json.Unmarshal(leftEncoded, &leftValue) == nil &&
		json.Unmarshal(rightEncoded, &rightValue) == nil &&
		reflect.DeepEqual(leftValue, rightValue)
}

func normalizeNativeOutput(output []json.RawMessage, usedTokens uint64) (string, error) {
	if len(output) == 0 || len(output) > 32 {
		return "", invalidOutput
	}
	text := ""
	calls := []any{}
	for _, raw := range output {
		var item nativeOutputItem
		if json.Unmarshal(raw, &item) != nil {
			return "", invalidOutput
		}
		switch item.Type {
		case "function_call":
			if item.CallID == "" || item.Name == "" || item.Arguments == "" {
				return "", invalidOutput
			}
			calls = append(calls, map[string]any{"id": item.CallID, "type": "function", "function": map[string]string{"name": item.Name, "arguments": item.Arguments}})
		case "message":
			for _, part := range item.Content {
				switch part.Type {
				case "refusal":
					return "", unavailable
				case "output_text":
					text += part.Text
				default:
					return "", invalidOutput
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
	encoded, err := json.Marshal(map[string]any{"content": text, "tool_calls": calls, "provider_items": output, "used_tokens": usedTokens})
	if err != nil || len(encoded) > 32768 {
		return "", invalidOutput
	}
	return string(encoded), nil
}
