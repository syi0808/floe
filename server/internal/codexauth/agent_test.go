package codexauth

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestNativeReplayValidatesEveryCallInOrderedGroup(test *testing.T) {
	raw := `{"messages":[{"role":"assistant","provider_items":[
        {"type":"reasoning","encrypted_content":"opaque"},
        {"type":"function_call","call_id":"first","name":"read","arguments":"{}"},
        {"type":"message","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"Checking more."}]},
        {"type":"function_call","call_id":"second","name":"read","arguments":"{\"day\":2}"}
    ],"tool_calls":[
        {"id":"first","function":{"name":"read","arguments":"{}"}},
        {"id":"second","function":{"name":"read","arguments":"{\"day\":2}"}}
    ]},{"role":"tool","tool_call_id":"first","content":"one"},{"role":"tool","tool_call_id":"second","content":"two"}],"tools":[]}`
	items, _, err := nativeInput(json.RawMessage(raw))
	if err != nil || len(items) != 6 {
		test.Fatal(items, err)
	}
	if items[2].(map[string]any)["phase"] != "commentary" ||
		items[4].(map[string]any)["call_id"] != "first" || items[5].(map[string]any)["call_id"] != "second" {
		test.Fatal(items)
	}
	changed := strings.Replace(raw, `"call_id":"second"`, `"call_id":"foreign"`, 1)
	if _, _, err := nativeInput(json.RawMessage(changed)); err == nil {
		test.Fatal("second replay call was not validated")
	}
}

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
	streamed := strings.Join([]string{
		`data: {"type":"response.output_item.done","item":{"type":"reasoning","encrypted_content":"opaque"}}`,
		`data: {"type":"response.output_item.done","item":{"type":"function_call","name":"read","call_id":"call_streamed","arguments":"{}"}}`,
		`data: {"type":"response.completed","response":{"status":"completed","output":[],"usage":{"total_tokens":37}}}`,
	}, "\n\n") + "\n\n"
	output, err = readNativeResponse(strings.NewReader(streamed))
	if err != nil || !strings.Contains(output, "call_streamed") || !strings.Contains(output, `"used_tokens":37`) || !strings.Contains(output, "opaque") {
		test.Fatal(output, err)
	}
	mismatched := strings.Join([]string{
		`data: {"type":"response.output_item.done","item":{"type":"function_call","name":"read","call_id":"streamed","arguments":"{}"}}`,
		`data: {"type":"response.completed","response":{"status":"completed","output":[{"type":"function_call","name":"read","call_id":"different","arguments":"{}"}]}}`,
	}, "\n\n") + "\n\n"
	if _, err := readNativeResponse(strings.NewReader(mismatched)); err == nil {
		test.Fatal("accepted mismatched terminal output")
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
