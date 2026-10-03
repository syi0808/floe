package views

type CommunicationItem struct {
	EvidenceHandle string   `json:"evidence_handle"`
	ThreadHandle   string   `json:"thread_handle"`
	ReceivedUnixMS int64    `json:"received_unix_ms"`
	From           string   `json:"from,omitempty"`
	To             string   `json:"to,omitempty"`
	Subject        string   `json:"subject,omitempty"`
	Snippet        string   `json:"snippet,omitempty"`
	Labels         []string `json:"labels"`
}

type CommunicationView struct {
	SchemaVersion    int                 `json:"schema_version"`
	ViewID           string              `json:"view_id"`
	SourceHandle     string              `json:"source_handle"`
	ObservedAtUnixMS int64               `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS  int64               `json:"expires_at_unix_ms"`
	CoverageComplete bool                `json:"coverage_complete"`
	NextCursor       *int                `json:"next_cursor,omitempty"`
	Items            []CommunicationItem `json:"items"`
}
