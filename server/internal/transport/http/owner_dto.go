package httptransport

import (
	"time"

	"floe/server/internal/inference"
	"floe/server/internal/integrations"
	"floe/server/internal/modelcatalog"
	"floe/server/internal/operation"
	"floe/server/internal/pairing"
	"floe/server/internal/trust"
)

type acknowledgementDTO struct {
	OK bool `json:"ok"`
}

type pairingIssuerDTO struct {
	KeyID       string `json:"key_id"`
	PublicKey   string `json:"public_key"`
	Fingerprint string `json:"fingerprint"`
}

func pairingIssuerProjection(value pairing.IssuerResult) pairingIssuerDTO {
	return pairingIssuerDTO{KeyID: value.KeyID, PublicKey: value.PublicKey, Fingerprint: value.Fingerprint}
}

type pairingStartDTO struct {
	SchemaVersion     int                    `json:"schema_version"`
	PairingID         string                 `json:"pairing_id"`
	Code              string                 `json:"code"`
	Proof             string                 `json:"proof"`
	ExpiresAtUnixMS   int64                  `json:"expires_at_unix_ms"`
	PersonID          string                 `json:"person_id"`
	DeviceID          string                 `json:"device_id"`
	Producer          trust.ProducerMetadata `json:"producer"`
	Issuer            pairingIssuerDTO       `json:"issuer"`
	ChallengeID       string                 `json:"challenge_id"`
	Challenge         string                 `json:"challenge_b64url"`
	ProducerSignature string                 `json:"producer_signature"`
}

type pairingStatusDTO struct {
	SchemaVersion     int                     `json:"schema_version"`
	PairingID         string                  `json:"pairing_id"`
	Status            string                  `json:"status"`
	PersonID          string                  `json:"person_id"`
	DeviceID          string                  `json:"device_id"`
	Issuer            *pairingIssuerDTO       `json:"issuer,omitempty"`
	IssuerFingerprint string                  `json:"issuer_fingerprint,omitempty"`
	ClientID          string                  `json:"client_id,omitempty"`
	Token             string                  `json:"token,omitempty"`
	Producer          *trust.ProducerMetadata `json:"producer,omitempty"`
}

type pairingStateDTO struct {
	SchemaVersion      int     `json:"schema_version"`
	PairingID          string  `json:"pairing_id"`
	Status             string  `json:"status"`
	CredentialDelivery *string `json:"credential_delivery,omitempty"`
}

type pairingCredentialDeliveryDTO struct {
	SchemaVersion     int                    `json:"schema_version"`
	PairingID         string                 `json:"pairing_id"`
	Status            string                 `json:"status"`
	ClientID          string                 `json:"client_id"`
	PersonID          string                 `json:"person_id"`
	DeviceID          string                 `json:"device_id"`
	Token             string                 `json:"token"`
	Producer          trust.ProducerMetadata `json:"producer"`
	IssuerFingerprint string                 `json:"issuer_fingerprint"`
	Issuer            pairingIssuerDTO       `json:"issuer"`
}

func pairingProjection(value pairing.ActionResult) (any, error) {
	switch result := value.(type) {
	case pairing.Acknowledgement:
		return acknowledgementDTO{OK: true}, nil
	case pairing.StartResult:
		return pairingStartDTO{SchemaVersion: result.SchemaVersion, PairingID: result.PairingID, Code: result.Code, Proof: result.Proof, ExpiresAtUnixMS: result.ExpiresAtUnixMS, PersonID: result.PersonID, DeviceID: result.DeviceID, Producer: result.Producer, Issuer: pairingIssuerProjection(result.Issuer), ChallengeID: result.ChallengeID, Challenge: result.Challenge, ProducerSignature: result.ProducerSignature}, nil
	case pairing.StatusResult:
		out := pairingStatusDTO{SchemaVersion: result.SchemaVersion, PairingID: result.PairingID, Status: result.Status, PersonID: result.PersonID, DeviceID: result.DeviceID}
		if credentials := result.Credentials; credentials != nil {
			issuer := pairingIssuerProjection(credentials.Issuer)
			producer := credentials.Producer
			out.Issuer = &issuer
			out.IssuerFingerprint = credentials.IssuerFingerprint
			out.ClientID = credentials.ClientID
			out.Token = credentials.Token
			out.Producer = &producer
		}
		return out, nil
	case pairing.StateResult:
		return pairingStateDTO{SchemaVersion: result.SchemaVersion, PairingID: result.PairingID, Status: result.Status, CredentialDelivery: result.CredentialDelivery}, nil
	case pairing.CredentialDeliveryResult:
		return pairingCredentialDeliveryDTO{SchemaVersion: result.SchemaVersion, PairingID: result.PairingID, Status: result.Status, ClientID: result.ClientID, PersonID: result.PersonID, DeviceID: result.DeviceID, Token: result.Token, Producer: result.Producer, IssuerFingerprint: result.IssuerFingerprint, Issuer: pairingIssuerProjection(result.Issuer)}, nil
	default:
		return nil, operation.Fail(operation.Internal, "operation_unavailable")
	}
}

