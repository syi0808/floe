package inference

import (
	"encoding/json"
	"errors"
)

const (
	ModelBudgetProfileVersion  = 1
	ModelBudgetOverrideVersion = 1

	LegacyAgentInputBytes         = uint32(32_768)
	GatewayInstructionBytes       = uint32(9_216)
	GatewayMessageCount           = uint32(256)
	GatewayToolCount              = uint32(64)
	AgentConversationBytes        = uint32(128 * 1024)
	GatewayRequestBytes           = uint32(98_304)
	GatewayOutputBytes            = uint32(16_384)
	MaximumKnownModelTokens       = uint32(10_000_000)
	modelInputEstimateMethod      = "utf8_model_input_bytes_upper_estimate_v1"
	modelInputEstimateUncertainty = "exact_tokenizer_unavailable"
)

const (
	LimitUnknown               = "unknown"
	LimitKnown                 = "known"
	LimitSourceUnknown         = "unknown"
	LimitSourceOperator        = "operator_configuration"
	LimitSourceProvider        = "provider_confirmed"
	ProviderConfirmedUnknown   = "unknown"
	ProviderConfirmedAvailable = "available"
	CatalogUnavailable         = "unavailable"
	CatalogModelNotListed      = "model_not_listed"
	CatalogMetadataUnknown     = "metadata_unknown"
	CatalogMetadataPresent     = "available"
	BudgetOverrideUnconfigured = "unconfigured"
	BudgetOverrideConfigured   = "configured"
)

// ModelBudgetOverride is explicit, local operator configuration attached to a
// selected provider model. Catalog suggestion metadata never populates it.
// Values are optional so a partially measured profile remains explicitly
// unknown instead of borrowing a limit from descriptive catalog data.
type ModelBudgetOverride struct {
	SchemaVersion                   int     `json:"schema_version"`
	ContextWindowTokens             *uint32 `json:"context_window_tokens,omitempty"`
	MaxOutputTokens                 *uint32 `json:"max_output_tokens,omitempty"`
	SelectedOutputReservationTokens *uint32 `json:"selected_output_reservation_tokens,omitempty"`
	ProviderOverheadTokens          *uint32 `json:"provider_overhead_tokens,omitempty"`
	SafetyMarginTokens              *uint32 `json:"safety_margin_tokens,omitempty"`
	MaxInputJSONBytes               *uint32 `json:"max_input_json_bytes,omitempty"`
}

func (o *ModelBudgetOverride) Validate() error {
	if o == nil || o.SchemaVersion != ModelBudgetOverrideVersion {
		return errors.New("invalid model budget override version")
	}
	configured := false
	for _, value := range []*uint32{o.ContextWindowTokens, o.MaxOutputTokens, o.SelectedOutputReservationTokens, o.ProviderOverheadTokens, o.SafetyMarginTokens} {
		if value != nil {
			configured = true
			if *value == 0 || *value > MaximumKnownModelTokens {
				return errors.New("invalid model token limit")
			}
		}
	}
	if o.MaxInputJSONBytes != nil {
		configured = true
		if *o.MaxInputJSONBytes == 0 || *o.MaxInputJSONBytes > GatewayRequestBytes {
			return errors.New("invalid model input framing limit")
		}
	}
	if !configured {
		return errors.New("empty model budget override")
	}
	if o.ContextWindowTokens != nil && o.MaxOutputTokens != nil && *o.MaxOutputTokens > *o.ContextWindowTokens {
		return errors.New("model output limit exceeds context window")
	}
	if o.SelectedOutputReservationTokens != nil {
		if o.ContextWindowTokens != nil && *o.SelectedOutputReservationTokens > *o.ContextWindowTokens {
			return errors.New("output reservation exceeds context window")
		}
		if o.MaxOutputTokens != nil && *o.SelectedOutputReservationTokens > *o.MaxOutputTokens {
			return errors.New("output reservation exceeds model output limit")
		}
	}
	return nil
}

func cloneModelBudgetOverride(value *ModelBudgetOverride) *ModelBudgetOverride {
	if value == nil {
		return nil
	}
	copy := *value
	clone := func(input *uint32) *uint32 {
		if input == nil {
			return nil
		}
		output := *input
		return &output
	}
	copy.ContextWindowTokens = clone(value.ContextWindowTokens)
	copy.MaxOutputTokens = clone(value.MaxOutputTokens)
	copy.SelectedOutputReservationTokens = clone(value.SelectedOutputReservationTokens)
	copy.ProviderOverheadTokens = clone(value.ProviderOverheadTokens)
	copy.SafetyMarginTokens = clone(value.SafetyMarginTokens)
	copy.MaxInputJSONBytes = clone(value.MaxInputJSONBytes)
	return &copy
}

