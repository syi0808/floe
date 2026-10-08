use chrono::{DateTime, Utc};
use floe_execution::BoxFuture;
use floe_kernel::{CommandFailure, PersonId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ActionAuthorityMode;
use crate::domain::record::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ActionStoreError {
    #[error("Actions require an unlocked Vault")]
    VaultLocked,
    #[error("Actions storage is unavailable")]
    Unavailable,
    #[error("Actions storage is busy")]
    StorageBusy,
    #[error("Action was not found")]
    NotFound,
    #[error("Action changed")]
    Conflict,
    #[error("Action record is invalid")]
    InvalidRecord,
    #[error("Action record is corrupt")]
    CorruptRecord,
    #[error("Pending Expert Action capacity is exhausted")]
    BudgetExceeded,
}

impl From<ActionStoreError> for floe_kernel::AgentFailure {
    fn from(value: ActionStoreError) -> Self {
        match value {
            ActionStoreError::VaultLocked => Self::VaultLocked,
            ActionStoreError::CorruptRecord => Self::VaultUnavailable,
            ActionStoreError::Unavailable => Self::StorageUnavailable,
            ActionStoreError::StorageBusy => Self::StorageBusy,
            ActionStoreError::NotFound => Self::NotFound,
            ActionStoreError::Conflict => Self::Conflict,
            ActionStoreError::InvalidRecord => Self::InvalidInput,
            ActionStoreError::BudgetExceeded => Self::BudgetExceeded,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionsAuthority {
    pub person_id: PersonId,
    pub revision: u64,
    pub calendar_create: ActionAuthorityMode,
}

impl ActionsAuthority {
    pub fn default_for(person_id: PersonId) -> Self {
        Self {
            person_id,
            revision: 1,
            calendar_create: ActionAuthorityMode::Ask,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityChange {
    pub command_id: Uuid,
    pub person_id: PersonId,
    pub expected_revision: u64,
    pub mode: ActionAuthorityMode,
}

#[derive(Clone, Debug)]
pub struct ActionPage {
    pub records: Vec<ActionRecord>,
    pub next_cursor: Option<Uuid>,
}
pub type RecoveryPage = ActionPage;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionAdmission {
    pub command_id: Uuid,
    pub request_digest: ActionDigest,
    pub record: ActionRecord,
}
#[derive(Clone, Debug)]
pub struct AdmittedAction {
    pub record: ActionRecord,
    pub replayed: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionDecisionKind {
    Approve,
    Reject,
    Cancel,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDecision {
    pub command_id: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub action_id: Uuid,
    pub expected_revision: u64,
    pub review_ref: ActionReviewRef,
    pub decision: ActionDecisionKind,
    pub now: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchIntent {
    pub action_id: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub expected_revision: u64,
    pub execution_id: Uuid,
    pub effect_digest: ActionDigest,
    pub authorization: ActionAuthorization,
    pub current_source_fence: ActionSourceFence,
    pub executor_generation: u64,
    pub now: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct DispatchAdmission {
    pub intent: ExecutionIntent,
    pub record_revision: u64,
    pub dispatch_required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSettlement {
    pub person_id: PersonId,
    pub execution_id: Uuid,
    pub effect_digest: ActionDigest,
    pub expected_revision: u64,
    pub outcome: CalendarEffectOutcome,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionReconciliation {
    pub command_id: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub action_id: Uuid,
    pub expected_revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionAck {
    pub person_id: PersonId,
    pub execution_id: Uuid,
    pub receipt_digest: ActionDigest,
    pub expected_ticket_revision: u64,
    pub day_projection_ref: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreDispatchStop {
    pub person_id: PersonId,
    pub action_id: Uuid,
    pub expected_revision: u64,
    pub state: PreDispatchState,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreDispatchState {
    Expired,
    Cancelled,
    Blocked { reason: ActionBlockedReason },
}

/// The single encrypted Action authority. All writes call the Actions-owned pure
/// transition functions while the exact current record is held by one transaction.
pub trait ActionsRepository: Send + Sync {
    /// Pure current coverage inspection for proposal presentation. Admission and
    /// dispatch still validate atomically in their own transactions.
    fn validate_proposal_coverage<'a>(
        &'a self,
        person_id: PersonId,
        coverage: &'a floe_agent_contract::DependencyCoverage,
    ) -> BoxFuture<'a, Result<(), ActionStoreError>>;

    fn get<'a>(
        &'a self,
        person_id: PersonId,
        action_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<ActionRecord>, ActionStoreError>>;
    fn list<'a>(
        &'a self,
        person_id: PersonId,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> BoxFuture<'a, Result<ActionPage, ActionStoreError>>;
    /// Replay lookup precedes fresh source, policy, Task evidence and expiry reads.
    fn find_admission<'a>(
        &'a self,
        person_id: PersonId,
        command_id: Uuid,
        request_digest: ActionDigest,
    ) -> BoxFuture<'a, Result<Option<ActionRecord>, ActionStoreError>>;
    fn admit<'a>(
        &'a self,
        admission: ActionAdmission,
    ) -> BoxFuture<'a, Result<AdmittedAction, CommandFailure<ActionStoreError>>>;
    fn record_decision<'a>(
        &'a self,
        decision: ActionDecision,
    ) -> BoxFuture<'a, Result<ActionRecord, CommandFailure<ActionStoreError>>>;
    fn admit_reconciliation<'a>(
        &'a self,
        command: ActionReconciliation,
    ) -> BoxFuture<'a, Result<ActionRecord, CommandFailure<ActionStoreError>>>;
    fn stop_before_dispatch<'a>(
        &'a self,
        stop: PreDispatchStop,
    ) -> BoxFuture<'a, Result<ActionRecord, ActionStoreError>>;
    fn prepare_dispatch<'a>(
        &'a self,
        intent: DispatchIntent,
    ) -> BoxFuture<'a, Result<DispatchAdmission, ActionStoreError>>;
    fn load_execution<'a>(
        &'a self,
        person_id: PersonId,
        execution_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<DispatchAdmission>, ActionStoreError>>;
    fn settle_execution<'a>(
        &'a self,
        settlement: ExecutionSettlement,
    ) -> BoxFuture<'a, Result<ActionRecord, ActionStoreError>>;
    fn pending_recovery<'a>(
        &'a self,
        person_id: PersonId,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> BoxFuture<'a, Result<RecoveryPage, ActionStoreError>>;
    fn ack_collection<'a>(
        &'a self,
        ack: CollectionAck,
    ) -> BoxFuture<'a, Result<CollectionTicket, ActionStoreError>>;
    fn read_authority<'a>(
        &'a self,
        person_id: PersonId,
    ) -> BoxFuture<'a, Result<ActionsAuthority, ActionStoreError>>;
    fn compare_and_set_authority<'a>(
        &'a self,
        change: AuthorityChange,
    ) -> BoxFuture<'a, Result<ActionsAuthority, CommandFailure<ActionStoreError>>>;
}
