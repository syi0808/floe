package contracts

import "floe/server/internal/contracts/source"

type ViewDescriptor = source.Descriptor

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
