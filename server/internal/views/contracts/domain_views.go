package contracts

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
