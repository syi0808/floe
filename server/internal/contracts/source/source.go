// Package source contains immutable source identities and resource bounds shared
// by the source-read and authorization owners.
package source

type ID string

type SourceReference struct {
	ConnectorID, ConnectionID, ExecutionOwner, Incarnation string
	Epoch                                                  uint64
}

type Reference = SourceReference

type Descriptor struct {
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

type Snapshot struct {
	SourceReference
	ConnectionRevision                   uint64
	PersonID, DeviceID, ProviderIdentity string
	IdentityGeneration                   uint64
	Resources                            []string
	Active                               bool
	Descriptor                           Descriptor
}

type Bounds struct{ MaxItems, MaxBytes uint32 }

func Clone(snapshot Snapshot) Snapshot {
	snapshot.Resources = append([]string(nil), snapshot.Resources...)
	return snapshot
}
