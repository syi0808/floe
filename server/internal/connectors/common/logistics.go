package common

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"sort"
	"strings"
)

var ErrInvalidLogistics = errors.New("invalid logistics context")

type LogisticsItem struct {
	EvidenceHandle string `json:"evidence_handle"`
	Kind           string `json:"kind"`
	Summary        string `json:"summary"`
	Status         string `json:"status"`
	OccursAtUnixMS *int64 `json:"occurs_at_unix_ms,omitempty"`
	NeedsAttention bool   `json:"needs_attention"`
}

type LogisticsView struct {
	SchemaVersion    int             `json:"schema_version"`
	ViewID           string          `json:"view_id"`
	SourceHandle     string          `json:"source_handle"`
	ObservedAtUnixMS int64           `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS  int64           `json:"expires_at_unix_ms"`
	CoverageComplete bool            `json:"coverage_complete"`
	Items            []LogisticsItem `json:"items"`
}

func MergeLogisticsViews(views []LogisticsView, nowUnixMS int64) (LogisticsView, error) {
	if len(views) == 0 || len(views) > 8 {
		return LogisticsView{}, ErrInvalidLogistics
	}
	merged := LogisticsView{SchemaVersion: 1, ViewID: "life.logistics", ExpiresAtUnixMS: views[0].ExpiresAtUnixMS, CoverageComplete: true, Items: []LogisticsItem{}}
	sources := make([]string, 0, len(views))
	seen := map[string]bool{}
	for _, view := range views {
		if view.SchemaVersion != 1 || view.ViewID != "life.logistics" || !validLogisticsText(view.SourceHandle, 256) || view.ObservedAtUnixMS > nowUnixMS || view.ExpiresAtUnixMS <= nowUnixMS || view.ExpiresAtUnixMS-view.ObservedAtUnixMS > 300_000 {
			return LogisticsView{}, ErrInvalidLogistics
		}
		if view.ObservedAtUnixMS > merged.ObservedAtUnixMS {
			merged.ObservedAtUnixMS = view.ObservedAtUnixMS
		}
		if view.ExpiresAtUnixMS < merged.ExpiresAtUnixMS {
			merged.ExpiresAtUnixMS = view.ExpiresAtUnixMS
		}
		merged.CoverageComplete = merged.CoverageComplete && view.CoverageComplete
		sources = append(sources, view.SourceHandle)
		for _, item := range view.Items {
			if seen[item.EvidenceHandle] || !validLogisticsItem(item) {
				return LogisticsView{}, ErrInvalidLogistics
			}
			seen[item.EvidenceHandle] = true
			merged.Items = append(merged.Items, item)
		}
	}
	if len(merged.Items) > 64 {
		return LogisticsView{}, ErrInvalidLogistics
	}
	sort.Strings(sources)
	merged.SourceHandle = logisticsHandle("logistics", strings.Join(sources, "\x00"))
	encoded, err := json.Marshal(merged)
	if err != nil || len(encoded) > 65_536 {
		return LogisticsView{}, ErrInvalidLogistics
	}
	return merged, nil
}

func validLogisticsItem(item LogisticsItem) bool {
	validKind := item.Kind == "reservation" || item.Kind == "travel" || item.Kind == "delivery" || item.Kind == "errand" || item.Kind == "home_state"
	return validKind && validLogisticsText(item.EvidenceHandle, 256) && validLogisticsText(item.Summary, 512) && validLogisticsText(item.Status, 128) && (item.OccursAtUnixMS == nil || *item.OccursAtUnixMS >= 0)
}

func validLogisticsText(value string, maximum int) bool {
	return strings.TrimSpace(value) != "" && len(value) <= maximum
}

func logisticsHandle(namespace, value string) string {
	digest := sha256.Sum256([]byte(namespace + "\x00" + value))
	return namespace + ":" + hex.EncodeToString(digest[:])
}
