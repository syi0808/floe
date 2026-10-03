package providers

import (
	"context"
	"encoding/json"
	"floe/server/internal/inference"
	codexauth "floe/server/internal/inference/codex"
	"floe/server/internal/trust"
	"strings"
)

func (p *provider) agent(ctx context.Context, in inference.AgentInvocation, effort string) (out inference.AgentResult, err error) {
	if !inference.SupportsAgent(p.target.Capabilities, in) {
		return out, inference.Failure{Code: inference.RequestRejected}
	}
	input, err := json.Marshal(in.Input)
	if err != nil {
		return out, inference.Failure{Code: inference.Validation}
	}
	var message map[string]any
	if p.target.Provider == "codex_oauth" {
		identity := p.codex.ReplayIdentity()
		if identity == "" {
			return out, inference.Failure{Code: inference.ProviderCredentialsUnavailable}
		}
		ctx = codexauth.WithAccountIdentity(ctx, identity)
		raw, usage, e := p.codex.GenerateAgent(ctx, p.target.Model, effort, in.Instructions, input, in.OutputFormat.Schema)
		if inference.ValidateUsage(usage) != nil {
			return out, inference.Failure{Code: inference.InvalidOutput}
		}
		out.Usage = usage
		if e != nil {
			return out, classifyCodexError(e)
		}
		if trust.StrictJSON([]byte(raw), 1<<20, 32) != nil || json.Unmarshal([]byte(raw), &message) != nil {
			return out, inference.Failure{Code: inference.InvalidOutput}
		}
	} else {
		if err = p.checkLocal(ctx); err != nil {
			return out, err
		}
		var plain struct {
			Messages []map[string]any `json:"messages"`
			Tools    []map[string]any `json:"tools"`
		}
		if json.Unmarshal(input, &plain) != nil {
			return out, inference.Failure{Code: inference.Validation}
		}
		messages := append([]map[string]any{{"role": "system", "content": in.Instructions}}, plain.Messages...)
		payload := map[string]any{"model": p.target.Model, "messages": messages, "tools": plain.Tools, "stream": false}
		if p.target.Provider == "ollama" {
			if in.OutputFormat.Kind == "json" {
				payload["format"] = in.OutputFormat.Schema
				delete(payload, "tools")
			}
			for _, m := range messages {
				if calls, ok := m["tool_calls"].([]any); ok {
					for _, raw := range calls {
						call := raw.(map[string]any)
						function := call["function"].(map[string]any)
						var arguments any
						if json.Unmarshal([]byte(function["arguments"].(string)), &arguments) != nil {
							return out, inference.Failure{Code: inference.Validation}
						}
						function["arguments"] = arguments
					}
				}
			}
			var response struct {
				Done    bool           `json:"done"`
				Message map[string]any `json:"message"`
			}
			data, e := p.postJSON(ctx, "/api/chat", payload)
			if e != nil {
				return out, e
			}
			out.Usage, err = agentUsage(data, true)
			if err != nil {
				return out, err
			}
			if json.Unmarshal(data, &response) != nil || !response.Done {
				return out, inference.Failure{Code: inference.InvalidOutput}
			}
			message = response.Message
		} else {
			payload["parallel_tool_calls"] = true
			if in.OutputFormat.Kind == "json" {
				// Optional properties retain their meaning. Exact portable schema
				// validation still gates release after provider generation.
				payload["response_format"] = map[string]any{"type": "json_schema", "json_schema": map[string]any{"name": "floe_result", "strict": false, "schema": in.OutputFormat.Schema}}
				delete(payload, "tools")
				delete(payload, "parallel_tool_calls")
			}
			if effort != "" {
				payload["reasoning_effort"] = effort
			}
			var response struct {
				Choices []struct {
					Finish  string         `json:"finish_reason"`
					Message map[string]any `json:"message"`
				} `json:"choices"`
			}
			data, e := p.postJSON(ctx, "/chat/completions", payload)
			if e != nil {
				return out, e
			}
			out.Usage, err = agentUsage(data, false)
			if err != nil {
				return out, err
			}
			if json.Unmarshal(data, &response) != nil || len(response.Choices) != 1 || response.Choices[0].Finish != "stop" && response.Choices[0].Finish != "tool_calls" {
				return out, inference.Failure{Code: inference.InvalidOutput}
			}
			message = response.Choices[0].Message
		}
	}
	output, ids, err := normalizeMessage(message)
	out.Output, out.CallIDs = output, ids
	if err != nil {
		return out, err
	}
	if err = inference.ValidateAgentResult(in, out); err != nil {
		return out, err
	}
	return out, nil
}

