package common

type CalendarItem struct {
	EvidenceHandle string `json:"evidence_handle"`
	UntrustedTitle string `json:"untrusted_title"`
	StartsAtUnixMS int64  `json:"starts_at_unix_ms"`
	EndsAtUnixMS   int64  `json:"ends_at_unix_ms"`
	AllDay         bool   `json:"all_day"`
}

type CalendarView struct {
	SchemaVersion    int            `json:"schema_version"`
	ViewID           string         `json:"view_id"`
	SourceHandle     string         `json:"source_handle"`
	ObservedAtUnixMS int64          `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS  int64          `json:"expires_at_unix_ms"`
	RangeStartUnixMS int64          `json:"range_start_unix_ms"`
	RangeEndUnixMS   int64          `json:"range_end_unix_ms"`
	CoverageComplete bool           `json:"coverage_complete"`
	NextCursor       *string        `json:"next_cursor,omitempty"`
	Items            []CalendarItem `json:"items"`
}
