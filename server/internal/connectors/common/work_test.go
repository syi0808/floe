package common

import "testing"

func TestWorkContextMergeIsDeterministicBoundedAndSourceLinked(test *testing.T) {
	view := func(source, scope, evidence string) WorkContextView {
		return WorkContextView{SchemaVersion: 1, ViewID: "work.context", SourceHandle: source, ScopeHandle: scope, ObservedAtUnixMS: 1000, ExpiresAtUnixMS: 2000, CoverageComplete: true, Items: []WorkItem{{EvidenceHandle: evidence, Kind: "project", Title: "Selected work", ObservedAtUnixMS: 999}}}
	}
	left, err := MergeWorkContextViews([]WorkContextView{view("github:a", "workspace:a", "github:item"), view("slack:b", "channel:b", "slack:item")}, 1500)
	if err != nil {
		test.Fatal(err)
	}
	right, err := MergeWorkContextViews([]WorkContextView{view("slack:b", "channel:b", "slack:item"), view("github:a", "workspace:a", "github:item")}, 1500)
	if err != nil || left.SourceHandle != right.SourceHandle || left.ScopeHandle != right.ScopeHandle || len(left.Items) != 2 {
		test.Fatalf("merge: %#v %#v %v", left, right, err)
	}
	if _, err := MergeWorkContextViews([]WorkContextView{view("github:a", "workspace:a", "same"), view("slack:b", "channel:b", "same")}, 1500); err == nil {
		test.Fatal("duplicate evidence accepted")
	}
}
