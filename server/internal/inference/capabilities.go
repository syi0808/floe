package inference

import "sort"

const (
	ChatCapability             = "chat"
	StructuredOutputCapability = "structured_output"
	ToolProposalsCapability    = "tool_proposals"
)

// Capabilities are declared by the operator for the configured model. Provider
// family names and successful text probes do not establish feature support.
func ValidCapabilities(values []string) bool {
	if len(values) < 1 || len(values) > 3 || values[0] != ChatCapability || !sort.StringsAreSorted(values) {
		return false
	}
	for i, value := range values {
		if value != ChatCapability && value != StructuredOutputCapability && value != ToolProposalsCapability || i > 0 && values[i-1] == value {
			return false
		}
	}
	return true
}

func SupportsAgent(capabilities []string, in AgentInvocation) bool {
	if !ValidCapabilities(capabilities) || !validOutputFormat(in.OutputFormat) || in.OutputFormat.Kind == "json" && len(in.Input.Tools) != 0 {
		return false
	}
	has := func(value string) bool {
		index := sort.SearchStrings(capabilities, value)
		return index < len(capabilities) && capabilities[index] == value
	}
	return (in.OutputFormat.Kind != "json" || has(StructuredOutputCapability)) &&
		(len(in.Input.Tools) == 0 || has(ToolProposalsCapability))
}