func pairingStateProjection(value pairing.StateResult) pairingStateDTO {
	return pairingStateDTO{SchemaVersion: value.SchemaVersion, PairingID: value.PairingID, Status: value.Status, CredentialDelivery: value.CredentialDelivery}
}

type pairingPendingDTO struct {
	ID                  string    `json:"id"`
	Code                string    `json:"code"`
	Expires             time.Time `json:"expires"`
	PersonID            string    `json:"person_id"`
	DeviceID            string    `json:"device_id"`
	IssuerKeyID         string    `json:"issuer_key_id"`
	IssuerPublicKey     string    `json:"issuer_public_key"`
	IssuerFingerprint   string    `json:"issuer_fingerprint"`
	ProducerFingerprint string    `json:"producer_fingerprint"`
	ProducerAudience    string    `json:"producer_audience"`
	LocalConfirmed      bool      `json:"local_confirmed"`
	AdminApproved       bool      `json:"admin_approved"`
	Phase               string    `json:"phase"`
	AllowedActions      []string  `json:"allowed_actions"`
}

func pairingPendingProjection(value *pairing.OperatorPending) *pairingPendingDTO {
	if value == nil || value.Pending == nil {
		return nil
	}
	p := value.Pending
	return &pairingPendingDTO{ID: p.ID, Code: p.Code, Expires: p.Expires, PersonID: p.PersonID, DeviceID: p.DeviceID, IssuerKeyID: p.IssuerKeyID, IssuerPublicKey: p.IssuerPublicKey, IssuerFingerprint: p.IssuerFingerprint, ProducerFingerprint: p.ProducerFingerprint, ProducerAudience: p.ProducerAudience, LocalConfirmed: p.LocalConfirmed, AdminApproved: p.AdminApproved, Phase: value.Phase, AllowedActions: value.AllowedActions}
}

type integrationCapabilityDTO struct {
	Connect     bool `json:"connect"`
	Cancel      bool `json:"cancel"`
	Disconnect  bool `json:"disconnect"`
	ScopeUpdate bool `json:"scope_update"`
}

type connectorCatalogEntryDTO struct {
	ID                 string                   `json:"id"`
	Name               string                   `json:"name"`
	AuthKind           string                   `json:"auth_kind"`
	Available          bool                     `json:"available"`
	Status             string                   `json:"status"`
	RequiredScopes     []string                 `json:"required_scopes"`
	ScopeFields        []string                 `json:"scope_fields"`
	Capabilities       integrationCapabilityDTO `json:"capabilities"`
	ConnectionID       *string                  `json:"connection_id,omitempty"`
	ConnectionRevision *uint64                  `json:"connection_revision,omitempty"`
	Incarnation        *string                  `json:"incarnation,omitempty"`
	Epoch              *uint64                  `json:"epoch,omitempty"`
	ExecutionOwner     *string                  `json:"execution_owner,omitempty"`
	IdentityUnverified *bool                    `json:"identity_unverified,omitempty"`
	Scope              *map[string]any          `json:"scope,omitempty"`
}

type connectorCatalogDTO struct {
	SchemaVersion int                        `json:"schema_version"`
	PersonID      string                     `json:"person_id"`
	DeviceID      string                     `json:"device_id"`
	Connectors    []connectorCatalogEntryDTO `json:"connectors"`
	Revision      uint64                     `json:"revision"`
}

func catalogProjection(value integrations.CatalogResult) connectorCatalogDTO {
	items := make([]connectorCatalogEntryDTO, len(value.Connectors))
	for i, item := range value.Connectors {
		entry := connectorCatalogEntryDTO{ID: item.ID, Name: item.Name, AuthKind: item.AuthKind, Available: item.Available, Status: item.Status, RequiredScopes: item.RequiredScopes, ScopeFields: item.ScopeFields, Capabilities: integrationCapabilityDTO{Connect: item.Capabilities.Connect, Cancel: item.Capabilities.Cancel, Disconnect: item.Capabilities.Disconnect, ScopeUpdate: item.Capabilities.ScopeUpdate}}
		if item.HasConnection {
			connectionID := item.ConnectionID
			connectionRevision := item.ConnectionRevision
			incarnation := item.Incarnation
			epoch := item.Epoch
			executionOwner := item.ExecutionOwner
			entry.ConnectionID = &connectionID
			entry.ConnectionRevision = &connectionRevision
			entry.Incarnation = &incarnation
			entry.Epoch = &epoch
			entry.ExecutionOwner = &executionOwner
			identityUnverified := item.IdentityUnverified
			entry.IdentityUnverified = &identityUnverified
			scope := item.Scope
			entry.Scope = &scope
		}
		items[i] = entry
	}
	return connectorCatalogDTO{SchemaVersion: value.SchemaVersion, PersonID: value.PersonID, DeviceID: value.DeviceID, Connectors: items, Revision: value.Revision}
}

