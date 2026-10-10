package inference

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"net/url"
	"sort"
	"strings"
	"time"
)

const CapabilityEvidenceContractVersion = 1

type CapabilityStatus string

const (
	CapabilityUnknown     CapabilityStatus = "unknown"
	CapabilitySupported   CapabilityStatus = "supported"
	CapabilityUnsupported CapabilityStatus = "unsupported"
)

type CapabilityProvenance struct {
	Source     string `json:"source"`
	VerifiedAt string `json:"verified_at"`
}

// CapabilityEvidenceSnapshot is an immutable typed view supplied through the
// owner port. An omitted fact means unknown; providers must never fill it from
// suggestion metadata, provider family, or a successful text probe.
type CapabilityEvidenceSnapshot struct {
	ContractVersion int                       `json:"contract_version"`
	Entries         []CapabilityEvidenceEntry `json:"entries"`
}

type CapabilityEvidenceEntry struct {
	ProviderID string                   `json:"provider_id"`
	ModelID    string                   `json:"model_id"`
	Endpoint   string                   `json:"endpoint"`
	Facts      []CapabilityEvidenceFact `json:"facts"`
}

type CapabilityEvidenceFact struct {
	Capability string               `json:"capability"`
	Status     CapabilityStatus     `json:"status"`
	Provenance CapabilityProvenance `json:"provenance"`
}

type CapabilityStateView struct {
	Status     CapabilityStatus      `json:"status"`
	Reason     string                `json:"reason,omitempty"`
	Provenance *CapabilityProvenance `json:"provenance,omitempty"`
}

// CapabilityStates contains all capabilities the operator can reason about.
// It is a map with a fixed set of keys and is cloned at API boundaries.
type CapabilityStates map[string]CapabilityStateView

type CapabilityMetadataPort interface {
	Snapshot(context.Context) (CapabilityEvidenceSnapshot, error)
}

func canonicalCapabilityNames() []string {
	return []string{ChatCapability, StructuredOutputCapability, ToolProposalsCapability}
}

func protocolCapabilitySet(values []string) (map[string]bool, bool) {
	if len(values) > 3 || !sort.StringsAreSorted(values) {
		return nil, false
	}
	seen := map[string]bool{}
	for i, value := range values {
		if value != ChatCapability && value != StructuredOutputCapability && value != ToolProposalsCapability || i > 0 && values[i-1] == value {
			return nil, false
		}
		seen[value] = true
	}
	return seen, true
}

func cloneCapabilityStates(states CapabilityStates) CapabilityStates {
	copy := make(CapabilityStates, len(states))
	for name, state := range states {
		if state.Provenance != nil {
			provenance := *state.Provenance
			state.Provenance = &provenance
		}
		copy[name] = state
	}
	return copy
}

func supportedCapabilities(states CapabilityStates) []string {
	values := make([]string, 0, len(states))
	for _, name := range canonicalCapabilityNames() {
		if states[name].Status == CapabilitySupported {
			values = append(values, name)
		}
	}
	return values
}

func defaultUnknownCapabilities(reason string) CapabilityStates {
	states := make(CapabilityStates, 3)
	for _, name := range canonicalCapabilityNames() {
		states[name] = CapabilityStateView{Status: CapabilityUnknown, Reason: reason}
	}
	return states
}

func validEvidenceSnapshot(snapshot CapabilityEvidenceSnapshot) bool {
	if snapshot.ContractVersion != CapabilityEvidenceContractVersion || snapshot.Entries == nil || len(snapshot.Entries) > 5000 {
		return false
	}
	entries := make(map[string]bool, len(snapshot.Entries))
	for _, entry := range snapshot.Entries {
		identity := ModelIdentity{ProviderID: entry.ProviderID, ModelID: entry.ModelID}
		endpoint, ok := CanonicalModelEndpoint(entry.Endpoint)
		if !validModelIdentityBase(identity) || !ok || endpoint != entry.Endpoint || entry.Facts == nil || len(entry.Facts) == 0 || len(entry.Facts) > 3 {
			return false
		}
		keyBytes, _ := json.Marshal(struct{ ProviderID, ModelID, Endpoint string }{entry.ProviderID, entry.ModelID, endpoint})
		key := string(keyBytes)
		if entries[key] {
			return false
		}
		entries[key] = true
		facts := map[string]bool{}
		for _, fact := range entry.Facts {
			if fact.Capability != ChatCapability && fact.Capability != StructuredOutputCapability && fact.Capability != ToolProposalsCapability || facts[fact.Capability] ||
				(fact.Status != CapabilitySupported && fact.Status != CapabilityUnsupported) ||
				!validProvenance(fact.Provenance) {
				return false
			}
			facts[fact.Capability] = true
		}
	}
	return true
}

func validModelIdentityBase(identity ModelIdentity) bool {
	return ValidAlias(identity.ProviderID) && identity.ModelID != "" && len(identity.ModelID) <= 128 &&
		identity.ModelID == strings.TrimSpace(identity.ModelID) && !strings.ContainsAny(identity.ModelID, "\r\n\x00")
}

func validProvenance(provenance CapabilityProvenance) bool {
	if provenance.Source == "" || len(provenance.Source) > 512 || strings.TrimSpace(provenance.Source) != provenance.Source || strings.ContainsAny(provenance.Source, "\r\n\x00") {
		return false
	}
	_, err := time.Parse(time.RFC3339, provenance.VerifiedAt)
	return err == nil
}

