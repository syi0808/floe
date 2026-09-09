package inference

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

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
			if payload["response_format"] != nil || payload["parallel_tool_calls"] != false || len(payload["tools"].([]any)) != 1 {
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
		if output.UsedTokens != 42 || output.Step == nil {
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

func TestAgentOutputValidation(test *testing.T) {
	for _, raw := range []string{`{}`, `{"content":" "}`, `{"tool_calls":[{}]}`, `{"tool_calls":[{},{}]}`, `{"tool_calls":[{"function":{"name":"read","arguments":"null"}}]}`} {
		var message map[string]any
		_ = json.Unmarshal([]byte(raw), &message)
		if _, err := normalizeAgentMessage(message, 0); err == nil {
			test.Fatal("accepted", raw)
		}
	}
}
