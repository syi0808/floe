package inference

import "encoding/json"

type Purpose string

const (
	QuickResponse      Purpose = "quick_response"
	EverydayAssistance Purpose = "everyday_assistance"
	DeepWork           Purpose = "deep_work"
)

var Purposes = []Purpose{QuickResponse, EverydayAssistance, DeepWork}

func ValidPurpose(value string) bool {
	return value == string(QuickResponse) || value == string(EverydayAssistance) || value == string(DeepWork)
}

type PurposeStatus string

const (
	Available     PurposeStatus = "available"
	NotConfigured PurposeStatus = "not_configured"
	Disabled      PurposeStatus = "disabled"
)

type PurposeCapability struct {
	Status             PurposeStatus
	CapabilityRevision string
	Capabilities       []string
}
type PurposeInventory struct{ QuickResponse, EverydayAssistance, DeepWork PurposeCapability }

func (i PurposeInventory) Get(p Purpose) PurposeCapability {
	switch p {
	case QuickResponse:
		return i.QuickResponse
	case EverydayAssistance:
		return i.EverydayAssistance
	case DeepWork:
		return i.DeepWork
	}
	return PurposeCapability{}
}
func (i *PurposeInventory) set(p Purpose, c PurposeCapability) {
	switch p {
	case QuickResponse:
		i.QuickResponse = c
	case EverydayAssistance:
		i.EverydayAssistance = c
	case DeepWork:
		i.DeepWork = c
	}
}

type AgentInvocation struct {
	Purpose                       Purpose
	CapabilityRevision, AttemptID string
	DataClasses                   []string
	Instructions                  string
	Input                         AgentInput
	OutputFormat                  OutputFormat
	MaxOutputBytes                uint64
}
type OutputFormat struct {
	Kind string `json:"kind"`
	Schema json.RawMessage `json:"schema,omitempty"`
}
type StructuredInvocation struct {
	Purpose                       Purpose
	CapabilityRevision, AttemptID string
	DataClasses                   []string
	Instructions                  string
	Input, OutputSchema           json.RawMessage
	MaxOutputBytes                uint64
}
type AgentInput struct {
	Messages []Message `json:"messages"`
	Tools    []Tool    `json:"tools"`
}
type Message struct {
	Role       string     `json:"role"`
	Content    *string    `json:"content,omitempty"`
	ToolCallID string     `json:"tool_call_id,omitempty"`
	ToolCalls  []ToolCall `json:"tool_calls,omitempty"`
}
type ToolCall struct {
	ID       string       `json:"id"`
	Type     string       `json:"type"`
	Function CallFunction `json:"function"`
}
type CallFunction struct {
	Name      string `json:"name"`
	Arguments string `json:"arguments"`
}
type Tool struct {
	Type     string       `json:"type"`
	Function ToolFunction `json:"function"`
}
type ToolFunction struct {
	Name        string          `json:"name"`
	Description string          `json:"description"`
	Parameters  json.RawMessage `json:"parameters"`
	Strict      bool            `json:"strict"`
}
type Step struct {
	Kind         string `json:"kind"`
	Text         string `json:"text,omitempty"`
	CapabilityID string `json:"capability_id,omitempty"`
	Input        string `json:"input,omitempty"`
}
type UsageObservation struct {
	Tokens     *uint64 `json:"tokens"`
	CostMicros *uint64 `json:"cost_micros"`
}
type AgentResult struct {
	Purpose                                Purpose
	CapabilityRevision, AttemptID, TraceID string
	Output                                 []Step
	CallIDs                                []string
	Usage                                  UsageObservation
}
type StructuredResult struct {
	Purpose                                Purpose
	CapabilityRevision, AttemptID, TraceID string
	Output                                 json.RawMessage
	Usage                                  UsageObservation
}
type FailureCode string

const (
	Validation                     FailureCode = "validation"
	UnsupportedSchema              FailureCode = "unsupported_schema"
	Unauthorized                   FailureCode = "unauthorized"
	PermissionDenied               FailureCode = "permission_denied"
	IdentityMismatch               FailureCode = "identity_mismatch"
	NotFound                       FailureCode = "not_found"
	MethodNotAllowed               FailureCode = "method_not_allowed"
	CapabilityChanged              FailureCode = "capability_changed"
	PurposeNotConfigured           FailureCode = "purpose_not_configured"
	PurposeDisabled                FailureCode = "purpose_disabled"
	BodyTooLarge                   FailureCode = "body_too_large"
	ContentTypeUnsupported         FailureCode = "content_type_unsupported"
	ModelBusy                      FailureCode = "model_busy"
	QuotaExceeded                  FailureCode = "quota_exceeded"
	InvalidOutput                  FailureCode = "invalid_output"
	RequestRejected                FailureCode = "request_rejected"
	ProviderCredentialsUnavailable FailureCode = "provider_credentials_unavailable"
	ModelUnavailable               FailureCode = "model_unavailable"
	ModelTimeout                   FailureCode = "model_timeout"
)

type Failure struct {
	Code       FailureCode
	TraceID    string
	AttemptID string
	Purpose Purpose
	CapabilityRevision string
	Usage      UsageObservation
	Dispatched bool
}

func (f Failure) Error() string { return string(f.Code) }