// CanonicalModelEndpoint returns the exact endpoint identity used for metadata
// matching. It removes URL spelling differences but never broadens an endpoint
// to a provider-family wildcard.
func CanonicalModelEndpoint(value string) (string, bool) {
	endpoint, err := url.Parse(value)
	if err != nil || strings.Contains(value, "#") || !endpoint.IsAbs() || endpoint.Opaque != "" || endpoint.User != nil || endpoint.RawQuery != "" || endpoint.ForceQuery || endpoint.Fragment != "" {
		return "", false
	}
	scheme := strings.ToLower(endpoint.Scheme)
	if scheme != "http" && scheme != "https" || endpoint.Hostname() == "" {
		return "", false
	}
	hostname := strings.ToLower(endpoint.Hostname())
	port := endpoint.Port()
	if scheme == "https" && port == "443" || scheme == "http" && port == "80" {
		port = ""
	}
	if port != "" {
		endpoint.Host = net.JoinHostPort(hostname, port)
	} else if strings.Contains(hostname, ":") {
		endpoint.Host = "[" + hostname + "]"
	} else {
		endpoint.Host = hostname
	}
	endpoint.Scheme = scheme
	escapedPath := strings.TrimRight(endpoint.EscapedPath(), "/")
	decodedPath, err := url.PathUnescape(escapedPath)
	if err != nil {
		return "", false
	}
	endpoint.Path = decodedPath
	endpoint.RawPath = escapedPath
	endpoint.ForceQuery = false
	canonical := endpoint.String()
	if canonical == "" {
		return "", false
	}
	return canonical, true
}

func resolveCapabilityStates(identity ModelIdentity, protocols []string, snapshot CapabilityEvidenceSnapshot, snapshotErr error) CapabilityStates {
	protocolSet, protocolOK := protocolCapabilitySet(protocols)
	states := defaultUnknownCapabilities("evidence_absent")
	if !protocolOK {
		return defaultUnknownCapabilities("adapter_protocol_unavailable")
	}
	for _, name := range canonicalCapabilityNames() {
		if !protocolSet[name] {
			states[name] = CapabilityStateView{Status: CapabilityUnsupported, Reason: "adapter_protocol_unsupported"}
		}
	}
	if snapshotErr != nil || !validModelIdentity(identity) || !validEvidenceSnapshot(snapshot) {
		for _, name := range canonicalCapabilityNames() {
			if states[name].Status == CapabilityUnknown {
				states[name] = CapabilityStateView{Status: CapabilityUnknown, Reason: "metadata_unavailable"}
			}
		}
		return states
	}
	for _, entry := range snapshot.Entries {
		endpoint, ok := CanonicalModelEndpoint(entry.Endpoint)
		if !ok || entry.ProviderID != identity.ProviderID || entry.ModelID != identity.ModelID || endpoint != identity.Endpoint {
			continue
		}
		for _, fact := range entry.Facts {
			if states[fact.Capability].Status == CapabilityUnsupported && states[fact.Capability].Reason == "adapter_protocol_unsupported" {
				continue
			}
			provenance := fact.Provenance
			states[fact.Capability] = CapabilityStateView{Status: fact.Status, Provenance: &provenance}
		}
		break
	}
	return states
}

func effectiveCapabilityMaterial(states CapabilityStates) []struct {
	Name   string
	Status CapabilityStatus
} {
	values := make([]struct {
		Name   string
		Status CapabilityStatus
	}, 0, len(states))
	for _, name := range canonicalCapabilityNames() {
		state := states[name].Status
		if state != CapabilitySupported && state != CapabilityUnsupported && state != CapabilityUnknown {
			state = CapabilityUnknown
		}
		values = append(values, struct {
			Name   string
			Status CapabilityStatus
		}{name, state})
	}
	return values
}

func supportsStructured(states CapabilityStates) bool {
	return states[ChatCapability].Status == CapabilitySupported && states[StructuredOutputCapability].Status == CapabilitySupported
}

func SupportsStructuredOutput(states CapabilityStates) bool { return supportsStructured(states) }

func SupportsAgent(states CapabilityStates, in AgentInvocation) bool {
	return supportsAgent(states, in)
}

func supportsAgent(states CapabilityStates, in AgentInvocation) bool {
	if !validOutputFormat(in.OutputFormat) || in.OutputFormat.Kind == "json" && len(in.Input.Tools) != 0 {
		return false
	}
	return states[ChatCapability].Status == CapabilitySupported &&
		(in.OutputFormat.Kind != "json" || states[StructuredOutputCapability].Status == CapabilitySupported) &&
		(len(in.Input.Tools) == 0 || states[ToolProposalsCapability].Status == CapabilitySupported)
}

func validateCapabilityStates(states CapabilityStates) error {
	if len(states) != 3 {
		return errors.New("invalid capability state set")
	}
	for _, name := range canonicalCapabilityNames() {
		state, ok := states[name]
		if !ok || state.Status != CapabilitySupported && state.Status != CapabilityUnsupported && state.Status != CapabilityUnknown {
			return errors.New("invalid capability state set")
		}
	}
	return nil
}
