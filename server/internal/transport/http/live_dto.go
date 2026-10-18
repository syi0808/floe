package httptransport

import "floe/server/internal/inference"

type liveProfileDTO struct {
	ProviderID     string                        `json:"provider_id"`
	ModelID        string                        `json:"model_id"`
	Endpoint       string                        `json:"endpoint"`
	Capabilities   []string                      `json:"capabilities"`
	Limits         inference.LiveTransportLimits `json:"limits"`
	ProtocolSource string                        `json:"protocol_source"`
	Experimental   bool                          `json:"experimental"`
}

func liveProfileProjection(p *inference.LiveProfile) *liveProfileDTO {
	if p == nil {
		return nil
	}
	return &liveProfileDTO{ProviderID: p.ModelIdentity.ProviderID, ModelID: p.ModelIdentity.ModelID, Endpoint: p.ModelIdentity.Endpoint, Capabilities: append([]string(nil), p.Capabilities...), Limits: p.Limits, ProtocolSource: p.ProtocolSource, Experimental: p.Experimental}
}

type livePurposeDTO struct {
	SchemaVersion      int             `json:"schema_version"`
	Purpose            string          `json:"purpose"`
	Status             string          `json:"status"`
	CapabilityRevision string          `json:"capability_revision,omitempty"`
	Profile            *liveProfileDTO `json:"profile,omitempty"`
}

func livePurposeProjection(p inference.LivePurposeCapability) livePurposeDTO {
	return livePurposeDTO{SchemaVersion: 1, Purpose: string(inference.LivePresentation), Status: string(p.Status), CapabilityRevision: p.CapabilityRevision, Profile: liveProfileProjection(p.Profile)}
}