func CloneModelBudgetOverride(value *ModelBudgetOverride) *ModelBudgetOverride {
	return cloneModelBudgetOverride(value)
}

type ModelTokenLimit struct {
	Status string  `json:"status"`
	Tokens *uint32 `json:"tokens"`
	Source string  `json:"source"`
}

type ModelTokenEstimatePolicy struct {
	Version                int     `json:"version"`
	Method                 string  `json:"method"`
	Uncertainty            string  `json:"uncertainty"`
	ProviderOverheadTokens *uint32 `json:"provider_overhead_tokens"`
	SafetyMarginTokens     *uint32 `json:"safety_margin_tokens"`
}

type ModelFramingGuards struct {
	ConfiguredInputJSONBytes *uint32 `json:"configured_input_json_bytes"`
	MaxInputJSONBytes        uint32  `json:"max_input_json_bytes"`
	MaxInstructionBytes      uint32  `json:"max_instruction_bytes"`
	MaxMessages              uint32  `json:"max_messages"`
	MaxTools                 uint32  `json:"max_tools"`
	MaxConversationBytes     uint32  `json:"max_conversation_bytes"`
	MaxRequestBytes          uint32  `json:"max_request_bytes"`
	MaxOutputBytes           uint32  `json:"max_output_bytes"`
}

type CatalogBudgetFacts struct {
	Status              string            `json:"status"`
	Revision            *uint64           `json:"revision"`
	ContextWindowTokens *uint32           `json:"context_window_tokens"`
	MaxOutputTokens     *uint32           `json:"max_output_tokens"`
	Provenance          *BudgetProvenance `json:"provenance"`
}

type BudgetProvenance struct {
	Source     string `json:"source"`
	VerifiedAt string `json:"verified_at"`
}

type ModelBudgetSources struct {
	Catalog                      CatalogBudgetFacts `json:"catalog"`
	OperatorConfiguration        string             `json:"operator_configuration"`
	ProviderConfirmed            string             `json:"provider_confirmed"`
	OperatorConfigurationVersion *int               `json:"operator_configuration_version"`
}

// ModelBudgetProfile is the non-secret resolved profile pinned by the Go
// capability revision and copied into the Rust PreparedModelPlan. Catalog
// values are descriptive facts; effective limits come only from an explicit
// operator override or a provider-confirmed source.
type ModelBudgetProfile struct {
	SchemaVersion             int                      `json:"schema_version"`
	ContextWindow             ModelTokenLimit          `json:"context_window"`
	MaxOutput                 ModelTokenLimit          `json:"max_output"`
	SelectedOutputReservation ModelTokenLimit          `json:"selected_output_reservation"`
	Estimator                 ModelTokenEstimatePolicy `json:"estimator"`
	Framing                   ModelFramingGuards       `json:"framing"`
	Sources                   ModelBudgetSources       `json:"sources"`
}

