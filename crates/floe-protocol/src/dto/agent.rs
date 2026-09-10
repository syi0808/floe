use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentVaultRequestDto {
    pub schema_version: u32,
    pub person_id: String,
    pub request_id: String,
    pub operation: AgentVaultOperationDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentVaultOperationDto {
    Submit { action: AgentVaultActionDto },
    Poll { after_sequence: usize },
    Stop {},
    Release {},
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
        change: Option<floe_agent::RegistryConfiguration>,
    },
    CalendarExperts {
        setup: Option<floe_agent::CalendarExpertSetup>,
    },
    CalendarAccess {
        change: floe_agent::CalendarAccessConfiguration,
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub continuation: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_route: Option<AgentRemoteRouteDto>,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRemoteRouteDto {
    pub base_url: String,
    pub bearer_token: String,
    pub purpose: String,
    pub external: bool,
    pub allow_external: bool,
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
    pub candidates: Vec<floe_agent::KnowledgeCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<floe_agent::KnowledgeDecisionResult>,
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
    pub memory_kind: floe_agent::PersonalMemoryKind,
    pub epistemic_status: floe_agent::EpistemicStatus,
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
    pub events: Vec<floe_agent::AgentEvent>,
    pub next_sequence: usize,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub session: Option<floe_agent::AgentSession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<floe_agent::RegistryOverview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_experts: Option<floe_agent::CalendarExpertOverview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<AgentProposalInspectionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_review: Option<AgentMemoryReviewOverviewDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<AgentMemoryOverviewDto>,
    pub failure: Option<floe_agent::AgentFailure>,
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
    pub events: Vec<floe_agent::AgentEvent>,
    pub next_sequence: usize,
    pub done: bool,
    pub session: Option<floe_agent::AgentSession>,
    pub failure: Option<floe_agent::AgentFailure>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentFixtureResultDto {
    pub session: floe_agent::AgentSession,
    pub events: Vec<floe_agent::AgentEvent>,
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
