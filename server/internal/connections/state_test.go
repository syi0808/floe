package connections

import "testing"

func TestRegistryAttemptSnapshotsDoNotMutateOwnerState(test *testing.T) {
	registry := NewRegistry()
	attempt := &Attempt{ID: "attempt", Status: "pending", Scope: map[string]any{"entities": []string{"first"}}}
	registry.PutAttempt(attempt.ID, attempt)
	attempt.Status = "connected"
	attempt.Scope["entities"].([]string)[0] = "changed"
	observed := registry.GetAttempt(attempt.ID)
	if observed.Status != "pending" || observed.Scope["entities"].([]string)[0] != "first" {
		test.Fatal("input mutation crossed registry ownership")
	}
	observed.Status = "failed"
	registry.Attempts()[attempt.ID].Scope["entities"].([]string)[0] = "changed"
	if registry.GetAttempt(attempt.ID).Status != "pending" || registry.GetAttempt(attempt.ID).Scope["entities"].([]string)[0] != "first" {
		test.Fatal("observation mutated registry state")
	}
	registry.DeleteAttempt(attempt.ID)
	if registry.GetAttempt(attempt.ID) != nil || len(registry.Attempts()) != 0 {
		test.Fatal("deleted attempt remained observable")
	}
}
