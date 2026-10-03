use floe_agent_contract::{AgentFailure, BoxFuture, CommandId, ExecutionScope, OwnerActor,
    PackageRef, PersonId, TaskExecutionReceiptRef};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingReviewRef { pub id: Uuid, pub digest: [u8; 32] }
impl BindingReviewRef {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.id.is_nil() || self.digest == [0; 32] { return Err(AgentFailure::InvalidInput); }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingPrepareIdentity {
    pub command_id: CommandId,
    pub person_id: PersonId,
    pub device_id: String,
    pub assignment_id: Uuid,
    pub requirement_key: String,
    pub expected_binding_revision: u64,
    pub task_origin: Option<TaskExecutionReceiptRef>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedCandidate { pub candidate_ref: Uuid, pub candidate: crate::Candidate, pub selected: bool }

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingReviewDescriptor {
    pub review_ref: BindingReviewRef,
    pub identity: BindingPrepareIdentity,
    pub registry_instance_id: Uuid,
    pub installation_id: Uuid,
    pub package: PackageRef,
    pub definition_revision: u64,
    pub requirement: crate::ExpertSourceRequirement,
    pub candidates: Vec<ReviewedCandidate>,
    pub catalog_revision: u64,
    pub catalog_digest: [u8; 32],
    pub source_expectations: Vec<crate::CandidateSourceExpectation>,
    pub created_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingReplacementReceipt {
    pub review_ref: BindingReviewRef,
    pub registry: crate::RegistryCommitReceipt,
    pub committed_at_unix_ms: i64,
}

pub struct ReviewedBindingReplacement {
    pub review_ref: BindingReviewRef,
    pub expected_binding_revision: u64,
    pub candidate_refs: Vec<Uuid>,
    pub committed_at_unix_ms: i64,
    pub registry: crate::RegistryCommit,
}

pub trait BindingReviewRepository: Send + Sync {
    fn find_prepare<'a>(&'a self, actor: &'a OwnerActor, command_id: CommandId, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<Option<BindingReviewDescriptor>, AgentFailure>>;
    fn prepare<'a>(&'a self, descriptor: BindingReviewDescriptor, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<BindingReviewDescriptor, AgentFailure>>;
    fn get<'a>(&'a self, actor: &'a OwnerActor, reference: BindingReviewRef, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<BindingReviewDescriptor, AgentFailure>>;
    fn find_replacement<'a>(&'a self, actor: &'a OwnerActor, command_id: CommandId, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<Option<BindingReplacementReceipt>, AgentFailure>>;
    fn find_review_replacement<'a>(&'a self, actor: &'a OwnerActor, reference: BindingReviewRef,
        scope: &'a ExecutionScope) -> BoxFuture<'a, Result<Option<BindingReplacementReceipt>, AgentFailure>>;
    fn commit_replacement<'a>(&'a self, replacement: ReviewedBindingReplacement, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<BindingReplacementReceipt, AgentFailure>>;
}
