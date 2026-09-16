use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use floe_context_contract::{GrantAuthority, GrantId, SourceAuthority};

use super::{
    AppProfileSelectionDto,
    calendar::{CalendarProviderDto, CalendarScopeDto},
};

pub use floe_agent_contract::{
    AgentFailureCategory, AgentFailureDomain, AgentFailureSafeAction, AgentRetryPolicy,
};

macro_rules! response_payload {
    ($name:ident) => {
        #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
        #[serde(transparent)]
        pub struct $name(pub Value);
    };
}

response_payload!(AgentEventDto);
response_payload!(AgentFailureDto);
response_payload!(AgentSessionDto);
response_payload!(CalendarExpertOverviewDto);
response_payload!(ConnectorSnapshotDto);
response_payload!(EpistemicStatusDto);
response_payload!(KnowledgeCandidateDto);
response_payload!(KnowledgeDecisionResultDto);
response_payload!(PersonalMemoryKindDto);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentVaultFailureDto {
    pub schema_version: u32,
    pub domain: AgentFailureDomain,
    pub category: AgentFailureCategory,
    pub reason_code: String,
    pub kind: String,
    pub stage: String,
    pub safe_actions: Vec<AgentFailureSafeAction>,
    pub affected_refs: Vec<String>,
    pub incident_id: String,
    pub retry_policy: AgentRetryPolicy,
    pub retryable: bool,
    pub recovery_action: AgentVaultRecoveryActionDto,
    /// Whether the client must reload the session before it can continue.
    /// Decided by the owner, never re-derived from `recovery_action`.
    pub reload_required: bool,
    /// Whether the client must stop applying results for the current session.
    pub seal_session: bool,
    pub correlation_request_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentVaultRecoveryActionDto {
    None,
    RefreshSession,
    RefreshContext,
    ReviewSource,
    ReopenVault,
    Reconcile,
    RetryRead,
}
response_payload!(RegistryOverviewDto);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryConfigurationDto {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub target: RegistryConfigurationTargetDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RegistryConfigurationTargetDto {
    Installation { id: Uuid, enabled: bool },
    Assignment { id: Uuid, enabled: bool },
    CalendarView { id: Uuid, enabled: bool },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarExpertSetupDto {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub setup_id: Uuid,
    pub provider: CalendarProviderDto,
    pub device_id: String,
    pub calendar_ids: Vec<String>,
    pub connection_scope: CalendarScopeDto,
    pub connection_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_authority: Option<SourceAuthority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_native_subject_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarAccessConfigurationDto {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub setup_id: Uuid,
    pub change: CalendarAccessChangeDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarSubjectPreviewRequestDto {
    pub provider: CalendarProviderDto,
    pub device_id: String,
    pub connection_id: String,
    pub calendar_ids: Vec<String>,
    pub connection_scope: CalendarScopeDto,
    pub connection_revision: u64,
    pub source_authority: SourceAuthority,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarSubjectPreviewDto {
    pub provider: CalendarProviderDto,
    pub device_id: String,
    pub calendar_ids: Vec<String>,
    pub connection_scope: CalendarScopeDto,
    pub connection_id: String,
    pub connection_revision: u64,
    pub source_authority: SourceAuthority,
    pub native_subject_fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CalendarAccessChangeDto {
    SetEnabled {
        enabled: bool,
    },
    SetScope {
        provider: CalendarProviderDto,
        device_id: String,
        calendar_ids: Vec<String>,
        connection_scope: CalendarScopeDto,
        connection_revision: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_authority: Option<SourceAuthority>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reviewed_native_subject_fingerprint: Option<String>,
    },
    Remove {},
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentVaultRequestDto {
    pub schema_version: u32,
    pub person_id: String,
    pub request_id: String,
    pub operation: AgentVaultOperationDto,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentVaultOperationDto {
    Submit { action: AgentVaultActionDto },
    Poll { after_sequence: usize },
    Stop {},
    Release {},
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentVaultActionDto {
    Status {},
    Create {},
    Unlock {},
    Lock {},
    Session {
        operation: AgentFixtureOperationDto,
    },
    Registry {
        change: Option<RegistryConfigurationDto>,
    },
    CalendarExperts {
        setup: Option<CalendarExpertSetupDto>,
    },
    CalendarAccess {
        change: CalendarAccessConfigurationDto,
    },
    CalendarSubjectPreview {
        request: CalendarSubjectPreviewRequestDto,
    },
    PersonalAccess {
        change: PersonalAccessConfigurationDto,
    },
    ContactsAccess {
        change: ContactsAccessConfigurationDto,
    },
    CalendarAction {
        operation: CalendarActionOperationDto,
    },
    InspectProposal {
        session_id: String,
        invocation_id: String,
    },
    ConversationSession {
        operation: AgentConversationSessionOperationDto,
    },
    ConversationTurn {
        request: AgentConversationTurnRequestDto,
    },
    MemoryReview {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decision: Option<AgentMemoryReviewDecisionDto>,
    },
    Memory {},
    Connections {},
    RemoteAuthorityInspectProducer {
        route: AgentRemoteRouteDto,
    },
    RemoteAuthorityReviewAndEnroll {
        route: AgentRemoteRouteDto,
        producer: RemoteProducerIdentityDto,
    },
    RemoteAuthorityEnrollmentStatus {
        route: AgentRemoteRouteDto,
        enrollment_id: String,
    },
    RemotePairingPrepare {},
    RemotePairingConfirm {
        route: AgentRemoteRouteDto,
        challenge: RemotePairingChallengeDto,
        polling_proof: String,
    },
    RemotePairingStatus {
        route: AgentRemoteRouteDto,
        pairing_id: String,
        polling_proof: String,
    },
    RemotePairingFinalize {
        route: AgentRemoteRouteDto,
        pairing_id: String,
        polling_proof: String,
        challenge: RemotePairingChallengeDto,
    },
    RemoteCalendarGrantPreview {
        route: AgentRemoteRouteDto,
        connector_id: String,
        connection_id: String,
        resource: String,
    },
    RemoteCalendarGrantReview {
        route: AgentRemoteRouteDto,
        connector_id: String,
        connection_id: String,
        resource: String,
        expected_producer_fingerprint: String,
    },
    RemoteCalendarGrantStatus {
        grant_id: GrantId,
    },
    RemoteCalendarGrantPause {
        grant_id: GrantId,
        expected_authority: GrantAuthority,
    },
    RemoteViewGrantPreview {
        route: AgentRemoteRouteDto,
        view_id: String,
        connector_id: String,
        connection_id: String,
        resource: String,
        consumer: String,
    },
    RemoteViewGrantReview {
        route: AgentRemoteRouteDto,
        view_id: String,
        connector_id: String,
        connection_id: String,
        resource: String,
        consumer: String,
        expected_producer_fingerprint: String,
        expected_source_authority: SourceAuthority,
        expected_connection_revision: u64,
        expected_provider_identity: String,
        expected_recipient: String,
    },
    RemoteViewGrantStatus {
        grant_id: GrantId,
    },
    RemoteViewGrantPause {
        grant_id: GrantId,
        expected_authority: GrantAuthority,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalAccessConfigurationDto {
    pub connector: String,
    pub device_id: String,
    pub change: PersonalAccessChangeDto,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PersonalAccessChangeDto {
    Inspect {},
    Review {
        expected_native_subject_fingerprint: String,
        consumers: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        feasibility_query: Option<FeasibilityGrantQueryDto>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_grant_id: Option<GrantId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_grant_authority: Option<GrantAuthority>,
    },
    SetEnabled {
        enabled: bool,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeasibilityGrantQueryDto {
    pub event_handle: String,
    pub evidence_handles: Vec<String>,
    pub destination_latitude: f64,
    pub destination_longitude: f64,
    pub event_start_unix_ms: i64,
    pub event_end_unix_ms: i64,
    pub travel_mode: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContactsAccessConfigurationDto {
    pub connector: String,
    pub device_id: String,
    pub change: ContactsAccessChangeDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContactsAccessChangeDto {
    Inspect {
        selected_handles: Vec<String>,
    },
    Review {
        selected_handles: Vec<String>,
        expected_native_subject_fingerprint: String,
        consumers: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_grant_id: Option<GrantId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_grant_authority: Option<GrantAuthority>,
    },
    SetEnabled {
        enabled: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalAccessOverviewDto {
    pub schema_version: u32,
    pub person_id: String,
    pub connector: String,
    pub device_id: String,
    pub connection_id: String,
    pub source_authority: Option<SourceAuthority>,
    pub grant_id: Option<GrantId>,
    pub grant_authority: Option<GrantAuthority>,
    pub state: String,
    pub review_required: bool,
    pub presence_available: bool,
    pub consumers: Vec<String>,
    pub native_subject_fingerprint: Option<String>,
    pub process_incarnation: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteProducerIdentityDto {
    pub schema_version: u32,
    pub instance_id: String,
    pub execution_owner: String,
    pub audience: String,
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteAuthorityEnrollmentStatusDto {
    pub enrollment_id: String,
    pub key_id: String,
    pub fingerprint: String,
    pub local_confirmed: bool,
    pub admin_approved: bool,
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemotePairingChallengeDto {
    pub schema_version: u32,
    pub pairing_id: String,
    pub challenge_id: String,
    pub challenge_b64url: String,
    pub producer_signature: String,
    pub producer: RemoteProducerIdentityDto,
    pub issuer: RemoteOwnerPublicKeyDto,
    pub expires_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemotePairingConfirmationDto {
    pub schema_version: u32,
    pub pairing_id: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemotePairingStatusDto {
    pub schema_version: u32,
    pub pairing_id: String,
    pub status: String,
    pub person_id: String,
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<RemoteProducerIdentityDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<RemoteOwnerPublicKeyDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteCalendarGrantPreviewDto {
    pub schema_version: u32,
    pub person_id: String,
    pub connector_id: String,
    pub connection_id: String,
    pub resource: String,
    pub source_authority: SourceAuthority,
    pub provider_identity: String,
    pub execution_owner: String,
    pub producer: RemoteProducerIdentityDto,
    pub consumer: String,
    pub purpose: String,
    pub recipient: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteCalendarGrantOverviewDto {
    pub schema_version: u32,
    pub person_id: String,
    pub grant_id: GrantId,
    pub grant_authority: GrantAuthority,
    pub connector_id: String,
    pub connection_id: String,
    pub resource: String,
    pub source_authority: SourceAuthority,
    pub execution_owner: String,
    pub state: String,
    pub review_required: bool,
    pub consumer: String,
    pub purpose: String,
    pub recipient: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteViewGrantPreviewDto {
    pub schema_version: u32,
    pub person_id: String,
    pub view_id: String,
    pub connector_id: String,
    pub connection_id: String,
    pub connection_revision: u64,
    pub resource: String,
    pub source_authority: SourceAuthority,
    pub provider_identity: String,
    pub execution_owner: String,
    pub producer: RemoteProducerIdentityDto,
    pub consumer: String,
    pub purpose: String,
    pub recipient: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteViewGrantOverviewDto {
    pub schema_version: u32,
    pub person_id: String,
    pub grant_id: GrantId,
    pub grant_authority: GrantAuthority,
    pub view_id: String,
    pub connector_id: String,
    pub connection_id: String,
    pub connection_revision: Option<u64>,
    pub resource: String,
    pub source_authority: SourceAuthority,
    pub execution_owner: String,
    pub state: String,
    pub review_required: bool,
    pub consumer: String,
    pub purpose: String,
    pub recipient: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteOwnerPublicKeyDto {
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentMemoryReviewDecisionDto {
    pub candidate_id: String,
    pub decision: AgentMemoryReviewDecisionKindDto,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMemoryReviewDecisionKindDto {
    Approve,
    Reject,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentConversationSessionOperationDto {
    Start {},
    Resume {},
    Get {
        session_id: String,
    },
    Recover {
        session_id: String,
        expected_revision: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConversationTurnRequestDto {
    pub session_id: String,
    pub expected_revision: u64,
    pub text: String,
    pub device_id: String,
    #[serde(default, skip_serializing_if = "is_auto_profile")]
    pub profile: AppProfileSelectionDto,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub continuation: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_of: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_route: Option<AgentRemoteRouteDto>,
}

fn is_auto_profile(profile: &AppProfileSelectionDto) -> bool {
    matches!(profile, AppProfileSelectionDto::Auto)
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRemoteRouteDto {
    pub base_url: String,
    pub bearer_token: String,
    pub purpose: String,
    pub external: bool,
    pub allow_external: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,
    pub calendar_connections: Vec<AgentRemoteCalendarConnectionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pairing: Option<AgentRemotePairingDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRemotePairingDto {
    pub client_id: String,
    pub person_id: String,
    pub device_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRemoteCalendarConnectionDto {
    pub connector_id: String,
    pub connection_id: String,
    pub connection_revision: u64,
}

impl std::fmt::Debug for AgentRemoteRouteDto {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AgentRemoteRouteDto")
            .field("base_url", &self.base_url)
            .field("bearer_token", &"[REDACTED]")
            .field("purpose", &self.purpose)
            .field("external", &self.external)
            .field("allow_external", &self.allow_external)
            .field("recipient", &self.recipient)
            .field("calendar_connections", &self.calendar_connections)
            .field("pairing", &self.pairing)
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProposalInspectionDto {
    pub schema_version: u32,
    pub person_id: String,
    pub session_id: String,
    pub invocation_id: String,
    pub action: Option<AgentProposalActionDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentMemoryReviewOverviewDto {
    pub schema_version: u32,
    pub person_id: String,
    pub candidates: Vec<KnowledgeCandidateDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<KnowledgeDecisionResultDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentMemoryOverviewDto {
    pub schema_version: u32,
    pub person_id: String,
    pub saved_count: usize,
    pub pending_count: usize,
    pub memories: Vec<AgentMemorySummaryDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentMemorySummaryDto {
    pub target_id: String,
    pub revision: u64,
    pub statement: String,
    pub memory_kind: PersonalMemoryKindDto,
    pub epistemic_status: EpistemicStatusDto,
    pub confidence_millis: u16,
    pub source_count: usize,
    pub origin: AgentMemoryOriginDto,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMemoryOriginDto {
    UserProvided,
    Learned,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProposalActionDto {
    pub action_id: String,
    pub execution_id: String,
    pub status: AgentProposalStatusDto,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProposalStatusDto {
    Pending,
    Approved,
    Rejected,
    Executing,
    Blocked,
    Unknown,
    Succeeded,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentVaultStateDto {
    Missing,
    Locked,
    Ready,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentVaultResultDto {
    pub request_id: String,
    pub events: Vec<AgentEventDto>,
    pub next_sequence: usize,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub session: Option<AgentSessionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<RegistryOverviewDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_experts: Option<CalendarExpertOverviewDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_subject_preview: Option<CalendarSubjectPreviewDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personal_access: Option<PersonalAccessOverviewDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_actions: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<AgentProposalInspectionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_review: Option<AgentMemoryReviewOverviewDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<AgentMemoryOverviewDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connections: Option<Vec<ConnectorSnapshotDto>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_producer: Option<RemoteProducerIdentityDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_enrollment: Option<RemoteAuthorityEnrollmentStatusDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_pairing: Option<RemotePairingStatusDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_owner: Option<RemoteOwnerPublicKeyDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_calendar_grant: Option<RemoteCalendarGrantOverviewDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_calendar_preview: Option<RemoteCalendarGrantPreviewDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_view_grant: Option<RemoteViewGrantOverviewDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_view_preview: Option<RemoteViewGrantPreviewDto>,
    pub failure: Option<AgentVaultFailureDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentFixtureRequestDto {
    pub schema_version: u32,
    pub person_id: String,
    pub operation: AgentFixtureOperationDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentFixtureOperationDto {
    Start {},
    Resume {},
    Get {
        session_id: String,
    },
    Turn {
        session_id: String,
        expected_revision: u64,
        prompt: AgentFixturePromptDto,
    },
    Recover {
        session_id: String,
        expected_revision: u64,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFixturePromptDto {
    Today,
    FollowUp,
    RepeatedCall,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentFixtureRunRequestDto {
    pub schema_version: u32,
    pub person_id: String,
    pub session_id: String,
    pub expected_revision: u64,
    pub operation: AgentFixtureRunOperationDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentFixtureRunOperationDto {
    Begin { prompt: AgentFixturePromptDto },
    Poll { after_sequence: usize },
    Stop {},
    Release {},
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentFixtureRunDto {
    pub session_id: String,
    pub expected_revision: u64,
    pub events: Vec<AgentEventDto>,
    pub next_sequence: usize,
    pub done: bool,
    pub session: Option<AgentSessionDto>,
    pub failure: Option<AgentFailureDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentFixtureResultDto {
    pub session: AgentSessionDto,
    pub events: Vec<AgentEventDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarActionRequestDto {
    pub schema_version: u32,
    pub person_id: String,
    pub operation: CalendarActionOperationDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CalendarActionOperationDto {
    Capabilities {},
    GetAuthority {},
    SetAuthority {
        calendar_create: ActionAuthorityModeDto,
    },
    Execute {
        action_id: String,
    },
    Recover {
        action_id: String,
    },
    List {},
    Get {
        action_id: String,
    },
    Propose {
        calendar_id: String,
        title: String,
        starts_at: String,
        ends_at: String,
        timezone: String,
    },
    Direct {
        calendar_id: String,
        title: String,
        starts_at: String,
        ends_at: String,
        timezone: String,
        event_id: Option<String>,
        event_revision: Option<u64>,
        delete: bool,
    },
    Decide {
        action_id: String,
        decision: CalendarActionDecisionDto,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionAuthorityModeDto {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarActionDecisionDto {
    Approve,
    Reject,
}
