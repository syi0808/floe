package inference

type PurposeModel struct {
	Model           string               `json:"model"`
	ReasoningEffort string               `json:"reasoning_effort,omitempty"`
	BudgetOverride  *ModelBudgetOverride `json:"budget_override,omitempty"`
}
