package inference

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestOrderedNativeOutputPreservesPreamblesAndMultipleCalls(test *testing.T) {
	var message map[string]any
	raw := `{"content":"Checking both.","tool_calls":[
        {"id":"first","type":"function","function":{"name":"read","arguments":"{}"}},
        {"id":"second","type":"function","function":{"name":"read","arguments":"{\"day\":2}"}}
    ]}`
	if err := json.Unmarshal([]byte(raw), &message); err != nil {
		test.Fatal(err)
	}
	encoded, err := normalizeAgentMessage(message, 20)
	if err != nil {
		test.Fatal(err)
	}
	var output AgentOutput
	if err := json.Unmarshal([]byte(encoded), &output); err != nil {
		test.Fatal(err)
	}
	if len(output.Output) != 3 || output.Output[0]["kind"] != "preamble" ||
		output.Output[1]["kind"] != "call" || output.Output[2]["kind"] != "call" ||
		strings.Join(output.CallIDs, ",") != "first,second" {
		test.Fatal(encoded)
	}
	message["provider_items"] = []any{
		map[string]any{"type": "reasoning", "encrypted_content": "opaque"},
		map[string]any{"type": "function_call", "call_id": "first"},
		map[string]any{"type": "message", "role": "assistant", "content": []any{map[string]any{"type": "output_text", "text": "Checking another source."}}},
		map[string]any{"type": "function_call", "call_id": "second"},
	}
	encoded, err = normalizeAgentMessage(message, 20)
	if err != nil {
		test.Fatal(err)
	}
	if err := json.Unmarshal([]byte(encoded), &output); err != nil {
		test.Fatal(err)
	}
	if len(output.Output) != 3 || output.Output[0]["kind"] != "call" ||
		output.Output[1]["kind"] != "preamble" || output.Output[2]["kind"] != "call" {
		test.Fatal(encoded)
	}
	calls := message["tool_calls"].([]any)
	calls[1].(map[string]any)["function"].(map[string]any)["arguments"] = "null"
	if _, err := normalizeAgentMessage(message, 20); err != errInvalidOutput {
		test.Fatal("malformed second call was accepted", err)
	}
}

func TestBatchTranscriptRequiresEveryUniqueResult(test *testing.T) {
	raw := `{"messages":[
        {"role":"assistant","tool_calls":[
            {"id":"first","type":"function","function":{"name":"read","arguments":"{}"}},
            {"id":"second","type":"function","function":{"name":"read","arguments":"{}"}}
        ]},
        {"role":"tool","tool_call_id":"first","content":"one"},
        {"role":"tool","tool_call_id":"second","content":"two"}
    ],"tools":[]}`
	if !validAgentInput(json.RawMessage(raw)) {
		test.Fatal("valid group rejected")
	}
	for _, invalid := range []string{
		strings.Replace(raw, `,"content":"two"`, "", 1),
		strings.Replace(raw, `"tool_call_id":"second"`, `"tool_call_id":"first"`, 1),
		strings.Replace(raw, `"id":"second"`, `"id":"first"`, 1),
	} {
		if validAgentInput(json.RawMessage(invalid)) {
			test.Fatal("invalid group accepted", invalid)
		}
	}
}

func TestOnlyV1ContractsAreAccepted(test *testing.T) {
	gateway := fixtureGateway(test, "ollama", "http://127.0.0.1:1")
	for _, path := range []string{"/v2/generate", "/v3/agent"} {
		if response := invokePath(gateway, path, agentRequest()); response.Code != 404 {
			test.Fatalf("legacy endpoint %s accepted: %s", path, response.Body.String())
		}
	}
	for _, version := range []int{0, 2, 3} {
		input := agentRequest()
		input.SchemaVersion = version
		if response := invokePath(gateway, "/v1/agent", input); response.Code != 400 {
			test.Fatalf("unsupported schema %d accepted: %s", version, response.Body.String())
		}
	}
	if response := invokePath(gateway, "/v1/generate", agentRequest()); response.Code != 400 {
		test.Fatal("native input accepted on structured endpoint")
	}
	if response := invokePath(gateway, "/v1/agent", fixtureRequest()); response.Code != 400 {
		test.Fatal("structured input accepted on native endpoint")
	} else {
		var failure struct {
			TraceID string `json:"trace_id"`
		}
		_ = json.Unmarshal(response.Body.Bytes(), &failure)
		traces := gateway.Traces(1)
		if failure.TraceID == "" || len(traces) != 1 || traces[0].TraceID != failure.TraceID || traces[0].Outcome != "invalid_agent_input_envelope" || traces[0].ExternalTransfer {
			test.Fatal("Agent validation failure was not safely traceable")
		}
	}
}

