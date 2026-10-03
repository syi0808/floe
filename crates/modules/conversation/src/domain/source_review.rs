use super::{
    ConversationInteraction, InteractionResolution, MAX_ACTIVE_INTERACTIONS_PER_RUN, RunState,
    RunTerminal, TurnAdmissionRequest, TurnMode,
};
use floe_agent_contract::{AgentFailure, PreparedModelPlan, SourceProjectionReview, TaskExecutionReceiptRef};
use floe_kernel::{PersonId, RunId};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceReviewLink {
    pub reference: floe_access::ReviewRef,
    pub requirement: floe_context_contract::SourceAccessRequirement,
}

/// Immutable owner evidence; never reconstructs transport or grants permission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BlockedReviewEvidence {
    ModelProjection {
        plan: PreparedModelPlan,
        review: SourceProjectionReview,
        access_reviews: Vec<SourceReviewLink>,
    },
    TaskModelProjection {
        execution: TaskExecutionReceiptRef,
        plan: PreparedModelPlan,
        review: SourceProjectionReview,
        access_reviews: Vec<SourceReviewLink>,
    },
    SourceRead {
        execution: TaskExecutionReceiptRef,
        tool_call_id: Uuid,
        blockers: floe_context_contract::SourceAccessBlockers,
        access_reviews: Vec<SourceReviewLink>,
    },
    ExpertBinding {
        execution: TaskExecutionReceiptRef,
        requirement_key: String,
        review: floe_experts::BindingReviewRef,
    },
    Navigation {
        execution: TaskExecutionReceiptRef,
        requirement: floe_context_contract::SourceAccessRequirement,
        target: super::NavigationOnlyTarget,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewAuditRecord {
    pub person_id: PersonId,
    pub device_id: String,
    pub session_id: Uuid,
    pub run_id: RunId,
    pub executor_generation: u64,
    pub operation_id: Uuid,
    pub evidence: BlockedReviewEvidence,
}

impl ReviewAuditRecord {
    pub fn targets(&self) -> Vec<super::ReviewedTarget> {
        match &self.evidence {
            BlockedReviewEvidence::ModelProjection { access_reviews, .. }
            | BlockedReviewEvidence::TaskModelProjection { access_reviews, .. }
            | BlockedReviewEvidence::SourceRead { access_reviews, .. } => access_reviews.iter()
                .map(|link| super::ReviewedTarget::SourceReview(link.reference.clone())).collect(),
            BlockedReviewEvidence::ExpertBinding { review, .. } => vec![super::ReviewedTarget::ExpertBinding(review.clone())],
            BlockedReviewEvidence::Navigation { target, .. } => vec![super::ReviewedTarget::NavigationOnly(target.clone())],
        }
    }
    pub fn matches_requirement(&self, requirement: &super::InteractionRequirement, target: &super::ReviewedTarget) -> bool {
        match (&self.evidence, target) {
            (BlockedReviewEvidence::ModelProjection { access_reviews, .. }
            | BlockedReviewEvidence::TaskModelProjection { access_reviews, .. }
            | BlockedReviewEvidence::SourceRead { access_reviews, .. }, super::ReviewedTarget::SourceReview(reference)) =>
                access_reviews.iter().any(|link| &link.reference == reference && *requirement == super::InteractionRequirement::from_source(&link.requirement, true)),
            (BlockedReviewEvidence::Navigation { requirement: source, .. }, super::ReviewedTarget::NavigationOnly(_)) =>
                *requirement == super::InteractionRequirement::from_source(source, false),
            (BlockedReviewEvidence::ExpertBinding { .. }, super::ReviewedTarget::ExpertBinding(_)) =>
                requirement.kind == super::InteractionRequirementKind::ConfigureExpertBinding && !requirement.inline
                    && requirement.source_id == "floe.expert.binding" && requirement.connection_id.is_none() && requirement.purpose == "configuration",
            _ => false,
        }
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.person_id.is_valid() || self.session_id.is_nil() || !self.run_id.is_valid()
            || self.executor_generation == 0 || self.operation_id.is_nil()
            || self.device_id.is_empty() || self.device_id.len() > 256
            || self.device_id.trim() != self.device_id || self.device_id.chars().any(char::is_control)
        { return Err(AgentFailure::StorageUnavailable); }
        match &self.evidence {
            BlockedReviewEvidence::ModelProjection { plan, review, .. }
            | BlockedReviewEvidence::TaskModelProjection { plan, review, .. } => {
                if let BlockedReviewEvidence::TaskModelProjection { execution, .. } = &self.evidence { execution.validate()?; }
                plan.validate()?; review.validate()?;
                let actual: [u8; 32] = sha2::Sha256::digest(serde_json::to_vec(&(plan, review.projection_operation_id, &review.blockers))
                    .map_err(|_| AgentFailure::StorageUnavailable)?).into();
                if actual != review.target_digest || self.device_id != plan.device_id
                    || self.person_id.to_string() != plan.principal || self.operation_id != review.projection_operation_id
                { return Err(AgentFailure::StorageUnavailable); }
            }
            BlockedReviewEvidence::SourceRead { execution, tool_call_id, blockers, .. } => {
                execution.validate()?; blockers.validate().map_err(|_| AgentFailure::StorageUnavailable)?;
                if tool_call_id.is_nil() { return Err(AgentFailure::StorageUnavailable); }
            }
            BlockedReviewEvidence::ExpertBinding { execution, requirement_key, review } => {
                execution.validate()?; review.validate()?;
                if requirement_key.is_empty() || requirement_key.len() > 256 || requirement_key.trim() != requirement_key
                    || requirement_key.chars().any(char::is_control) { return Err(AgentFailure::StorageUnavailable); }
            }
            BlockedReviewEvidence::Navigation { execution, requirement, target } => {
                execution.validate()?; requirement.validate().map_err(|_| AgentFailure::StorageUnavailable)?; target.validate()?;
                if requirement.source_id() != target.source_id
                    || requirement.connection_id().map(|id| id.as_str()) != target.connection_id.as_deref()
                    || requirement.consumer().identifier() != target.consumer
                    || super::InteractionRequirement::from_source(requirement, false).purpose != target.purpose
                    || target.destination != match requirement.reason() {
                        floe_context_contract::SourceAccessRequirementKind::RequestSystemPermission => super::NavigationDestination::SystemPermission,
                        floe_context_contract::SourceAccessRequirementKind::SelectResource => super::NavigationDestination::ResourcePicker,
                        _ => super::NavigationDestination::ConnectionSettings,
                    }
                    || requirement.reason() == floe_context_contract::SourceAccessRequirementKind::ReviewProcessing
                { return Err(AgentFailure::StorageUnavailable); }
            }
        }
        if let BlockedReviewEvidence::ModelProjection { review, access_reviews, .. }
            | BlockedReviewEvidence::TaskModelProjection { review, access_reviews, .. } = &self.evidence {
            validate_source_links(access_reviews, &review.blockers)?;
        }
        if let BlockedReviewEvidence::SourceRead { blockers, access_reviews, .. } = &self.evidence {
            validate_source_links(access_reviews, blockers)?;
        }
        let targets = self.targets();
        if targets.is_empty() || targets.len() > MAX_ACTIVE_INTERACTIONS_PER_RUN { return Err(AgentFailure::StorageUnavailable); }
        let mut seen = std::collections::HashSet::new();
        for target in targets {
            target.validate()?;
            if !seen.insert(super::canonical_target_digest(&target)?) { return Err(AgentFailure::StorageUnavailable); }
        }
        Ok(())
    }
}

fn validate_source_links(links: &[SourceReviewLink], blockers: &floe_context_contract::SourceAccessBlockers) -> Result<(), AgentFailure> {
    let mut ids = std::collections::HashSet::new();
    let mut connections = std::collections::HashSet::new();
    for link in links {
        link.reference.validate()?;
        link.requirement.validate().map_err(|_| AgentFailure::StorageUnavailable)?;
        if !blockers.blockers().contains(&link.requirement) || !ids.insert(link.reference.id)
            || !link.requirement.inline_resolution() || link.requirement.connection_id().is_none()
            || !connections.insert(link.requirement.connection_id().cloned()) {
            return Err(AgentFailure::StorageUnavailable);
        }
    }
    let expected = blockers.blockers().iter().map(|b| b.connection_id().cloned()).collect::<std::collections::HashSet<_>>();
    if expected != connections { return Err(AgentFailure::StorageUnavailable); }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct ReviewPublication {
    pub record: ReviewAuditRecord,
    pub interactions: Vec<ConversationInteraction>,
}
impl ReviewPublication {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.record.validate()?;
        let targets = self.record.targets();
        if self.interactions.len() != targets.len() { return Err(AgentFailure::InvalidInput); }
        for (interaction, target) in self.interactions.iter().zip(targets) {
            interaction.validate()?;
            if interaction.revision != 1 || !matches!(interaction.state, super::InteractionState::Pending) { return Err(AgentFailure::InvalidInput); }
            if interaction.person_id != self.record.person_id || interaction.session_id != self.record.session_id
                || interaction.origin_run_id != self.record.run_id || interaction.audit != self.record || interaction.target != target
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
    pub publications: Vec<ReviewPublication>,
}
impl BlockedRunCommit {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.terminal.validate()?;
        if !self.run_id.is_valid()
            || self.session_id.is_nil()
            || !self.person_id.is_valid()
            || self.expected_session_revision == 0
            || self.expected_aggregate_revision == 0
            || self.executor_generation == 0
            || self.terminal.state != RunState::Blocked
            || self.publications.is_empty()
            || self.publications.len() > MAX_ACTIVE_INTERACTIONS_PER_RUN
        {
            return Err(AgentFailure::InvalidInput);
        }
        let mut ids = std::collections::HashSet::new();
        let mut review_ids = std::collections::HashSet::new();
        for publication in &self.publications {
            publication.validate()?;
            if publication.record.run_id != self.run_id
                || publication.record.person_id != self.person_id
                || publication.record.session_id != self.session_id
                || publication.record.executor_generation != self.executor_generation
            {
                return Err(AgentFailure::InvalidInput);
            }
            for interaction in &publication.interactions {
                let review_id = match &interaction.target {
                    super::ReviewedTarget::SourceReview(reference) => Some(("source", reference.id)),
                    super::ReviewedTarget::ExpertBinding(reference) => Some(("binding", reference.id)),
                    super::ReviewedTarget::NavigationOnly(_) => None,
                };
                if review_id.is_some_and(|id| !review_ids.insert(id)) { return Err(AgentFailure::InvalidInput); }
                let link = self.terminal.blocked.as_ref().ok_or(AgentFailure::InvalidInput)?
                    .interactions.iter().find(|link| link.interaction_id == interaction.id).ok_or(AgentFailure::InvalidInput)?;
                if link.target != interaction.target || link.origin.origin != interaction.origin
                    || link.origin.person_id != self.person_id || link.origin.session_id != self.session_id
                    || link.origin.run_id != self.run_id || link.origin.executor_generation != self.executor_generation
                    || link.origin.device_id != publication.record.device_id {
                    return Err(AgentFailure::InvalidInput);
                }
                if !ids.insert(interaction.id) {
                    return Err(AgentFailure::InvalidInput);
                }
            }
        }
        let blocked = self
            .terminal
            .blocked
            .as_ref()
            .ok_or(AgentFailure::InvalidInput)?;
        if blocked
            .interaction_refs()
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            != ids
        {
            return Err(AgentFailure::InvalidInput);
        }
        for reference in &self.terminal.interactions {
            let interaction = self.publications.iter().flat_map(|publication| &publication.interactions)
                .find(|interaction| interaction.id == reference.interaction_id).ok_or(AgentFailure::InvalidInput)?;
            if reference.kind != interaction.kind || reference.status != floe_agent_contract::UserInteractionStatus::Pending {
                return Err(AgentFailure::InvalidInput);
            }
        }
        if ids.len() != self.terminal.interactions.len()
            || self
                .terminal
                .interactions
                .iter()
                .any(|r| !ids.contains(&r.interaction_id))
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// Decision intent is persisted before invoking the owner under this subordinate replay identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "owner", rename_all = "snake_case", deny_unknown_fields)]
pub enum OwnerResolutionReceipt {
    SourceProcessing {
        receipt: floe_access::GrantCommitReceipt,
    },
    ExpertBinding {
        receipt: floe_experts::BindingMutationReceipt,
    },
}
impl OwnerResolutionReceipt {
    pub fn command_id(&self) -> Uuid {
        match self {
            Self::SourceProcessing { receipt } => receipt.reservation.command_id,
            Self::ExpertBinding { receipt } => receipt.command_id.as_uuid(),
        }
    }
    pub fn operation_id(&self) -> Uuid {
        match self {
            Self::SourceProcessing { receipt } => receipt.reservation.operation_id,
            Self::ExpertBinding { receipt } => receipt.command_id.as_uuid(),
        }
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.command_id().is_nil() || self.operation_id().is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        match self {
            Self::SourceProcessing { receipt } => {
                receipt.validate()?;
                if receipt.commit_id.is_nil()
                    || !matches!(receipt.kind, floe_access::GrantCommitKind::Reviewed { .. })
                {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            Self::ExpertBinding { receipt } => {
                receipt.review_ref.validate()?;
                if !receipt.command_id.is_valid() || receipt.assignment_ref.is_nil() || receipt.binding_revision == 0 || receipt.registry_revision == 0 || receipt.committed_at_unix_ms < 0 {
                    return Err(AgentFailure::InvalidInput);
                }
            }
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
        if !self.origin_run_id.is_valid()
            || !self.person_id.is_valid()
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.session_id.is_nil()
            || self.user_message_id.is_nil()
            || self.group_digest == [0; 32]
            || self.expected_session_revision == 0
            || self.lineage == 0
            || self.lineage > super::MAX_RESUME_LINEAGE
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct ResumeChildAdmission {
    pub request: ResumeRequired,
    pub child: TurnAdmissionRequest,
}
impl ResumeChildAdmission {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.request.validate()?;
        self.child.validate()?;
        if self.child.principal != self.request.person_id.to_string()
            || self.child.device_id != self.request.device_id
            || self.child.session_id != self.request.session_id
            || self.child.expected_session_revision != self.request.expected_session_revision
            || !matches!(&self.child.mode, TurnMode::Resume(r) if r.origin_run_id == self.request.origin_run_id && r.lineage == self.request.lineage)
            || !matches!(&self.child.input, super::TurnInput::ExistingMessage { message_id } if *message_id == self.request.user_message_id)
        {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }
}
