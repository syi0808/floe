use floe_agent_contract::{AgentFailure, PreparedModelPlan, SourceProjectionReview};
use floe_kernel::{OwnerActor, PersonId, RunId};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use uuid::Uuid;
use super::{ConversationInteraction, InteractionOrigin, InteractionResolution, RunTerminal, RunState, TurnAdmissionRequest, TurnMode, MAX_ACTIVE_INTERACTIONS_PER_RUN};

/// Immutable audit evidence. This value never reconstructs a model transport or grants source access.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionReviewRecord {
    pub person_id: PersonId,
    pub device_id: String,
    pub session_id: Uuid,
    pub run_id: RunId,
    pub executor_generation: u64,
    pub plan: PreparedModelPlan,
    pub review: SourceProjectionReview,
    pub access_reviews: Vec<floe_access::ReviewRef>,
}
impl ProjectionReviewRecord {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.plan.validate()?;
        self.review.validate()?;
        if !self.person_id.is_valid() || self.session_id.is_nil() || !self.run_id.is_valid()
            || self.executor_generation == 0 || self.device_id != self.plan.device_id
            || self.person_id.to_string() != self.plan.principal
            || self.access_reviews.is_empty() || self.access_reviews.len() > MAX_ACTIVE_INTERACTIONS_PER_RUN
        { return Err(AgentFailure::StorageUnavailable); }
        let actual: [u8; 32] = sha2::Sha256::digest(&serde_json::to_vec(&(&self.plan, self.review.projection_operation_id, &self.review.blockers))
            .map_err(|_| AgentFailure::StorageUnavailable)?).into();
        if actual != self.review.target_digest { return Err(AgentFailure::StorageUnavailable); }
        let mut ids = std::collections::HashSet::new();
        for reference in &self.access_reviews { reference.validate()?; if !ids.insert(reference.id) { return Err(AgentFailure::StorageUnavailable); } }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ProjectionReviewPublication {
    pub record: ProjectionReviewRecord,
    pub interactions: Vec<ConversationInteraction>,
}
impl ProjectionReviewPublication {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.record.validate()?;
        if self.interactions.len() != self.record.access_reviews.len() { return Err(AgentFailure::InvalidInput); }
        for (interaction, reference) in self.interactions.iter().zip(&self.record.access_reviews) {
            interaction.validate()?;
            if interaction.person_id != self.record.person_id || interaction.session_id != self.record.session_id
                || interaction.origin_run_id != self.record.run_id
                || interaction.projection.as_ref() != Some(&self.record)
                || interaction.target != super::ReviewedTarget::SourceReview(reference.clone())
            { return Err(AgentFailure::InvalidInput); }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct BlockedRunCommit {
    pub run_id: RunId,
    pub session_id: Uuid,
    pub person_id: PersonId,
    pub expected_session_revision: u64,
    pub expected_aggregate_revision: u64,
    pub expected_journal_revision: u64,
    pub executor_generation: u64,
    pub terminal: RunTerminal,
    pub publications: Vec<ProjectionReviewPublication>,
}
impl BlockedRunCommit {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.terminal.validate()?;
        if !self.run_id.is_valid() || self.session_id.is_nil() || !self.person_id.is_valid()
            || self.expected_session_revision == 0 || self.expected_aggregate_revision == 0
            || self.executor_generation == 0 || self.terminal.state != RunState::Blocked
            || self.publications.is_empty() || self.publications.len() > MAX_ACTIVE_INTERACTIONS_PER_RUN
        { return Err(AgentFailure::InvalidInput); }
        let mut ids = std::collections::HashSet::new();
        for publication in &self.publications {
            publication.validate()?;
            if publication.record.run_id != self.run_id || publication.record.person_id != self.person_id
                || publication.record.session_id != self.session_id || publication.record.executor_generation != self.executor_generation
            { return Err(AgentFailure::InvalidInput); }
            for interaction in &publication.interactions { if !ids.insert(interaction.id) { return Err(AgentFailure::InvalidInput); } }
        }
        let blocked = self.terminal.blocked.as_ref().ok_or(AgentFailure::InvalidInput)?;
        if blocked.interaction_refs.iter().copied().collect::<std::collections::HashSet<_>>() != ids {
            return Err(AgentFailure::InvalidInput);
        }
        if ids.len() != self.terminal.interactions.len() || self.terminal.interactions.iter().any(|r| !ids.contains(&r.interaction_id)) {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// Decision intent is persisted before invoking the owner under this subordinate replay identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag="owner", rename_all="snake_case", deny_unknown_fields)]
pub enum OwnerResolutionReceipt {
    SourceProcessing { receipt: floe_access::GrantCommitReceipt },
    ExpertBinding { command_id: Uuid, operation_id: Uuid, revision: u64, digest: [u8; 32] },
}
impl OwnerResolutionReceipt {
    pub fn command_id(&self) -> Uuid { match self {
        Self::SourceProcessing { receipt } => receipt.reservation.command_id,
        Self::ExpertBinding { command_id, .. } => *command_id,
    } }
    pub fn operation_id(&self) -> Uuid { match self {
        Self::SourceProcessing { receipt } => receipt.reservation.operation_id,
        Self::ExpertBinding { operation_id, .. } => *operation_id,
    } }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.command_id().is_nil() || self.operation_id().is_nil() { return Err(AgentFailure::InvalidInput); }
        match self {
            Self::SourceProcessing { receipt } => {
                receipt.validate()?;
                if receipt.commit_id.is_nil() || !matches!(receipt.kind, floe_access::GrantCommitKind::Reviewed { .. }) { return Err(AgentFailure::InvalidInput); }
            }
            Self::ExpertBinding { revision, digest, .. } if *revision == 0 || *digest == [0; 32] => return Err(AgentFailure::InvalidInput),
            _ => {}
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct InteractionResolutionCommit {
    pub resolution: InteractionResolution,
    pub owner_receipt: OwnerResolutionReceipt,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeRequired {
    pub origin_run_id: RunId,
    pub person_id: PersonId,
    pub device_id: String,
    pub session_id: Uuid,
    pub user_message_id: Uuid,
    pub group_digest: [u8; 32],
    pub expected_session_revision: u64,
    pub lineage: u8,
}
impl ResumeRequired {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.origin_run_id.is_valid() || !self.person_id.is_valid() || self.device_id.is_empty()
            || self.device_id.len() > 256 || self.session_id.is_nil() || self.user_message_id.is_nil()
            || self.group_digest == [0; 32] || self.expected_session_revision == 0
            || self.lineage == 0 || self.lineage > super::MAX_RESUME_LINEAGE
        { return Err(AgentFailure::InvalidInput); } Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct ResumeChildAdmission { pub request: ResumeRequired, pub child: TurnAdmissionRequest }
impl ResumeChildAdmission {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.request.validate()?; self.child.validate()?;
        if self.child.principal != self.request.person_id.to_string() || self.child.device_id != self.request.device_id
            || self.child.session_id != self.request.session_id || self.child.expected_session_revision != self.request.expected_session_revision
            || !matches!(&self.child.mode, TurnMode::Resume(r) if r.origin_run_id == self.request.origin_run_id && r.lineage == self.request.lineage)
            || !matches!(&self.child.input, super::TurnInput::ExistingMessage { message_id } if *message_id == self.request.user_message_id)
        { return Err(AgentFailure::Conflict); } Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct PublishTaskProjectionReview {
    pub actor: OwnerActor,
    pub session_id: Uuid,
    pub origin_run_id: RunId,
    pub task_id: Uuid,
    pub capability_call_id: Option<Uuid>,
    pub plan: PreparedModelPlan,
    pub review: SourceProjectionReview,
    pub now_unix_ms: i64,
}
