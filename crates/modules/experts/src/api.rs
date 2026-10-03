use floe_agent_contract::{AgentFailure, BoxFuture, CommandId, ExecutionScope, OwnerActor, TaskExecutionReceiptRef};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingReviewAction { Replace, Refresh }

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingCandidateSummary {
    pub candidate_ref: Uuid,
    pub label: String,
    pub availability: crate::CandidateAvailability,
    pub selected: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingReview {
    pub review_ref: crate::BindingReviewRef,
    pub assignment_ref: Uuid,
    pub requirement_ref: String,
    pub binding_revision: u64,
    pub candidate_refs_and_labels: Vec<BindingCandidateSummary>,
    pub expires_at_unix_ms: i64,
    pub allowed_actions: Vec<BindingReviewAction>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingInspection {
    pub assignment_ref: Uuid,
    pub requirement_ref: String,
    pub binding_revision: u64,
    pub candidates: Vec<BindingInspectionCandidate>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingInspectionCandidate {
    pub label: String,
    pub availability: crate::CandidateAvailability,
    pub selected: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertRequirementSummary {
    pub requirement_ref: String,
    pub label: String,
    pub selected_count: usize,
    pub minimum_sources: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertAssignmentSummary {
    pub assignment_ref: Uuid,
    pub installation_ref: Uuid,
    pub display_name: String,
    pub enabled: bool,
    pub binding_revision: u64,
    pub requirements: Vec<ExpertRequirementSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertInstallationSummary {
    pub installation_ref: Uuid,
    pub display_name: String,
    pub version: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertDirectorySnapshot {
    pub revision: u64,
    pub installations: Vec<ExpertInstallationSummary>,
    pub assignments: Vec<ExpertAssignmentSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingMutationReceipt {
    pub command_id: CommandId,
    pub review_ref: crate::BindingReviewRef,
    pub assignment_ref: Uuid,
    pub binding_revision: u64,
    pub registry_revision: u64,
    pub committed_at_unix_ms: i64,
}

pub trait ExpertClock: Send + Sync { fn now_unix_ms(&self) -> i64; }
pub struct SystemExpertClock;
impl ExpertClock for SystemExpertClock {
    fn now_unix_ms(&self) -> i64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64).unwrap_or(0)
    }
}

pub trait ExpertsOwner: Send + Sync {
    fn close_admission(&self);
    fn shutdown<'a>(&'a self) -> BoxFuture<'a, Result<(), AgentFailure>>;
    fn read_task_execution_receipt<'a>(&'a self, actor: &'a OwnerActor,
        reference: &'a TaskExecutionReceiptRef, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<floe_agent_contract::TaskExecutionReceipt, AgentFailure>>;
    fn directory<'a>(&'a self, actor: &'a OwnerActor, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<ExpertDirectorySnapshot, AgentFailure>>;
    fn set_installation_enabled<'a>(&'a self, actor: &'a OwnerActor, command_id: CommandId,
        installation_id: Uuid, expected_revision: u64, enabled: bool, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<ExpertDirectorySnapshot, AgentFailure>>;
    fn inspect_binding<'a>(&'a self, actor: &'a OwnerActor, assignment_id: Uuid,
        requirement_key: String, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<BindingInspection, AgentFailure>>;
    fn prepare_binding_review<'a>(&'a self, actor: &'a OwnerActor, command_id: CommandId,
        assignment_id: Uuid, requirement_key: String, expected_binding_revision: u64,
        scope: &'a ExecutionScope) -> BoxFuture<'a, Result<BindingReview, AgentFailure>>;
    fn prepare_task_binding_review<'a>(&'a self, actor: &'a OwnerActor, command_id: CommandId,
        task_receipt: TaskExecutionReceiptRef, requirement_key: String, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<BindingReview, AgentFailure>>;
    fn inspect_binding_review<'a>(&'a self, actor: &'a OwnerActor, review_ref: crate::BindingReviewRef,
        scope: &'a ExecutionScope) -> BoxFuture<'a, Result<BindingReview, AgentFailure>>;
    fn replace_binding<'a>(&'a self, actor: &'a OwnerActor, command_id: CommandId,
        review_ref: crate::BindingReviewRef, expected_binding_revision: u64,
        candidate_ids: Vec<Uuid>, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<ExpertDirectorySnapshot, AgentFailure>>;
    fn binding_operation_receipt<'a>(&'a self, actor: &'a OwnerActor, command_id: CommandId,
        scope: &'a ExecutionScope) -> BoxFuture<'a, Result<Option<BindingMutationReceipt>, AgentFailure>>;
    fn binding_review_receipt<'a>(&'a self, actor: &'a OwnerActor, review_ref: crate::BindingReviewRef,
        scope: &'a ExecutionScope) -> BoxFuture<'a, Result<Option<BindingMutationReceipt>, AgentFailure>>;
}
