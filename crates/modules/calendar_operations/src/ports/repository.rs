use chrono::{DateTime, Utc};
use floe_execution::BoxFuture;
use floe_kernel::{CommandFailure, PersonId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::record::*;
use floe_access::{
    OperationApprovalRef, OperationAuthorizationPolicy, OperationDecisionKind,
    OperationDecisionReceipt, OperationPolicyChange,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CalendarOperationStoreError {
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

impl From<CalendarOperationStoreError> for floe_kernel::AgentFailure {
    fn from(value: CalendarOperationStoreError) -> Self {
        match value {
            CalendarOperationStoreError::VaultLocked => Self::VaultLocked,
            CalendarOperationStoreError::CorruptRecord => Self::VaultUnavailable,
            CalendarOperationStoreError::Unavailable => Self::StorageUnavailable,
            CalendarOperationStoreError::StorageBusy => Self::StorageBusy,
            CalendarOperationStoreError::NotFound => Self::NotFound,
            CalendarOperationStoreError::Conflict => Self::Conflict,
            CalendarOperationStoreError::InvalidRecord => Self::InvalidInput,
            CalendarOperationStoreError::BudgetExceeded => Self::BudgetExceeded,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActionPage {
    pub records: Vec<ActionRecord>,
    pub next_cursor: Option<Uuid>,
}
pub type RecoveryPage = ActionPage;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationAdmission {
    pub command_id: Uuid,
    pub request_digest: ActionDigest,
    pub publication: Option<OperationApprovalPublicationContext>,
    pub record: ActionRecord,
}

/// Immutable Conversation correlation included in an Expert proposal's
/// canonical Calendar Operations command digest and owner receipt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationApprovalPublicationContext {
    pub session_id: Uuid,
    pub origin_run_id: Uuid,
}
#[derive(Clone, Debug)]
pub struct AdmittedOperation {
    pub record: ActionRecord,
    pub replayed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDecision {
    pub command_id: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub action_id: Uuid,
    pub expected_revision: u64,
    pub review_ref: OperationApprovalRef,
    pub decision: OperationDecisionKind,
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
    pub authorization: OperationDecisionReceipt,
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
    /// Restrict a caller-owned reconciliation to its exact origin inside the
    /// same transaction that admits the reconciliation command.
    pub expected_origin: Option<crate::ActionOriginKind>,
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
pub trait CalendarOperationsRepository: Send + Sync {
    /// Pure current coverage inspection for proposal presentation. Admission and
    /// dispatch still validate atomically in their own transactions.
    fn validate_proposal_coverage<'a>(
        &'a self,
        person_id: PersonId,
        coverage: &'a floe_agent_contract::DependencyCoverage,
    ) -> BoxFuture<'a, Result<(), CalendarOperationStoreError>>;

    fn get<'a>(
        &'a self,
        person_id: PersonId,
        action_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<ActionRecord>, CalendarOperationStoreError>>;
    fn list<'a>(
        &'a self,
        person_id: PersonId,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> BoxFuture<'a, Result<ActionPage, CalendarOperationStoreError>>;
    fn list_direct<'a>(
        &'a self,
        person_id: PersonId,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> BoxFuture<'a, Result<ActionPage, CalendarOperationStoreError>>;
    /// Replay lookup precedes fresh source, policy, Task evidence and expiry reads.
    fn find_admission<'a>(
        &'a self,
        person_id: PersonId,
        command_id: Uuid,
        request_digest: ActionDigest,
    ) -> BoxFuture<'a, Result<Option<ActionRecord>, CalendarOperationStoreError>>;
    fn admit<'a>(
        &'a self,
        admission: OperationAdmission,
    ) -> BoxFuture<'a, Result<AdmittedOperation, CommandFailure<CalendarOperationStoreError>>>;
    fn record_decision<'a>(
        &'a self,
        decision: ActionDecision,
    ) -> BoxFuture<'a, Result<ActionRecord, CommandFailure<CalendarOperationStoreError>>>;
    fn admit_reconciliation<'a>(
        &'a self,
        command: ActionReconciliation,
    ) -> BoxFuture<'a, Result<ActionRecord, CommandFailure<CalendarOperationStoreError>>>;
    fn stop_before_dispatch<'a>(
        &'a self,
        stop: PreDispatchStop,
    ) -> BoxFuture<'a, Result<ActionRecord, CalendarOperationStoreError>>;
    fn prepare_dispatch<'a>(
        &'a self,
        intent: DispatchIntent,
    ) -> BoxFuture<'a, Result<DispatchAdmission, CalendarOperationStoreError>>;
    fn load_execution<'a>(
        &'a self,
        person_id: PersonId,
        execution_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<DispatchAdmission>, CalendarOperationStoreError>>;
    fn settle_execution<'a>(
        &'a self,
        settlement: ExecutionSettlement,
    ) -> BoxFuture<'a, Result<ActionRecord, CalendarOperationStoreError>>;
    fn pending_recovery<'a>(
        &'a self,
        person_id: PersonId,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> BoxFuture<'a, Result<RecoveryPage, CalendarOperationStoreError>>;
    fn ack_collection<'a>(
        &'a self,
        ack: CollectionAck,
    ) -> BoxFuture<'a, Result<CollectionTicket, CalendarOperationStoreError>>;
    fn read_authority<'a>(
        &'a self,
        person_id: PersonId,
    ) -> BoxFuture<'a, Result<OperationAuthorizationPolicy, CalendarOperationStoreError>>;
    fn compare_and_set_authority<'a>(
        &'a self,
        change: OperationPolicyChange,
    ) -> BoxFuture<
        'a,
        Result<OperationAuthorizationPolicy, CommandFailure<CalendarOperationStoreError>>,
    >;
}
