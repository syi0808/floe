use std::{collections::BTreeMap, future::Future};

use chrono::{DateTime, Utc};
use floe_agent_contract::ExpertResult;
use floe_context_contract::ContextDependency;
use floe_day::{CalendarConnection, CalendarMirror, Event, PersonId};
use floe_kernel::AgentFailure;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    ActionAuthority, ActionAuthorityMode, ActionFailure, AgentActionAdmission, AgentActionEnvelope,
    CalendarAction, CalendarActionState, CalendarCreateReceipt, CalendarPreflight,
    ExpertProposalReference,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionErrorCode {
    Validation,
    NotFound,
    Conflict,
    Storage,
    Budget,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{message}")]
pub struct ActionError {
    pub code: ActionErrorCode,
    pub message: String,
    pub metadata: BTreeMap<String, String>,
}

impl ActionError {
    pub fn new(code: ActionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ActionErrorCode::Validation, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ActionErrorCode::NotFound, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ActionErrorCode::Conflict, message)
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::new(ActionErrorCode::Storage, message)
    }

    pub fn budget(message: impl Into<String>) -> Self {
        Self::new(ActionErrorCode::Budget, message)
    }
}

impl From<ActionError> for AgentFailure {
    fn from(error: ActionError) -> Self {
        match error.code {
            ActionErrorCode::NotFound => AgentFailure::NotFound,
            ActionErrorCode::Conflict => AgentFailure::Conflict,
            ActionErrorCode::Validation => AgentFailure::InvalidInput,
            ActionErrorCode::Storage => AgentFailure::StorageUnavailable,
            ActionErrorCode::Budget => AgentFailure::BudgetExceeded,
        }
    }
}

/// Durable projection of proposals, approvals and settled outcomes.
#[allow(async_fn_in_trait)]
pub trait ActionRepository: Send + Sync {
    async fn calendar_actions(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<CalendarAction>, ActionError>;
    async fn calendar_action(
        &self,
        person_id: PersonId,
        id: Uuid,
    ) -> Result<CalendarAction, ActionError>;
    async fn save_calendar_action(
        &self,
        action: &CalendarAction,
        previous: Option<&CalendarAction>,
    ) -> Result<(), ActionError>;
    async fn bounded_expert_calendar_action(
        &self,
        person_id: PersonId,
        invocation_id: Uuid,
    ) -> Result<Option<CalendarAction>, ActionError>;
    async fn action_authority(
        &self,
        person_id: PersonId,
    ) -> Result<Option<ActionAuthority>, ActionError>;
    async fn put_action_authority(&self, authority: &ActionAuthority) -> Result<(), ActionError>;
    async fn calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarMirror>, ActionError>;
    async fn calendar_connection(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarConnection>, ActionError>;
    async fn list_events(&self, person_id: PersonId) -> Result<Vec<Event>, ActionError>;
}

/// Durable pre-dispatch intent and settlement for expert-originated actions.
///
/// The implementation owns storage atomicity; the closure-taking members keep the
/// Actions decision inside the same durable transaction the evidence was read in.
#[allow(async_fn_in_trait)]
pub trait ExpertActionStore {
    async fn agent_action_policy(&self) -> Result<ActionAuthorityMode, AgentFailure>;
    async fn expert_proposal_dependency(
        &self,
        reference: &ExpertProposalReference,
    ) -> Result<ContextDependency, AgentFailure>;
    /// Record the durable pre-dispatch intent for one action.
    ///
    /// The admission it returns is what the store already knows about this
    /// execution; a caller that only needs the record to exist ignores it.
    async fn store_agent_action_envelope(
        &self,
        envelope: AgentActionEnvelope,
    ) -> Result<AgentActionAdmission, AgentFailure>;
    async fn agent_action_admission(
        &self,
        execution_id: Uuid,
    ) -> Result<AgentActionAdmission, AgentFailure>;
    async fn agent_calendar_action(
        &self,
        invocation_id: Uuid,
    ) -> Result<CalendarAction, AgentFailure>;
    async fn admit_agent_action_dispatch_with_cancellation_and_fence(
        &self,
        execution_id: Uuid,
        expected_digest: &str,
        now: DateTime<Utc>,
        cancellation: floe_execution::Cancellation,
        fence: impl Fn() -> Result<(), AgentFailure> + Send + Sync,
    ) -> Result<AgentActionAdmission, AgentFailure>;
    async fn settle_agent_action(
        &self,
        admission: &AgentActionAdmission,
        state: CalendarActionState,
    ) -> Result<CalendarAction, AgentFailure>;
    async fn decide_agent_action(
        &self,
        execution_id: Uuid,
        expected_digest: &str,
        approve: bool,
        now: DateTime<Utc>,
    ) -> Result<AgentActionAdmission, AgentFailure>;
    async fn cancel_agent_action(
        &self,
        execution_id: Uuid,
        expected_digest: &str,
    ) -> Result<AgentActionAdmission, AgentFailure>;
    async fn with_expert_proposal<ResultValue, Publish>(
        &self,
        reference: &ExpertProposalReference,
        publish: impl FnOnce(ExpertResult) -> Publish,
    ) -> Result<ResultValue, AgentFailure>
    where
        Publish: Future<Output = Result<ResultValue, AgentFailure>>;
    async fn with_recorded_expert_proposal<ResultValue, Inspect>(
        &self,
        reference: &ExpertProposalReference,
        inspect: impl FnOnce(ExpertResult) -> Inspect,
    ) -> Result<ResultValue, AgentFailure>
    where
        Inspect: Future<Output = Result<ResultValue, AgentFailure>>;
}

/// External calendar authority reached through a provider adapter.
pub trait CalendarActionProvider {
    fn preflight(
        &self,
        action: &CalendarAction,
        local_events: &[Event],
    ) -> impl Future<Output = Result<CalendarPreflight, ActionFailure>> + Send;

    fn create(
        &self,
        action: &CalendarAction,
    ) -> impl Future<Output = Result<CalendarCreateReceipt, ActionFailure>> + Send;

    fn lookup(
        &self,
        action: &CalendarAction,
    ) -> impl Future<Output = Result<Vec<CalendarCreateReceipt>, ActionFailure>> + Send;

    fn validate_source(
        &self,
        _action: &CalendarAction,
        _dependency: &ContextDependency,
        _native_subject_fingerprint: &str,
    ) -> impl Future<Output = Result<(), ActionFailure>> + Send {
        async { Err(ActionFailure::PermissionDenied) }
    }
}
