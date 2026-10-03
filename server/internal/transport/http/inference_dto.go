package httptransport

import (
	"bytes"
	"encoding/json"
	"floe/server/internal/inference"
	"floe/server/internal/trust"
	"io"
)

type PurposeCapabilityDTO struct {
	Status             string   `json:"status"`
	CapabilityRevision string   `json:"capability_revision,omitempty"`
	Capabilities       []string `json:"capabilities,omitempty"`
}
type InventoryResponseDTO struct {
	SchemaVersion int `json:"schema_version"`
	Purposes      struct {
		QuickResponse      PurposeCapabilityDTO `json:"quick_response"`
		EverydayAssistance PurposeCapabilityDTO `json:"everyday_assistance"`
		DeepWork           PurposeCapabilityDTO `json:"deep_work"`
	} `json:"purposes"`
}
type AgentRequestDTO struct {
	SchemaVersion      int               `json:"schema_version"`
	Purpose            inference.Purpose `json:"purpose"`
	CapabilityRevision string            `json:"capability_revision"`
	AttemptID          string            `json:"attempt_id"`
	DataClasses        []string          `json:"data_classes"`
	Instructions       string            `json:"instructions"`
	Input              json.RawMessage   `json:"input"`
	OutputFormat       json.RawMessage   `json:"output_format"`
	MaxOutputBytes     uint64            `json:"max_output_bytes"`
}
type StructuredRequestDTO struct {
	SchemaVersion      int               `json:"schema_version"`
	Purpose            inference.Purpose `json:"purpose"`
	CapabilityRevision string            `json:"capability_revision"`
	AttemptID          string            `json:"attempt_id"`
	DataClasses        []string          `json:"data_classes"`
	Instructions       string            `json:"instructions"`
	Input              json.RawMessage   `json:"input"`
	OutputSchema       json.RawMessage   `json:"output_schema"`
	MaxOutputBytes     uint64            `json:"max_output_bytes"`
}
type AgentResponseDTO struct {
	SchemaVersion      int                        `json:"schema_version"`
	Purpose            inference.Purpose          `json:"purpose"`
	CapabilityRevision string                     `json:"capability_revision"`
	AttemptID          string                     `json:"attempt_id"`
	TraceID            string                     `json:"trace_id"`
	Output             []inference.Step           `json:"output"`
	CallIDs            []string                   `json:"call_ids"`
	Usage              inference.UsageObservation `json:"usage"`
}
type StructuredResponseDTO struct {
	SchemaVersion      int                        `json:"schema_version"`
	Purpose            inference.Purpose          `json:"purpose"`
	CapabilityRevision string                     `json:"capability_revision"`
	AttemptID          string                     `json:"attempt_id"`
	TraceID            string                     `json:"trace_id"`
	Output             json.RawMessage            `json:"output"`
	Usage              inference.UsageObservation `json:"usage"`
}
type InferenceErrorDTO struct {
	SchemaVersion int `json:"schema_version"`
	Error         struct {
		Code inference.FailureCode `json:"code"`
	} `json:"error"`
	TraceID            *string                    `json:"trace_id"`
	AttemptID          *string                    `json:"attempt_id"`
	Purpose            *inference.Purpose         `json:"purpose"`
	CapabilityRevision *string                    `json:"capability_revision"`
	Usage              inference.UsageObservation `json:"usage"`
}

