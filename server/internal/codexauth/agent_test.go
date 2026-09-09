package codexauth

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestNativeCallResultPairing(test *testing.T) {
	items, tools, err := nativeInput(json.RawMessage(`{"messages":[{"role":"assistant","tool_calls":[{"id":"call_1","function":{"name":"read","arguments":"{}"}}]},{"role":"tool","tool_call_id":"call_1","content":"observed"}],"tools":[{"function":{"name":"read","description":"Read","parameters":{"type":"object"}}}]}`))
	if err != nil || len(tools) != 1 || len(items) != 2 {
		test.Fatal(items, tools, err)
	}
	if items[0].(map[string]any)["call_id"] != items[1].(map[string]any)["call_id"] {
		test.Fatal("lost call identity")
	}
}

func TestNativeResponseRequiresTerminalCompletion(test *testing.T) {
	partial := "data: {\"type\":\"response.output_text.done\",\"text\":\"not final\"}\n\n"
	if _, err := readNativeResponse(strings.NewReader(partial)); err == nil {
		test.Fatal("accepted incomplete stream")
	}
	completed := `data: {"type":"response.completed","response":{"status":"completed","output":[{"type":"reasoning"},{"type":"function_call","name":"read","call_id":"call_1","arguments":"{}"}]}}` + "\n\n"
	output, err := readNativeResponse(strings.NewReader(completed))
	if err != nil || !strings.Contains(output, "call_1") {
		test.Fatal(output, err)
	}
}

func TestNativeReplayPreservesReasoningAndCallIdentity(test *testing.T) {
	raw := json.RawMessage(`{"messages":[{"role":"assistant","provider_items":[{"type":"reasoning","encrypted_content":"opaque"},{"type":"function_call","call_id":"provider_1","name":"read","arguments":"{}"}],"tool_calls":[{"id":"provider_1","function":{"name":"read","arguments":"{}"}}]},{"role":"tool","tool_call_id":"provider_1","content":"result"}],"tools":[]}`)
	items, _, err := nativeInput(raw)
	if err != nil || len(items) != 3 {
		test.Fatal(items, err)
	}
	if items[0].(map[string]any)["encrypted_content"] != "opaque" || items[1].(map[string]any)["call_id"] != items[2].(map[string]any)["call_id"] {
		test.Fatal("lost replay data")
	}
}

func TestNativeReplayCannotReplaceTheRecordedCall(test *testing.T) {
	for _, call := range []string{
		`{"type":"function_call","call_id":"wrong","name":"read","arguments":"{}"}`,
		`{"type":"function_call","call_id":"original","name":"write","arguments":"{}"}`,
		`{"type":"function_call","call_id":"original","name":"read","arguments":"{\"changed\":true}"}`,
		`{"type":"reasoning"}`,
	} {
		raw := json.RawMessage(`{"messages":[{"role":"assistant","provider_items":[` + call + `],"tool_calls":[{"id":"original","function":{"name":"read","arguments":"{}"}}]}],"tools":[]}`)
		if _, _, err := nativeInput(raw); err == nil {
			test.Fatal("accepted mismatched replay", call)
		}
	}
}
