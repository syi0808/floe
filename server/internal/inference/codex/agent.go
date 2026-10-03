package codexauth

import (
	"bufio"
	"encoding/json"
	"floe/server/internal/inference"
	"floe/server/internal/trust"
	"io"
	"reflect"
	"strings"
	"unicode"
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

func readNativeResponse(reader io.Reader) (string, inference.UsageObservation, error) {
	usage := inference.UsageObservation{}
	scanner := bufio.NewScanner(io.LimitReader(reader, 1048577))
	scanner.Buffer(make([]byte, 4096), 1048576)
	streamed := []json.RawMessage{}
	responseID := ""
	invalidContent := false
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
				ID     string          `json:"id"`
				Status string          `json:"status"`
				Output json.RawMessage `json:"output"`
				Usage  json.RawMessage `json:"usage"`
			} `json:"response"`
		}
		if trust.StrictJSON([]byte(data), 1048576, 32) != nil || !inference.ValidJSONTextEncoding([]byte(data)) || json.Unmarshal([]byte(data), &event) != nil {
			return "", usage, invalidOutput
		}
		switch event.Type {
		case "response.failed", "response.incomplete", "error":
			return "", usage, unavailable
		case "response.created", "response.in_progress":
			if !validNativeResponseID(event.Response.ID) || responseID != "" && responseID != event.Response.ID {
				return "", usage, invalidOutput
			}
			responseID = event.Response.ID
		case "response.output_item.done":
			if len(event.Item) == 0 || len(streamed) >= 32 {
				invalidContent = true
				continue
			}
			streamed = append(streamed, append(json.RawMessage(nil), event.Item...))
		case "response.completed":
			if event.Response.Status != "completed" || !validNativeResponseID(event.Response.ID) || responseID != "" && responseID != event.Response.ID {
				return "", usage, invalidOutput
			}
			var err error
			usage, err = completedNativeUsage(event.Response.Usage)
			if err != nil {
				return "", inference.UsageObservation{}, err
			}
			// Usage belongs to this completed response, independently of whether
			// its content can be normalized into the requested Agent contract.
			var output []json.RawMessage
			if invalidContent || len(event.Response.Output) != 0 && json.Unmarshal(event.Response.Output, &output) != nil {
				return "", usage, invalidOutput
			}
			if len(output) == 0 {
				output = streamed
			} else if len(streamed) != 0 && !sameNativeOutput(streamed, output) {
				return "", usage, invalidOutput
			}
			text, err := normalizeNativeOutput(output)
			return text, usage, err
		}
	}
	return "", usage, invalidOutput
}

func validNativeResponseID(value string) bool {
	if value == "" || len(value) > 256 {
		return false
	}
	for _, char := range value {
		if unicode.IsControl(char) {
			return false
		}
	}
	return true
}

func completedNativeUsage(raw json.RawMessage) (inference.UsageObservation, error) {
	unknown := inference.UsageObservation{}
	if len(raw) == 0 || strings.TrimSpace(string(raw)) == "null" {
		return unknown, nil
	}
	var fields map[string]json.RawMessage
	if json.Unmarshal(raw, &fields) != nil || fields == nil {
		return unknown, invalidOutput
	}
	value, present := fields["total_tokens"]
	if !present || strings.TrimSpace(string(value)) == "null" {
		return unknown, nil
	}
	var tokens uint64
	if json.Unmarshal(value, &tokens) != nil || tokens > trust.MaxJSONInteger {
		return unknown, invalidOutput
	}
	return inference.UsageObservation{Tokens: &tokens}, nil
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

func normalizeNativeOutput(output []json.RawMessage) (string, error) {
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
	encoded, err := json.Marshal(map[string]any{"content": text, "tool_calls": calls, "provider_items": output})
	if err != nil || len(encoded) > 32768 {
		return "", invalidOutput
	}
	return string(encoded), nil
}