func ResolveModelBudgetProfile(override *ModelBudgetOverride) ModelBudgetProfile {
	configuredInput := cloneModelBudgetOverride(override)
	requestedInput := LegacyAgentInputBytes
	if configuredInput != nil && configuredInput.MaxInputJSONBytes != nil {
		requestedInput = *configuredInput.MaxInputJSONBytes
	}
	maxInput := min(requestedInput, LegacyAgentInputBytes, GatewayRequestBytes)

	profile := ModelBudgetProfile{
		SchemaVersion:             ModelBudgetProfileVersion,
		ContextWindow:             unknownTokenLimit(),
		MaxOutput:                 unknownTokenLimit(),
		SelectedOutputReservation: unknownTokenLimit(),
		Estimator: ModelTokenEstimatePolicy{
			Version: ModelBudgetProfileVersion, Method: modelInputEstimateMethod, Uncertainty: modelInputEstimateUncertainty,
		},
		Framing: ModelFramingGuards{
			MaxInputJSONBytes: maxInput, MaxInstructionBytes: GatewayInstructionBytes,
			MaxMessages: GatewayMessageCount, MaxTools: GatewayToolCount,
			MaxConversationBytes: AgentConversationBytes, MaxRequestBytes: GatewayRequestBytes,
			MaxOutputBytes: GatewayOutputBytes,
		},
		Sources: ModelBudgetSources{
			Catalog:               CatalogBudgetFacts{Status: CatalogUnavailable},
			OperatorConfiguration: BudgetOverrideUnconfigured,
			ProviderConfirmed:     LimitUnknown,
		},
	}
	if configuredInput == nil {
		return profile
	}
	profile.Sources.OperatorConfiguration = BudgetOverrideConfigured
	version := configuredInput.SchemaVersion
	profile.Sources.OperatorConfigurationVersion = &version
	profile.Framing.ConfiguredInputJSONBytes = configuredInput.MaxInputJSONBytes
	profile.ContextWindow = operatorTokenLimit(configuredInput.ContextWindowTokens)
	profile.MaxOutput = operatorTokenLimit(configuredInput.MaxOutputTokens)
	profile.SelectedOutputReservation = operatorTokenLimit(configuredInput.SelectedOutputReservationTokens)
	profile.Estimator.ProviderOverheadTokens = configuredInput.ProviderOverheadTokens
	profile.Estimator.SafetyMarginTokens = configuredInput.SafetyMarginTokens
	return profile
}

func unknownTokenLimit() ModelTokenLimit {
	return ModelTokenLimit{Status: LimitUnknown, Source: LimitSourceUnknown}
}

func operatorTokenLimit(tokens *uint32) ModelTokenLimit {
	if tokens == nil {
		return unknownTokenLimit()
	}
	copy := *tokens
	return ModelTokenLimit{Status: LimitKnown, Tokens: &copy, Source: LimitSourceOperator}
}

func (p ModelBudgetProfile) hasContextEstimatePolicy() bool {
	return p.ContextWindow.Status == LimitKnown && p.ContextWindow.Tokens != nil &&
		p.SelectedOutputReservation.Status == LimitKnown && p.SelectedOutputReservation.Tokens != nil &&
		p.Estimator.ProviderOverheadTokens != nil && p.Estimator.SafetyMarginTokens != nil
}

