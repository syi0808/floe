use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use floe_context_contract::{GrantAuthority, GrantId, SourceAuthority};

use super::calendar::{CalendarProviderDto, CalendarScopeDto};

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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMemoryReviewDecisionKindDto {
    Approve,
    Reject,
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
