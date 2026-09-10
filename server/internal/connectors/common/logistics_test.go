package common

import "testing"

func TestLogisticsMergeRejectsDuplicatesAndUsesEarliestExpiry(test *testing.T) {
	view := func(source, evidence string, expires int64) LogisticsView {
		return LogisticsView{SchemaVersion: 1, ViewID: "life.logistics", SourceHandle: source, ObservedAtUnixMS: 1000, ExpiresAtUnixMS: expires, CoverageComplete: true, Items: []LogisticsItem{{EvidenceHandle: evidence, Kind: "delivery", Summary: "Package update", Status: "mail_candidate"}}}
	}
	merged, err := MergeLogisticsViews([]LogisticsView{view("mail:a", "mail:item", 1900), view("home:b", "home:item", 2000)}, 1500)
	if err != nil || len(merged.Items) != 2 || merged.ExpiresAtUnixMS != 1900 {
		test.Fatalf("merge: %#v %v", merged, err)
	}
	if _, err := MergeLogisticsViews([]LogisticsView{view("mail:a", "same", 1900), view("home:b", "same", 2000)}, 1500); err == nil {
		test.Fatal("duplicate evidence accepted")
	}
}
