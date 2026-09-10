package common

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"sort"
	"strings"
)

var ErrInvalidWorkContext = errors.New("invalid work context")

type WorkItem struct {
	EvidenceHandle   string  `json:"evidence_handle"`
	Kind             string  `json:"kind"`
	Title            string  `json:"title"`
	Excerpt          *string `json:"excerpt,omitempty"`
	Status           *string `json:"status,omitempty"`
	Blocker          *string `json:"blocker,omitempty"`
	NextAction       *string `json:"next_action,omitempty"`
	ObservedAtUnixMS int64   `json:"observed_at_unix_ms"`
}

type WorkContextView struct {
	SchemaVersion    int        `json:"schema_version"`
	ViewID           string     `json:"view_id"`
	SourceHandle     string     `json:"source_handle"`
	ObservedAtUnixMS int64      `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS  int64      `json:"expires_at_unix_ms"`
	CoverageComplete bool       `json:"coverage_complete"`
	ScopeHandle      string     `json:"scope_handle"`
	Items            []WorkItem `json:"items"`
}

func MergeWorkContextViews(views []WorkContextView, nowUnixMS int64) (WorkContextView, error) {
	if len(views) == 0 || len(views) > 8 {
		return WorkContextView{}, ErrInvalidWorkContext
	}
	merged := WorkContextView{
		SchemaVersion:    1,
		ViewID:           "work.context",
		ExpiresAtUnixMS:  views[0].ExpiresAtUnixMS,
		CoverageComplete: true,
		Items:            []WorkItem{},
	}
	sources, scopes := make([]string, 0, len(views)), make([]string, 0, len(views))
	seen := map[string]bool{}
	for _, view := range views {
		if view.SchemaVersion != 1 || view.ViewID != "work.context" || !validWorkHandle(view.SourceHandle) || !validWorkHandle(view.ScopeHandle) || view.ObservedAtUnixMS > nowUnixMS || view.ExpiresAtUnixMS <= nowUnixMS || view.ExpiresAtUnixMS-view.ObservedAtUnixMS > 300_000 {
			return WorkContextView{}, ErrInvalidWorkContext
		}
		if view.ObservedAtUnixMS > merged.ObservedAtUnixMS {
			merged.ObservedAtUnixMS = view.ObservedAtUnixMS
		}
		if view.ExpiresAtUnixMS < merged.ExpiresAtUnixMS {
			merged.ExpiresAtUnixMS = view.ExpiresAtUnixMS
		}
		merged.CoverageComplete = merged.CoverageComplete && view.CoverageComplete
		sources, scopes = append(sources, view.SourceHandle), append(scopes, view.ScopeHandle)
		for _, item := range view.Items {
			if seen[item.EvidenceHandle] || !validWorkItem(item, view.ObservedAtUnixMS) {
				return WorkContextView{}, ErrInvalidWorkContext
			}
			seen[item.EvidenceHandle] = true
			merged.Items = append(merged.Items, item)
		}
	}
	if len(merged.Items) > 64 {
		return WorkContextView{}, ErrInvalidWorkContext
	}
	sort.Strings(sources)
	sort.Strings(scopes)
	merged.SourceHandle = workHandle("work", strings.Join(sources, "\x00"))
	merged.ScopeHandle = workHandle("workspace", strings.Join(scopes, "\x00"))
	encoded, err := json.Marshal(merged)
	if err != nil || len(encoded) > 65_536 {
		return WorkContextView{}, ErrInvalidWorkContext
	}
	return merged, nil
}

func validWorkItem(item WorkItem, observedAt int64) bool {
	validKind := item.Kind == "selected_file" || item.Kind == "project" || item.Kind == "meeting_decision" || item.Kind == "communication"
	return validKind && validWorkHandle(item.EvidenceHandle) && validWorkText(item.Title, 512) && item.ObservedAtUnixMS >= 0 && item.ObservedAtUnixMS <= observedAt && validOptionalWorkText(item.Excerpt, 2048) && validOptionalWorkText(item.Status, 256) && validOptionalWorkText(item.Blocker, 512) && validOptionalWorkText(item.NextAction, 512)
}

func validOptionalWorkText(value *string, maximum int) bool {
	return value == nil || validWorkText(*value, maximum)
}

func validWorkText(value string, maximum int) bool {
	return strings.TrimSpace(value) != "" && len(value) <= maximum
}

func validWorkHandle(value string) bool {
	return strings.TrimSpace(value) != "" && len(value) <= 256
}

func workHandle(namespace, value string) string {
	digest := sha256.Sum256([]byte(namespace + "\x00" + value))
	return namespace + ":" + hex.EncodeToString(digest[:])
}