func exactObject(data []byte, required []string, optional ...string) (map[string]json.RawMessage, bool) {
	data = bytes.TrimSpace(data)
	var fields map[string]json.RawMessage
	if len(data) == 0 || data[0] != '{' || json.Unmarshal(data, &fields) != nil || fields == nil {
		return nil, false
	}
	allowed := map[string]bool{}
	for _, k := range required {
		v, ok := fields[k]
		if !ok || bytes.Equal(bytes.TrimSpace(v), []byte("null")) {
			return nil, false
		}
		allowed[k] = true
	}
	for _, k := range optional {
		allowed[k] = true
	}
	for k, v := range fields {
		if !allowed[k] || bytes.Equal(bytes.TrimSpace(v), []byte("null")) {
			return nil, false
		}
	}
	return fields, true
}
func decodeDTO(data []byte, out any) bool {
	d := json.NewDecoder(bytes.NewReader(data))
	d.DisallowUnknownFields()
	return d.Decode(out) == nil && d.Decode(new(any)) == io.EOF
}
func decodeAgentRequest(data []byte) (inference.AgentInvocation, error) {
	if trust.StrictJSON(data, 98304, 32) != nil || !inference.ValidJSONTextEncoding(data) {
		return inference.AgentInvocation{}, inference.Failure{Code: inference.Validation}
	}
	if _, ok := exactObject(bytes.TrimSpace(data), []string{"schema_version", "purpose", "capability_revision", "attempt_id", "data_classes", "instructions", "input", "output_format", "max_output_bytes"}); !ok {
		return inference.AgentInvocation{}, inference.Failure{Code: inference.Validation}
	}
	var dto AgentRequestDTO
	if !decodeDTO(data, &dto) {
		return inference.AgentInvocation{}, inference.Failure{Code: inference.Validation}
	}
	if dto.SchemaVersion != 2 {
		return inference.AgentInvocation{}, inference.Failure{Code: inference.UnsupportedSchema}
	}
	input, ok := decodeAgentInput(dto.Input)
	if !ok {
		return inference.AgentInvocation{}, inference.Failure{Code: inference.Validation}
	}
	format, err := inference.DecodeOutputFormat(dto.OutputFormat)
	if err != nil {
		return inference.AgentInvocation{}, err
	}
	out := inference.AgentInvocation{Purpose: dto.Purpose, CapabilityRevision: dto.CapabilityRevision, AttemptID: dto.AttemptID, DataClasses: dto.DataClasses, Instructions: dto.Instructions, Input: input, OutputFormat: format, MaxOutputBytes: dto.MaxOutputBytes}
	if err := inference.ValidateAgentInvocation(out); err != nil {
		return inference.AgentInvocation{}, err
	}
	return out, nil
}
func decodeStructuredRequest(data []byte) (inference.StructuredInvocation, error) {
	if trust.StrictJSON(data, 98304, 32) != nil {
		return inference.StructuredInvocation{}, inference.Failure{Code: inference.Validation}
	}
	if _, ok := exactObject(bytes.TrimSpace(data), []string{"schema_version", "purpose", "capability_revision", "attempt_id", "data_classes", "instructions", "input", "output_schema", "max_output_bytes"}); !ok {
		return inference.StructuredInvocation{}, inference.Failure{Code: inference.Validation}
	}
	var dto StructuredRequestDTO
	if !decodeDTO(data, &dto) {
		return inference.StructuredInvocation{}, inference.Failure{Code: inference.Validation}
	}
	if dto.SchemaVersion != 2 {
		return inference.StructuredInvocation{}, inference.Failure{Code: inference.UnsupportedSchema}
	}
	out := inference.StructuredInvocation{Purpose: dto.Purpose, CapabilityRevision: dto.CapabilityRevision, AttemptID: dto.AttemptID, DataClasses: dto.DataClasses, Instructions: dto.Instructions, Input: dto.Input, OutputSchema: dto.OutputSchema, MaxOutputBytes: dto.MaxOutputBytes}
	if err := inference.ValidateStructuredInvocation(out); err != nil {
		return inference.StructuredInvocation{}, err
	}
	return out, nil
}
func decodeAgentInput(data []byte) (inference.AgentInput, bool) {
	var out inference.AgentInput
	fields, ok := exactObject(bytes.TrimSpace(data), []string{"messages", "tools"})
	if !ok || len(data) > 32768 {
		return out, false
	}
	var messages, tools []json.RawMessage
	if json.Unmarshal(fields["messages"], &messages) != nil || json.Unmarshal(fields["tools"], &tools) != nil || messages == nil || tools == nil {
		return out, false
	}
	out.Messages = []inference.Message{}
	out.Tools = []inference.Tool{}
	for _, raw := range messages {
		var role struct {
			Role string `json:"role"`
		}
		if json.Unmarshal(raw, &role) != nil {
			return out, false
		}
		var fields map[string]json.RawMessage
		switch role.Role {
		case "user":
			fields, ok = exactObject(raw, []string{"role", "content"})
		case "tool":
			fields, ok = exactObject(raw, []string{"role", "tool_call_id", "content"})
		case "assistant":
			var probe map[string]json.RawMessage
			_ = json.Unmarshal(raw, &probe)
			if _, calls := probe["tool_calls"]; calls {
				fields, ok = exactObject(raw, []string{"role", "tool_calls"}, "content")
			} else {
				fields, ok = exactObject(raw, []string{"role", "content"})
			}
		default:
			return out, false
		}
		if !ok {
			return out, false
		}
		if callsRaw, exists := fields["tool_calls"]; exists {
			var calls []json.RawMessage
			if json.Unmarshal(callsRaw, &calls) != nil || len(calls) == 0 {
				return out, false
			}
			for _, call := range calls {
				c, ok := exactObject(call, []string{"id", "type", "function"})
				if !ok {
					return out, false
				}
				if _, ok = exactObject(c["function"], []string{"name", "arguments"}); !ok {
					return out, false
				}
			}
		}
		var message inference.Message
		if !decodeDTO(raw, &message) {
			return out, false
		}
		out.Messages = append(out.Messages, message)
	}
	for _, raw := range tools {
		fields, ok := exactObject(raw, []string{"type", "function"})
		if !ok {
			return out, false
		}
		f, ok := exactObject(fields["function"], []string{"name", "description", "parameters", "strict"})
		if !ok || string(bytes.TrimSpace(f["strict"])) != "false" {
			return out, false
		}
		var t inference.Tool
		if !decodeDTO(raw, &t) {
			return out, false
		}
		out.Tools = append(out.Tools, t)
	}
	return out, true
}
func inventoryDTO(i inference.PurposeInventory) InventoryResponseDTO {
	capability := func(c inference.PurposeCapability) PurposeCapabilityDTO {
		return PurposeCapabilityDTO{string(c.Status), c.CapabilityRevision, append([]string(nil), c.Capabilities...)}
	}
	var out InventoryResponseDTO
	out.SchemaVersion = 2
	out.Purposes.QuickResponse = capability(i.QuickResponse)
	out.Purposes.EverydayAssistance = capability(i.EverydayAssistance)
	out.Purposes.DeepWork = capability(i.DeepWork)
	return out
}