func (p ModelBudgetProfile) Validate() error {
	if p.SchemaVersion != ModelBudgetProfileVersion || p.Estimator.Version != ModelBudgetProfileVersion ||
		p.Estimator.Method != modelInputEstimateMethod || p.Estimator.Uncertainty != modelInputEstimateUncertainty ||
		p.Framing.MaxInputJSONBytes == 0 || p.Framing.MaxInputJSONBytes > LegacyAgentInputBytes ||
		p.Framing.MaxInstructionBytes != GatewayInstructionBytes || p.Framing.MaxMessages != GatewayMessageCount ||
		p.Framing.MaxTools != GatewayToolCount || p.Framing.MaxConversationBytes != AgentConversationBytes ||
		p.Framing.MaxRequestBytes != GatewayRequestBytes || p.Framing.MaxOutputBytes != GatewayOutputBytes {
		return errors.New("invalid resolved model budget profile")
	}
	validateLimit := func(limit ModelTokenLimit) bool {
		switch limit.Status {
		case LimitUnknown:
			return limit.Tokens == nil && limit.Source == LimitSourceUnknown
		case LimitKnown:
			return limit.Tokens != nil && *limit.Tokens > 0 && *limit.Tokens <= MaximumKnownModelTokens &&
				(limit.Source == LimitSourceOperator || limit.Source == LimitSourceProvider)
		default:
			return false
		}
	}
	if !validateLimit(p.ContextWindow) || !validateLimit(p.MaxOutput) || !validateLimit(p.SelectedOutputReservation) {
		return errors.New("invalid model token limit source")
	}
	if p.ContextWindow.Tokens != nil && p.MaxOutput.Tokens != nil && *p.MaxOutput.Tokens > *p.ContextWindow.Tokens ||
		p.SelectedOutputReservation.Tokens != nil && p.MaxOutput.Tokens != nil && *p.SelectedOutputReservation.Tokens > *p.MaxOutput.Tokens ||
		p.SelectedOutputReservation.Tokens != nil && p.ContextWindow.Tokens != nil && *p.SelectedOutputReservation.Tokens > *p.ContextWindow.Tokens {
		return errors.New("inconsistent resolved model budget limits")
	}
	if p.Estimator.ProviderOverheadTokens != nil && (*p.Estimator.ProviderOverheadTokens == 0 || *p.Estimator.ProviderOverheadTokens > MaximumKnownModelTokens) ||
		p.Estimator.SafetyMarginTokens != nil && (*p.Estimator.SafetyMarginTokens == 0 || *p.Estimator.SafetyMarginTokens > MaximumKnownModelTokens) {
		return errors.New("invalid model estimate policy")
	}
	if p.Framing.ConfiguredInputJSONBytes != nil && (*p.Framing.ConfiguredInputJSONBytes == 0 || *p.Framing.ConfiguredInputJSONBytes > GatewayRequestBytes) {
		return errors.New("invalid configured model input limit")
	}
	requestedInput := LegacyAgentInputBytes
	if p.Framing.ConfiguredInputJSONBytes != nil {
		requestedInput = *p.Framing.ConfiguredInputJSONBytes
	}
	if p.Framing.MaxInputJSONBytes != min(requestedInput, LegacyAgentInputBytes, GatewayRequestBytes) {
		return errors.New("inconsistent effective model input limit")
	}
	if p.Sources.OperatorConfiguration != BudgetOverrideConfigured && p.Sources.OperatorConfiguration != BudgetOverrideUnconfigured ||
		p.Sources.ProviderConfirmed != ProviderConfirmedUnknown && p.Sources.ProviderConfirmed != ProviderConfirmedAvailable {
		return errors.New("invalid model budget source status")
	}
	hasProviderConfirmedLimit := p.ContextWindow.Source == LimitSourceProvider || p.MaxOutput.Source == LimitSourceProvider || p.SelectedOutputReservation.Source == LimitSourceProvider
	if (p.Sources.ProviderConfirmed == ProviderConfirmedAvailable) != hasProviderConfirmedLimit {
		return errors.New("provider-confirmed limit status does not match profile facts")
	}
	if p.Sources.OperatorConfiguration == BudgetOverrideConfigured &&
		(p.Sources.OperatorConfigurationVersion == nil || *p.Sources.OperatorConfigurationVersion != ModelBudgetOverrideVersion) ||
		p.Sources.OperatorConfiguration == BudgetOverrideUnconfigured && p.Sources.OperatorConfigurationVersion != nil {
		return errors.New("invalid model budget configuration version")
	}
	if p.Sources.OperatorConfiguration == BudgetOverrideUnconfigured &&
		(p.ContextWindow.Source == LimitSourceOperator || p.MaxOutput.Source == LimitSourceOperator || p.SelectedOutputReservation.Source == LimitSourceOperator ||
			p.Estimator.ProviderOverheadTokens != nil || p.Estimator.SafetyMarginTokens != nil || p.Framing.ConfiguredInputJSONBytes != nil) {
		return errors.New("unconfigured model profile contains effective limits")
	}
	if !validCatalogBudgetFacts(p.Sources.Catalog) {
		return errors.New("invalid model catalog budget facts")
	}
	return nil
}

func validCatalogBudgetFacts(facts CatalogBudgetFacts) bool {
	if facts.Revision != nil && *facts.Revision == 0 ||
		facts.ContextWindowTokens != nil && (*facts.ContextWindowTokens == 0 || *facts.ContextWindowTokens > MaximumKnownModelTokens) ||
		facts.MaxOutputTokens != nil && (*facts.MaxOutputTokens == 0 || *facts.MaxOutputTokens > MaximumKnownModelTokens) ||
		facts.ContextWindowTokens != nil && facts.MaxOutputTokens != nil && *facts.MaxOutputTokens > *facts.ContextWindowTokens {
		return false
	}
	if facts.Provenance != nil && (facts.Provenance.Source == "" || facts.Provenance.VerifiedAt == "") {
		return false
	}
	switch facts.Status {
	case CatalogUnavailable:
		return facts.Revision == nil && facts.ContextWindowTokens == nil && facts.MaxOutputTokens == nil && facts.Provenance == nil
	case CatalogModelNotListed:
		return facts.Revision != nil && facts.ContextWindowTokens == nil && facts.MaxOutputTokens == nil && facts.Provenance == nil
	case CatalogMetadataUnknown:
		return facts.Revision != nil && facts.ContextWindowTokens == nil && facts.MaxOutputTokens == nil
	case CatalogMetadataPresent:
		return facts.Revision != nil && facts.Provenance != nil &&
			(facts.ContextWindowTokens != nil || facts.MaxOutputTokens != nil) &&
			facts.Provenance.Source != "" && facts.Provenance.VerifiedAt != ""
	default:
		return false
	}
}

