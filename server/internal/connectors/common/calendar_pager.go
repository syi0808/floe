package common

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"
)

const maxCalendarLeavesPerRead = 16

var ErrCalendarCursor = errors.New("invalid calendar cursor")
var ErrCalendarResult = errors.New("invalid calendar result")

type CalendarLeaf struct {
	ResourceID string
	Read       func(context.Context, time.Time, time.Time, string, int, time.Time) (CalendarView, error)
}

type CalendarPager struct {
	leaves       []CalendarLeaf
	digest       string
	sourceHandle string
}

type calendarCursor struct {
	Version           int    `json:"version"`
	ResourceSetDigest string `json:"resource_set_digest"`
	ResourceIndex     int    `json:"resource_index"`
	ProviderCursor    string `json:"provider_cursor"`
}

func NewCalendarPager(connectionID string, leaves []CalendarLeaf) (*CalendarPager, error) {
	if connectionID == "" || len(leaves) == 0 {
		return nil, ErrCalendarCursor
	}
	resourceIDs := make([]string, len(leaves))
	for index, leaf := range leaves {
		if leaf.ResourceID == "" || leaf.Read == nil || index > 0 && leaves[index-1].ResourceID >= leaf.ResourceID {
			return nil, ErrCalendarCursor
		}
		resourceIDs[index] = leaf.ResourceID
	}
	encoded, err := json.Marshal(resourceIDs)
	if err != nil {
		return nil, ErrCalendarCursor
	}
	digest := sha256.Sum256(encoded)
	sourceDigest := sha256.Sum256([]byte(connectionID + ":" + hex.EncodeToString(digest[:])))
	return &CalendarPager{leaves: append([]CalendarLeaf(nil), leaves...), digest: hex.EncodeToString(digest[:]), sourceHandle: fmt.Sprintf("calendar:%x", sourceDigest)}, nil
}

func (pager *CalendarPager) encodeCursor(index int, providerCursor string) (string, error) {
	encoded, err := json.Marshal(calendarCursor{Version: 1, ResourceSetDigest: pager.digest, ResourceIndex: index, ProviderCursor: providerCursor})
	if err != nil {
		return "", ErrCalendarResult
	}
	cursor := base64.RawURLEncoding.EncodeToString(encoded)
	if len(cursor) > 2048 {
		return "", ErrCalendarResult
	}
	return cursor, nil
}

func (pager *CalendarPager) decodeCursor(encoded string) (int, string, error) {
	if encoded == "" {
		return 0, "", nil
	}
	if len(encoded) > 2048 {
		return 0, "", ErrCalendarCursor
	}
	decoded, err := base64.RawURLEncoding.DecodeString(encoded)
	if err != nil || base64.RawURLEncoding.EncodeToString(decoded) != encoded {
		return 0, "", ErrCalendarCursor
	}
	var cursor calendarCursor
	if json.Unmarshal(decoded, &cursor) != nil {
		return 0, "", ErrCalendarCursor
	}
	canonical, err := json.Marshal(cursor)
	if err != nil || !bytes.Equal(canonical, decoded) {
		return 0, "", ErrCalendarCursor
	}
	if cursor.Version != 1 || cursor.ResourceSetDigest != pager.digest || cursor.ResourceIndex < 0 || cursor.ResourceIndex >= len(pager.leaves) || len(cursor.ProviderCursor) > 2048 || strings.ContainsAny(cursor.ProviderCursor, "\r\n\x00") {
		return 0, "", ErrCalendarCursor
	}
	return cursor.ResourceIndex, cursor.ProviderCursor, nil
}

func (pager *CalendarPager) Read(ctx context.Context, rangeStart, rangeEnd time.Time, encodedCursor string, limit int, now time.Time) (CalendarView, error) {
	if rangeStart.IsZero() || rangeEnd.IsZero() || !rangeStart.Before(rangeEnd) || rangeEnd.Sub(rangeStart) > 32*24*time.Hour || rangeStart.UnixMilli() < 0 || limit < 1 || limit > 128 {
		return CalendarView{}, ErrCalendarCursor
	}
	index, providerCursor, err := pager.decodeCursor(encodedCursor)
	if err != nil {
		return CalendarView{}, err
	}
	result := CalendarView{SchemaVersion: 1, ViewID: "calendar.timeline", SourceHandle: pager.sourceHandle, ObservedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: now.Add(5 * time.Minute).UnixMilli(), RangeStartUnixMS: rangeStart.UnixMilli(), RangeEndUnixMS: rangeEnd.UnixMilli(), Items: []CalendarItem{}}
	seen := map[string]bool{}
	reads := 0
	for index < len(pager.leaves) {
		if reads == maxCalendarLeavesPerRead {
			break
		}
		remaining := limit - len(result.Items)
		if remaining == 0 {
			break
		}
		leaf, readErr := pager.leaves[index].Read(ctx, rangeStart, rangeEnd, providerCursor, remaining, now)
		if readErr != nil {
			return CalendarView{}, readErr
		}
		reads++
		if leaf.SchemaVersion != 1 || leaf.ViewID != result.ViewID || leaf.RangeStartUnixMS != result.RangeStartUnixMS || leaf.RangeEndUnixMS != result.RangeEndUnixMS || leaf.ObservedAtUnixMS > now.UnixMilli() || leaf.ExpiresAtUnixMS <= now.UnixMilli() || len(leaf.Items) > remaining || leaf.CoverageComplete == (leaf.NextCursor != nil) {
			return CalendarView{}, ErrCalendarResult
		}
		for _, item := range leaf.Items {
			if item.EvidenceHandle == "" || seen[item.EvidenceHandle] {
				return CalendarView{}, ErrCalendarResult
			}
			seen[item.EvidenceHandle] = true
			result.Items = append(result.Items, item)
		}
		if leaf.ObservedAtUnixMS > result.ObservedAtUnixMS {
			result.ObservedAtUnixMS = leaf.ObservedAtUnixMS
		}
		if leaf.ExpiresAtUnixMS < result.ExpiresAtUnixMS {
			result.ExpiresAtUnixMS = leaf.ExpiresAtUnixMS
		}
		if leaf.NextCursor != nil {
			if *leaf.NextCursor == "" || len(*leaf.NextCursor) > 2048 || strings.ContainsAny(*leaf.NextCursor, "\r\n\x00") {
				return CalendarView{}, ErrCalendarResult
			}
			providerCursor = *leaf.NextCursor
			break
		}
		index++
		providerCursor = ""
	}
	result.CoverageComplete = index == len(pager.leaves)
	if !result.CoverageComplete {
		nextCursor, cursorErr := pager.encodeCursor(index, providerCursor)
		if cursorErr != nil {
			return CalendarView{}, cursorErr
		}
		result.NextCursor = &nextCursor
	}
	encoded, err := json.Marshal(result)
	if err != nil || len(encoded) > 65_536 {
		return CalendarView{}, ErrCalendarResult
	}
	return result, nil
}
