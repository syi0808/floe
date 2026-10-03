package views

type ViewDescriptor struct {
	SchemaVersion      int    `json:"schema_version"`
	ID                 string `json:"id"`
	Version            string `json:"version"`
	DataClass          string `json:"data_class"`
	Retention          string `json:"retention"`
	FreshnessTTLMS     int64  `json:"freshness_ttl_ms"`
	MaxItems           int    `json:"max_items"`
	MaxBytes           int    `json:"max_bytes"`
	ProvenanceRequired bool   `json:"provenance_required"`
}

type ViewSnapshot struct {
	SchemaVersion    int    `json:"schema_version"`
	ViewID           string `json:"view_id"`
	SourceHandle     string `json:"source_handle"`
	ObservedAtUnixMS int64  `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS  int64  `json:"expires_at_unix_ms"`
	ItemCount        int    `json:"item_count"`
	ByteCount        int    `json:"byte_count"`
	ProvenanceCount  int    `json:"provenance_count"`
}
