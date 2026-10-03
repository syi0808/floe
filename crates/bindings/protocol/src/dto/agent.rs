use super::{EpistemicStatusDto, PersonalMemoryKindDto};
pub use floe_agent_contract::{
    AgentFailureCategory, AgentFailureDomain, AgentFailureSafeAction, AgentRetryPolicy,
};
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMemoryReviewDecisionKindDto {
    Approve,
    Reject,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentVaultStateDto {
    Missing,
    Locked,
    Ready,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionAuthorityModeDto {
    Allow,
    Ask,
    Deny,
}