func (p ModelBudgetProfile) effectiveRevisionMaterial() any {
	return struct {
		ContextWindow             ModelTokenLimit
		MaxOutput                 ModelTokenLimit
		SelectedOutputReservation ModelTokenLimit
		Estimator                 ModelTokenEstimatePolicy
		Framing                   ModelFramingGuards
	}{p.ContextWindow, p.MaxOutput, p.SelectedOutputReservation, p.Estimator, p.Framing}
}

type modelInputEstimate struct {
	Instructions string       `json:"instructions"`
	Input        AgentInput   `json:"input"`
	OutputFormat OutputFormat `json:"output_format"`
}

type structuredModelInputEstimate struct {
	Instructions string          `json:"instructions"`
	Input        string          `json:"input"`
	OutputSchema json.RawMessage `json:"output_schema"`
}

// ValidateModelBudgetInput checks profile framing and, only when every input
// is known, a conservative context estimate. The estimate covers only model
// input fields and excludes Gateway request IDs, capability metadata, bearer
// credentials and the HTTP envelope.
func ValidateModelBudgetInput(profile ModelBudgetProfile, invocation AgentInvocation) error {
	if len(invocation.Input.Messages) > int(profile.Framing.MaxMessages) || len(invocation.Input.Tools) > int(profile.Framing.MaxTools) {
		return Failure{Code: BodyTooLarge}
	}
	inputBytes, err := json.Marshal(invocation.Input)
	if err != nil {
		return Failure{Code: Validation}
	}
	if uint64(len(inputBytes)) > uint64(profile.Framing.MaxInputJSONBytes) {
		return Failure{Code: BodyTooLarge}
	}
	if uint64(len(invocation.Instructions)) > uint64(profile.Framing.MaxInstructionBytes) {
		return Failure{Code: BodyTooLarge}
	}
	if !profile.hasContextEstimatePolicy() {
		return nil
	}
	modelInput, err := json.Marshal(modelInputEstimate{
		Instructions: invocation.Instructions,
		Input:        invocation.Input,
		OutputFormat: invocation.OutputFormat,
	})
	if err != nil {
		return Failure{Code: Validation}
	}
	needed := uint64(len(modelInput))
	for _, extra := range []*uint32{profile.Estimator.ProviderOverheadTokens, profile.Estimator.SafetyMarginTokens, profile.SelectedOutputReservation.Tokens} {
		if uint64(*extra) > ^uint64(0)-needed {
			return Failure{Code: BodyTooLarge}
		}
		needed += uint64(*extra)
	}
	if needed > uint64(*profile.ContextWindow.Tokens) {
		return Failure{Code: BodyTooLarge}
	}
	return nil
}

// ValidateStructuredModelBudgetInput applies the same selected profile to the
// operator-only structured endpoint. Existing validation keeps its 32 KiB
// input/schema caps and 8 KiB instruction cap; profile limits may tighten them.
func ValidateStructuredModelBudgetInput(profile ModelBudgetProfile, invocation StructuredInvocation) error {
	if uint64(len(invocation.Input)) > uint64(profile.Framing.MaxInputJSONBytes) {
		return Failure{Code: BodyTooLarge}
	}
	if uint64(len(invocation.Instructions)) > 8192 || uint64(len(invocation.Instructions)) > uint64(profile.Framing.MaxInstructionBytes) {
		return Failure{Code: BodyTooLarge}
	}
	if !profile.hasContextEstimatePolicy() {
		return nil
	}
	modelInput, err := json.Marshal(structuredModelInputEstimate{
		Instructions: invocation.Instructions,
		Input:        string(invocation.Input),
		OutputSchema: invocation.OutputSchema,
	})
	if err != nil {
		return Failure{Code: Validation}
	}
	needed := uint64(len(modelInput))
	for _, extra := range []*uint32{profile.Estimator.ProviderOverheadTokens, profile.Estimator.SafetyMarginTokens, profile.SelectedOutputReservation.Tokens} {
		if uint64(*extra) > ^uint64(0)-needed {
			return Failure{Code: BodyTooLarge}
		}
		needed += uint64(*extra)
	}
	if needed > uint64(*profile.ContextWindow.Tokens) {
		return Failure{Code: BodyTooLarge}
	}
	return nil
}
