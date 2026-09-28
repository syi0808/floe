package connections

import (
	"fmt"
	"reflect"
	"strings"
	"testing"
)

func TestCalendarScopeCanonicalResourceSet(t *testing.T) {
	for _, connectorID := range []string{"calendar.google", "calendar.microsoft"} {
		definition, ok := DefinitionFor(connectorID)
		if !ok {
			t.Fatal(connectorID)
		}
		many := make([]any, 11)
		for index := range many {
			many[index] = fmt.Sprintf("calendar-%02d", 10-index)
		}
		validated, err := ValidatedConnectorScope(definition, map[string]any{"calendar_ids": many})
		if err != nil {
			t.Fatal(err)
		}
		calendarIDs := validated["calendar_ids"].([]string)
		if len(calendarIDs) != 11 || calendarIDs[0] != "calendar-00" || calendarIDs[10] != "calendar-10" {
			t.Fatalf("not canonical: %v", calendarIDs)
		}
		for _, invalid := range []map[string]any{
			{"calendar_id": "calendar-a"},
			{"calendar_ids": []any{}},
			{"calendar_ids": []any{"a", "a"}},
			{"calendar_ids": []any{" a"}},
			{"calendar_ids": []any{"a\nb"}},
			{"calendar_ids": []any{"*"}},
			{"calendar_ids": []any{strings.Repeat("a", 257)}},
		} {
			if _, err := ValidatedConnectorScope(definition, invalid); err == nil {
				t.Fatalf("accepted invalid scope: %v", invalid)
			}
		}
		oversized := make([]any, 80)
		for index := range oversized {
			oversized[index] = fmt.Sprintf("%03d-%s", index, strings.Repeat("a", 196))
		}
		if _, err := ValidatedConnectorScope(definition, map[string]any{"calendar_ids": oversized}); err == nil {
			t.Fatal("accepted source set beyond proof budget")
		}
		withComma, err := ValidatedConnectorScope(definition, map[string]any{"calendar_ids": []any{"opaque,id", "other"}})
		if err != nil || !reflect.DeepEqual(withComma["calendar_ids"], []string{"opaque,id", "other"}) {
			t.Fatalf("opaque comma ID not preserved: %v %v", withComma, err)
		}
	}
}