// The HTTP status and bounded JSON framing have been validated by postJSON.
// Decode observations independently so content type errors cannot erase usage.
func agentUsage(data []byte, ollama bool) (inference.UsageObservation, error) {
	unknown := inference.UsageObservation{}
	bad := inference.Failure{Code: inference.InvalidOutput}
	var fields map[string]json.RawMessage
	if json.Unmarshal(data, &fields) != nil || fields == nil {
		return unknown, bad
	}
	if ollama {
		prompt, err := usageCount(fields["prompt_eval_count"])
		if err != nil {
			return unknown, err
		}
		output, err := usageCount(fields["eval_count"])
		if err != nil {
			return unknown, err
		}
		if prompt != nil && output != nil && *output > trust.MaxJSONInteger-*prompt {
			return unknown, bad
		}
		return inference.UsageObservation{Tokens: sumUsage(prompt, output)}, nil
	}
	raw := fields["usage"]
	if len(raw) == 0 || strings.TrimSpace(string(raw)) == "null" {
		return unknown, nil
	}
	var usage map[string]json.RawMessage
	if json.Unmarshal(raw, &usage) != nil || usage == nil {
		return unknown, bad
	}
	tokens, err := usageCount(usage["total_tokens"])
	if err != nil {
		return unknown, err
	}
	return inference.UsageObservation{Tokens: tokens}, nil
}

func usageCount(raw json.RawMessage) (*uint64, error) {
	if len(raw) == 0 || strings.TrimSpace(string(raw)) == "null" {
		return nil, nil
	}
	var value uint64
	if json.Unmarshal(raw, &value) != nil || value > trust.MaxJSONInteger {
		return nil, inference.Failure{Code: inference.InvalidOutput}
	}
	return &value, nil
}

func normalizeMessage(message map[string]any) ([]inference.Step, []string, error) {
	bad := func() ([]inference.Step, []string, error) {
		return nil, nil, inference.Failure{Code: inference.InvalidOutput}
	}
	if message == nil || message["refusal"] != nil {
		return bad()
	}
	calls, ok := message["tool_calls"].([]any)
	if message["tool_calls"] != nil && !ok || len(calls) > 8 {
		return bad()
	}
	steps := map[string]inference.Step{}
	ids := []string{}
	for _, raw := range calls {
		call, ok := raw.(map[string]any)
		if !ok || call["type"] != nil && call["type"] != "function" {
			return bad()
		}
		f, ok := call["function"].(map[string]any)
		if !ok {
			return bad()
		}
		name, ok := f["name"].(string)
		if !ok || !inference.ValidAlias(name) {
			return bad()
		}
		id, _ := call["id"].(string)
		if id == "" {
			id = trust.NewID()
		}
		if _, ok := steps[id]; ok {
			return bad()
		}
		args, ok := f["arguments"].(string)
		if !ok {
			raw, err := json.Marshal(f["arguments"])
			if err != nil {
				return bad()
			}
			args = string(raw)
		}
		if trust.StrictJSON([]byte(args), 32768, 32) != nil || !strings.HasPrefix(strings.TrimSpace(args), "{") {
			return bad()
		}
		steps[id] = inference.Step{Kind: "call", CapabilityID: name, Input: args}
		ids = append(ids, id)
	}
	output := []inference.Step{}
	text := func(s string) {
		if strings.TrimSpace(s) != "" {
			output = append(output, inference.Step{Kind: "preamble", Text: s})
		}
	}
	// Native reasoning items are consumed within this invocation and never exported or replayed.
	if items, ok := message["provider_items"].([]any); ok {
		ordered := []string{}
		for _, raw := range items {
			item, ok := raw.(map[string]any)
			if !ok {
				return bad()
			}
			switch item["type"] {
			case "function_call":
				id, _ := item["call_id"].(string)
				step, ok := steps[id]
				if !ok {
					return bad()
				}
				delete(steps, id)
				ordered = append(ordered, id)
				output = append(output, step)
			case "message":
				parts, ok := item["content"].([]any)
				if !ok || item["role"] != "assistant" {
					return bad()
				}
				for _, raw := range parts {
					part, ok := raw.(map[string]any)
					if !ok {
						return bad()
					}
					if part["type"] == "refusal" {
						return bad()
					}
					if part["type"] == "output_text" {
						s, ok := part["text"].(string)
						if !ok {
							return bad()
						}
						text(s)
					}
				}
			case "reasoning":
			default:
				return bad()
			}
		}
		if len(steps) != 0 {
			return bad()
		}
		ids = ordered
	} else {
		s, _ := message["content"].(string)
		text(s)
		for _, id := range ids {
			output = append(output, steps[id])
		}
	}
	if len(output) == 0 || len(output) > 16 {
		return bad()
	}
	if len(ids) == 0 {
		output[len(output)-1].Kind = "answer"
	}
	return output, ids, nil
}
