package authorization

import (
	"encoding/json"
	"testing"
)

func TestCalendarAuthorityRejectsCaseAliasesAndNestedUnknownFields(t *testing.T) {
	valid := []byte(`{"schema_version":1,"query":{"range_start_unix_ms":1,"range_end_unix_ms":2,"cursor":"","limit":1}}`)
	if !ValidateCalendarCaseExact(valid) {
		t.Fatal("valid calendar envelope rejected")
	}
	for _, encoded := range [][]byte{
		[]byte(`{"SCHEMA_VERSION":1}`),
		[]byte(`{"schema_version":1,"query":{"LIMIT":1,"range_start_unix_ms":1,"range_end_unix_ms":2,"cursor":""}}`),
	} {
		if ValidateCalendarCaseExact(encoded) {
			t.Fatalf("case alias accepted: %s", encoded)
		}
	}
	if validateCalendarObject([]byte(`{"range_start_unix_ms":1,"range_end_unix_ms":2,"cursor":"","limit":1,"unknown":true}`), map[string]struct{}{"range_start_unix_ms": {}, "range_end_unix_ms": {}, "cursor": {}, "limit": {}}) == nil {
		t.Fatal("nested unknown field accepted")
	}
}
func TestBoundedCalendarViewCountsOnlyValidatedItems(t *testing.T) {
	result, itemCount, err := boundedCalendarView(map[string]any{
		"items": []any{map[string]any{"id": "one"}, map[string]any{"id": "two"}},
	})
	if err != nil || itemCount != 2 || !json.Valid(result) {
		t.Fatalf("unexpected bounded view result: items=%d err=%v", itemCount, err)
	}
	if _, _, err := boundedCalendarView(map[string]any{"items": "not-an-array"}); err == nil {
		t.Fatal("invalid item collection accepted")
	}
}