type attemptDTO struct {
	SchemaVersion int    `json:"schema_version"`
	OperationID   string `json:"operation_id"`
	ConnectorID   string `json:"connector_id"`
	ConnectionID  string `json:"connection_id"`
	PersonID      string `json:"person_id"`
	DeviceID      string `json:"device_id"`
	SetupState    string `json:"setup_state"`
	ManagementRef string `json:"management_ref"`
	Revision      uint64 `json:"revision"`
}

func attemptProjection(value integrations.AttemptResult) attemptDTO {
	return attemptDTO{SchemaVersion: value.SchemaVersion, OperationID: value.OperationID, ConnectorID: value.ConnectorID, ConnectionID: value.ConnectionID, PersonID: value.PersonID, DeviceID: value.DeviceID, SetupState: value.SetupState, ManagementRef: value.ManagementRef, Revision: value.Revision}
}

type scopeResultDTO struct {
	SchemaVersion      int            `json:"schema_version"`
	PersonID           string         `json:"person_id"`
	DeviceID           string         `json:"device_id"`
	ConnectionID       string         `json:"connection_id"`
	ConnectionRevision uint64         `json:"connection_revision"`
	ConnectorID        string         `json:"connector_id"`
	Scope              map[string]any `json:"scope"`
}

type disconnectDTO struct {
	SchemaVersion      int    `json:"schema_version"`
	OperationID        string `json:"operation_id"`
	PersonID           string `json:"person_id"`
	DeviceID           string `json:"device_id"`
	ConnectionID       string `json:"connection_id"`
	ConnectorID        string `json:"connector_id"`
	ConnectionRevision uint64 `json:"connection_revision"`
	CleanupState       string `json:"cleanup_state"`
}

type connectionsDTO struct {
	SchemaVersion int                     `json:"schema_version"`
	PersonID      string                  `json:"person_id"`
	DeviceID      string                  `json:"device_id"`
	Connections   []integrations.Snapshot `json:"connections"`
}

func connectionsProjection(value integrations.ConnectionsResult) connectionsDTO {
	connections := value.Connections
	if connections == nil {
		connections = []integrations.Snapshot{}
	}
	return connectionsDTO{SchemaVersion: value.SchemaVersion, PersonID: value.PersonID, DeviceID: value.DeviceID, Connections: connections}
}

type accountProgressDTO struct {
	Status           string `json:"status"`
	AuthorizationURL string `json:"auth_url"`
	InferenceEnabled bool   `json:"inference_enabled"`
}

type inferenceRecoveryDTO struct {
	OK        bool   `json:"ok"`
	Recovered bool   `json:"recovered"`
	Status    string `json:"status,omitempty"`
	Category  string `json:"category,omitempty"`
	Code      string `json:"code,omitempty"`
}

type inferenceProbeDTO struct {
	OK        bool   `json:"ok"`
	ElapsedMS int64  `json:"elapsed_ms"`
	TraceID   string `json:"trace_id"`
}

type providerPurposeDTO struct {
	Model            string                         `json:"model"`
	ReasoningEffort  string                         `json:"reasoning_effort"`
	Active           bool                           `json:"active"`
	Available        bool                           `json:"available"`
	CapabilityStates CapabilityStatesDTO            `json:"capability_states"`
	BudgetOverride   *inference.ModelBudgetOverride `json:"budget_override,omitempty"`
}

type providerProfileDTO struct {
	BaseURL       string                        `json:"base_url"`
	HasCredential bool                          `json:"has_credential"`
	Purposes      map[string]providerPurposeDTO `json:"purposes"`
}

type clientScopeDTO struct {
	PersonID string `json:"person_id"`
	DeviceID string `json:"device_id"`
}

type managementStateDTO struct {
	CSRF         string                        `json:"csrf"`
	Providers    map[string]providerProfileDTO `json:"providers"`
	Clients      []string                      `json:"clients"`
	ClientScopes map[string]clientScopeDTO     `json:"client_scopes"`
	Pairing      *pairingPendingDTO            `json:"pairing"`
	Address      string                        `json:"address"`
	Traces       []inference.AuditRecord       `json:"traces"`
	Inventory    *inference.PurposeInventory   `json:"inventory"`
	ModelCatalog *modelcatalog.Projection      `json:"model_catalog"`
}

func providerProfilesProjection(values map[string]inference.OperatorProviderProfile) map[string]providerProfileDTO {
	out := make(map[string]providerProfileDTO, len(values))
	for provider, value := range values {
		purposes := make(map[string]providerPurposeDTO, len(value.Purposes))
		for purpose, item := range value.Purposes {
			purposes[purpose] = providerPurposeDTO{Model: item.Model, ReasoningEffort: item.ReasoningEffort, Active: item.Active, Available: item.Available, CapabilityStates: capabilityStatesDTO(item.CapabilityStates), BudgetOverride: item.BudgetOverride}
		}
		out[provider] = providerProfileDTO{BaseURL: value.BaseURL, HasCredential: value.HasCredential, Purposes: purposes}
	}
	return out
}