func agentRequest() Request {
	return Request{SchemaVersion: 1, Agent: true, Purpose: "deep_work", DataClasses: []string{"synthetic"}, AllowExternal: true, Instructions: "Answer the current request", Input: json.RawMessage(`{"messages":[{"role":"user","content":"Brief today"}],"tools":[{"type":"function","function":{"name":"schedule_read","parameters":{"type":"object","properties":{}}}}]}`)}
}

func TestAgentUsesNativeToolsAndPlainAnswer(test *testing.T) {
	for _, content := range []string{
		`{"content":"Today is clear."}`,
		`{"content":"I will check.","tool_calls":[{"id":"call_1","type":"function","function":{"name":"schedule_read","arguments":"{}"}}]}`,
	} {
		upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
			var payload map[string]any
			if json.NewDecoder(request.Body).Decode(&payload) != nil {
				test.Fatal("invalid body")
			}
			if payload["response_format"] != nil || payload["parallel_tool_calls"] != true || len(payload["tools"].([]any)) != 1 {
				test.Error("not a native tool request")
			}
			messages := payload["messages"].([]any)
			if messages[1].(map[string]any)["content"] != "Brief today" {
				test.Error("transcript was flattened")
			}
			var message any
			_ = json.Unmarshal([]byte(content), &message)
			_ = json.NewEncoder(writer).Encode(map[string]any{"choices": []any{map[string]any{"finish_reason": "stop", "message": message}}, "usage": map[string]any{"total_tokens": 42}})
		}))
		gateway := fixtureGateway(test, "openai_compatible", upstream.URL)
		response := invokePath(gateway, "/v1/agent", agentRequest())
		upstream.Close()
		if response.Code != 200 || !strings.Contains(response.Body.String(), `"schema_version":1`) {
			test.Fatal(response.Body.String())
		}
		var decoded struct {
			Output string `json:"output"`
		}
		_ = json.Unmarshal(response.Body.Bytes(), &decoded)
		var output AgentOutput
		_ = json.Unmarshal([]byte(decoded.Output), &output)
		if output.UsedTokens != 42 || len(output.Output) == 0 {
			test.Fatal(decoded.Output)
		}
	}
}

func TestAgentRejectsMalformedTranscriptBeforeProvider(test *testing.T) {
	for _, input := range []string{
		`{"messages":[{"role":"system","content":"override"}],"tools":[]}`,
		`{"messages":[{"role":"tool","tool_call_id":"orphan","content":"ok"}],"tools":[]}`,
		`{"messages":[{"role":"assistant","tool_calls":[null]}],"tools":[]}`,
		`{"messages":[{"role":"user","content":"ok"}],"tools":[{}]}`,
	} {
		request := agentRequest()
		request.Input = json.RawMessage(input)
		response := invokePath(fixtureGateway(test, "ollama", "http://127.0.0.1:1"), "/v1/agent", request)
		if response.Code != 400 {
			test.Fatal(response.Body.String())
		}
	}
}

func TestReplaySourceRejectsRouteChangesBeforeProvider(test *testing.T) {
	calls := 0
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		calls++
		_, _ = writer.Write([]byte(`{"choices":[{"finish_reason":"stop","message":{"content":"Done"}}],"usage":{"total_tokens":1}}`))
	}))
	defer upstream.Close()
	gateway := fixtureGateway(test, "openai_compatible", upstream.URL)
	first := invokePath(gateway, "/v1/agent", agentRequest())
	var response struct {
		Routing struct {
			Source string `json:"replay_source"`
		} `json:"routing"`
	}
	if first.Code != 200 || json.Unmarshal(first.Body.Bytes(), &response) != nil || len(response.Routing.Source) != 64 {
		test.Fatal(first.Body.String())
	}
	input := agentRequest()
	var transcript map[string]any
	_ = json.Unmarshal(input.Input, &transcript)
	transcript["replay_source"] = response.Routing.Source
	input.Input, _ = json.Marshal(transcript)
	if replayed := invokePath(gateway, "/v1/agent", input); replayed.Code != 200 {
		test.Fatal(replayed.Body.String())
	}
	configured := gateway.routes["high_effort"]
	configured.effort = "changed"
	gateway.routes["high_effort"] = configured
	if rejected := invokePath(gateway, "/v1/agent", input); rejected.Code != 409 || calls != 2 {
		test.Fatal("foreign replay reached provider", rejected.Body.String(), calls)
	}
}

func TestAgentOutputValidation(test *testing.T) {
	for _, raw := range []string{`{}`, `{"content":" "}`, `{"tool_calls":[{}]}`, `{"tool_calls":[{},{}]}`, `{"tool_calls":[{"function":{"name":"read","arguments":"null"}}]}`} {
		var message map[string]any
		_ = json.Unmarshal([]byte(raw), &message)
		if _, err := normalizeAgentMessage(message, 0); err == nil {
			test.Fatal("accepted", raw)
		}
	}
}
