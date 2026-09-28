package common

import (
	"context"
	"errors"
	"fmt"
	"testing"
	"time"
)

func TestCalendarPagerWalksProviderCursorAndResourceBoundary(t *testing.T) {
	now := time.UnixMilli(1_000_000)
	start, end := time.UnixMilli(1_000), time.UnixMilli(2_000)
	calls := []string{}
	leaf := func(resourceID string) CalendarLeaf {
		return CalendarLeaf{ResourceID: resourceID, Read: func(_ context.Context, _, _ time.Time, cursor string, limit int, _ time.Time) (CalendarView, error) {
			calls = append(calls, fmt.Sprintf("%s:%s:%d", resourceID, cursor, limit))
			view := CalendarView{SchemaVersion: 1, ViewID: "calendar.timeline", SourceHandle: "leaf-opaque", ObservedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: now.Add(time.Minute).UnixMilli(), RangeStartUnixMS: start.UnixMilli(), RangeEndUnixMS: end.UnixMilli(), CoverageComplete: true, Items: []CalendarItem{{EvidenceHandle: "opaque-" + resourceID + cursor, UntrustedTitle: "Meeting", StartsAtUnixMS: 1_200, EndsAtUnixMS: 1_300}}}
			if resourceID == "A" && cursor == "" {
				next := "provider-next"
				view.NextCursor = &next
				view.CoverageComplete = false
			}
			return view, nil
		}}
	}
	pager, err := NewCalendarPager("connection", []CalendarLeaf{leaf("A"), leaf("B")})
	if err != nil {
		t.Fatal(err)
	}
	first, err := pager.Read(context.Background(), start, end, "", 3, now)
	if err != nil || first.CoverageComplete || first.NextCursor == nil || len(first.Items) != 1 {
		t.Fatalf("first page: %+v %v", first, err)
	}
	index, providerCursor, err := pager.decodeCursor(*first.NextCursor)
	if err != nil || index != 0 || providerCursor != "provider-next" {
		t.Fatalf("composite provider cursor: %d %q %v", index, providerCursor, err)
	}
	second, err := pager.Read(context.Background(), start, end, *first.NextCursor, 3, now)
	if err != nil || !second.CoverageComplete || second.NextCursor != nil || len(second.Items) != 2 {
		t.Fatalf("second page: %+v %v", second, err)
	}
	if fmt.Sprint(calls) != "[A::3 A:provider-next:3 B::2]" {
		t.Fatalf("wrong resource traversal: %v", calls)
	}
	boundary, err := pager.Read(context.Background(), start, end, *first.NextCursor, 1, now)
	if err != nil || boundary.NextCursor == nil {
		t.Fatalf("boundary page: %+v %v", boundary, err)
	}
	index, providerCursor, err = pager.decodeCursor(*boundary.NextCursor)
	if err != nil || index != 1 || providerCursor != "" {
		t.Fatalf("next resource cursor: %d %q %v", index, providerCursor, err)
	}
	changed, err := NewCalendarPager("connection", []CalendarLeaf{leaf("A"), leaf("B"), leaf("C")})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := changed.Read(context.Background(), start, end, *first.NextCursor, 3, now); !errors.Is(err, ErrCalendarCursor) {
		t.Fatalf("stale resource-set cursor accepted: %v", err)
	}
}

func TestCalendarPagerReadsElevenResourcesWithoutPermissionCountCap(t *testing.T) {
	now := time.UnixMilli(1_000_000)
	start, end := time.UnixMilli(1_000), time.UnixMilli(2_000)
	leaves := make([]CalendarLeaf, 11)
	for index := range leaves {
		resourceID := fmt.Sprintf("calendar-%02d", index)
		leaves[index] = CalendarLeaf{ResourceID: resourceID, Read: func(context.Context, time.Time, time.Time, string, int, time.Time) (CalendarView, error) {
			return CalendarView{SchemaVersion: 1, ViewID: "calendar.timeline", SourceHandle: "opaque", ObservedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: now.Add(time.Minute).UnixMilli(), RangeStartUnixMS: start.UnixMilli(), RangeEndUnixMS: end.UnixMilli(), CoverageComplete: true, Items: []CalendarItem{{EvidenceHandle: "opaque-" + resourceID, UntrustedTitle: "Meeting", StartsAtUnixMS: 1_200, EndsAtUnixMS: 1_300}}}, nil
		}}
	}
	pager, err := NewCalendarPager("connection", leaves)
	if err != nil {
		t.Fatal(err)
	}
	view, err := pager.Read(context.Background(), start, end, "", 11, now)
	if err != nil || !view.CoverageComplete || len(view.Items) != 11 || view.NextCursor != nil {
		t.Fatalf("eleven-resource read: %+v %v", view, err)
	}
}
