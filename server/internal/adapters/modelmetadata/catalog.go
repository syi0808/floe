// Package modelmetadata adapts the local model catalog to Inference's typed
// capability-evidence snapshot port.
package modelmetadata

import (
	"context"
	"errors"

	"floe/server/internal/inference"
	"floe/server/internal/modelcatalog"
)

type CatalogSource struct{ store *modelcatalog.Store }

func NewCatalogSource(store *modelcatalog.Store) *CatalogSource {
	return &CatalogSource{store: store}
}

func (source *CatalogSource) Snapshot(ctx context.Context) (inference.CapabilityEvidenceSnapshot, error) {
	if err := ctx.Err(); err != nil {
		return inference.CapabilityEvidenceSnapshot{}, err
	}
	if source == nil || source.store == nil {
		return inference.CapabilityEvidenceSnapshot{}, errors.New("model metadata unavailable")
	}
	projection := source.store.Projection()
	contract := projection.Catalog.CapabilityEvidence
	if contract == nil {
		return inference.CapabilityEvidenceSnapshot{ContractVersion: inference.CapabilityEvidenceContractVersion, Entries: []inference.CapabilityEvidenceEntry{}}, nil
	}
	snapshot := inference.CapabilityEvidenceSnapshot{
		ContractVersion: contract.ContractVersion,
		Entries:         make([]inference.CapabilityEvidenceEntry, len(contract.Entries)),
	}
	for i, entry := range contract.Entries {
		mapped := inference.CapabilityEvidenceEntry{
			ProviderID: entry.ProviderID,
			ModelID:    entry.ModelID,
			Endpoint:   entry.Endpoint,
			Facts:      make([]inference.CapabilityEvidenceFact, len(entry.Facts)),
		}
		for j, fact := range entry.Facts {
			mapped.Facts[j] = inference.CapabilityEvidenceFact{
				Capability: fact.Capability,
				Status:     inference.CapabilityStatus(fact.Status),
				Provenance: inference.CapabilityProvenance{Source: fact.Provenance.Source, VerifiedAt: fact.Provenance.VerifiedAt},
			}
		}
		snapshot.Entries[i] = mapped
	}
	return snapshot, nil
}

var _ inference.CapabilityMetadataPort = (*CatalogSource)(nil)
