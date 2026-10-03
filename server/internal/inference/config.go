package inference

import "errors"

type InferenceConfig struct {
	Routes map[Purpose]PurposeRoute `json:"routes"`
}
type PurposeRoute struct {
	TargetID        string `json:"target_id"`
	ReasoningEffort string `json:"reasoning_effort"`
	Enabled         bool   `json:"enabled"`
}

func ValidateConfig(config InferenceConfig) error {
	if config.Routes == nil || len(config.Routes) > 3 {
		return errors.New("invalid inference configuration")
	}
	for p, r := range config.Routes {
		if !ValidPurpose(string(p)) || !ValidAlias(r.TargetID) || !ValidEffort(r.ReasoningEffort) {
			return errors.New("invalid inference route")
		}
	}
	return nil
}
func ValidEffort(value string) bool {
	return value == "" || value == "low" || value == "medium" || value == "high" || value == "xhigh" || value == "max" || value == "ultra"
}
