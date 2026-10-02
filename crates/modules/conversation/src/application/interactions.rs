//! Trusted publication and decision commands for durable interactions.
//!
//! Publication verifies the interaction against its admitted origin: the
//! Person against the Session, the Run against the Session, and the Tool
//! call, Delegation Task or Model attempt against the origin Run's durable
//! journal identity. A forged origin conflicts even when every id is
//! well-formed. Decisions bind the reviewed target digest through
//! compare-and-swap; an identical command id rejoins the recorded decision.

use floe_agent_contract::{AgentFailure, JournalEvent, UserInteractionKind};
use floe_kernel::{PersonId, RunId};
use uuid::Uuid;

use crate::{
    ConversationInteraction, ConversationRepository, DecisionAdmission, ExpireInteraction,
    ExpireOutcome, InteractionDecision, InteractionDecisionKind, InteractionOrigin,
    InteractionRepository, InteractionRequirement, InteractionResolution, InteractionState,
    PublishAdmission, ReviewedTarget, RunState, SupersedeInteraction,
    domain::INTERACTION_PENDING_LIFETIME_MS,
    domain::{canonical_requirement_digest, canonical_target_digest, interaction_publication_id},
};

#[derive(Clone, Debug)]
pub struct PublishInteractionRequest {
    pub principal: String,
    pub session_id: Uuid,
    pub origin_run_id: RunId,
    pub origin: InteractionOrigin,
    pub kind: UserInteractionKind,
    pub requirement: InteractionRequirement,
    pub target: ReviewedTarget,
}

impl PublishInteractionRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.principal.trim() != self.principal
            || self.principal.is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.session_id.is_nil()
            || !self.origin_run_id.is_valid()
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.origin
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        self.requirement
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        self.target
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if (self.kind == UserInteractionKind::ProcessingRecipient)
            != (self.requirement.kind
                == crate::InteractionRequirementKind::ApproveProcessingRecipient)
            || (self.kind == UserInteractionKind::ExpertBinding)
                != (self.requirement.kind
                    == crate::InteractionRequirementKind::ConfigureExpertBinding)
            || (self.kind == UserInteractionKind::ExpertBinding)
                != matches!(self.target, crate::ReviewedTarget::ExpertBinding(_))
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecideInteractionCommand {
    pub command_id: Uuid,
    pub interaction_id: Uuid,
    pub principal: String,
    pub expected_revision: u64,
    pub kind: InteractionDecisionKind,
    pub target_digest: [u8; 32],
}

impl DecideInteractionCommand {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.command_id.is_nil()
            || self.interaction_id.is_nil()
            || self.principal.trim() != self.principal
            || self.principal.is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.expected_revision == 0
            || self.target_digest == [0; 32]
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// Publish the interaction for an admitted origin, or replay it.
///
/// A replay of the identical publication returns the same row without
/// re-verifying the origin journal: the stored row already binds the verified
/// origin, and its digests are rechecked on load. A new publication verifies
/// Person/Session/Run binding, rejects a cancelled origin Run, and requires
/// the origin call/task/attempt in the Run's durable journal.
pub async fn publish_interaction<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    request: PublishInteractionRequest,
    now_unix_ms: i64,
) -> Result<PublishAdmission, AgentFailure>
where
    Runs: ConversationRepository + ?Sized,
    Interactions: InteractionRepository + ?Sized,
{
    request.validate()?;
    if now_unix_ms < 0 {
        return Err(AgentFailure::InvalidInput);
    }
    let person_id = parse_principal(&request.principal)?;
    let requirement_digest = canonical_requirement_digest(&request.requirement)?;
    let target_digest = canonical_target_digest(&request.target)?;
    let id = interaction_publication_id(
        request.origin_run_id,
        &request.origin,
        &requirement_digest,
        &target_digest,
    )?;
    if let Some(existing) = interactions.get_interaction(person_id, id).await? {
        if existing.requirement_digest != requirement_digest
            || existing.target_digest != target_digest
            || existing.session_id != request.session_id
            || existing.origin_run_id != request.origin_run_id
            || existing.origin != request.origin
            || existing.kind != request.kind
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        return Ok(PublishAdmission::Existing(existing));
    }
    let receipt = runs
        .load_receipt(request.origin_run_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    if receipt.principal != request.principal || receipt.session_id != request.session_id {
        return Err(AgentFailure::Conflict);
    }
    if receipt.state == RunState::Cancelled {
        return Err(AgentFailure::Conflict);
    }
    let journal = runs.load_journal(request.origin_run_id).await?;
    if !origin_admitted(&journal, &request.origin) {
        return Err(AgentFailure::Conflict);
    }
    let expires_at_unix_ms = now_unix_ms
        .checked_add(INTERACTION_PENDING_LIFETIME_MS)
        .ok_or(AgentFailure::InvalidInput)?;
    let record = ConversationInteraction {
        id,
        person_id,
        session_id: request.session_id,
        origin_run_id: request.origin_run_id,
        origin_turn_id: request.origin_run_id.as_uuid(),
        origin: request.origin,
        kind: request.kind,
        requirement: request.requirement,
        requirement_digest,
        target: request.target,
        target_digest,
        state: InteractionState::Pending,
        revision: 1,
        created_at_unix_ms: now_unix_ms,
        expires_at_unix_ms,
    };
    record.validate().map_err(|_| AgentFailure::InvalidInput)?;
    interactions.publish_interaction(record).await
}

/// A blocked model dispatch, ready to publish under its attempted origin.
#[derive(Clone, Debug)]
pub struct PublishModelRequirement {
    pub principal: String,
    pub session_id: Uuid,
    pub origin_run_id: RunId,
    /// The exact attempted origin: Model{attempt_id} for a root or
    /// finalization dispatch, Task{task_id} for a delegated dispatch.
    pub origin: InteractionOrigin,
    pub requirement: floe_agent_contract::ProcessingRequirement,
    pub device_id: String,
}

/// Re-scope a blocked-dispatch requirement to the attempting Run.
///
/// A linked resume dispatches under its origin-carried lineage so a grant
/// reviewed under the origin still scopes the child — but a fresh blockage
/// is the child's own review: the card publishes under the attempting Run,
/// so the requirement must bind that same Run. A dispatch that already
/// binds itself passes through unchanged.
pub fn rescope_blocked_requirement(
    requirement: floe_agent_contract::ProcessingRequirement,
    session_id: Uuid,
    run_id: RunId,
) -> Result<floe_agent_contract::ProcessingRequirement, AgentFailure> {
    if requirement.lineage().session_id() == session_id
        && requirement.lineage().origin_run_id() == run_id.as_uuid()
    {
        return Ok(requirement);
    }
    let lineage = floe_agent_contract::RecipientLineage::try_new(session_id, run_id.as_uuid())
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    floe_agent_contract::ProcessingRequirement::try_new(
        requirement.recipient().to_owned(),
        requirement.profile_id().to_owned(),
        requirement.purpose().to_owned(),
        requirement.consumer().to_owned(),
        requirement.input_data_classes().to_vec(),
        requirement.source_scopes().to_vec(),
        requirement.projection_ref(),
        requirement.projection_revision(),
        lineage,
    )
    .map_err(|_| AgentFailure::StorageUnavailable)
}

/// Publish the durable card for one blocked model dispatch.
///
/// Converts the owner-produced requirement into the reviewed interaction
/// verbatim, binds the verified calling device, and verifies the attempted
/// origin against the origin Run's durable journal. The requirement's
/// lineage must match this execution's Session and origin Run; a mismatch
/// fails closed. A replay of the identical publication rejoins the same
/// row. Returns the published (or rejoined) interaction.
pub async fn publish_model_requirement<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    request: PublishModelRequirement,
    now_unix_ms: i64,
) -> Result<PublishAdmission, AgentFailure>
where
    Runs: ConversationRepository + ?Sized,
    Interactions: InteractionRepository + ?Sized,
{
    request
        .requirement
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    if request.principal.trim() != request.principal
        || request.principal.is_empty()
        || request.principal.len() > 256
        || request.principal.chars().any(char::is_control)
        || request.session_id.is_nil()
        || !request.origin_run_id.is_valid()
        || request.device_id.trim() != request.device_id
        || request.device_id.is_empty()
        || request.device_id.len() > 256
        || request.device_id.chars().any(char::is_control)
        || now_unix_ms < 0
    {
        return Err(AgentFailure::InvalidInput);
    }
    request
        .origin
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    // The lineage binds this explicit intent: a requirement minted for
    // another Session or origin never publishes here.
    if request.requirement.lineage().session_id() != request.session_id
        || request.requirement.lineage().origin_run_id() != request.origin_run_id.as_uuid()
    {
        return Err(AgentFailure::Conflict);
    }
    publish_interaction(
        runs,
        interactions,
        PublishInteractionRequest {
            principal: request.principal,
            session_id: request.session_id,
            origin_run_id: request.origin_run_id,
            origin: request.origin,
            kind: UserInteractionKind::ProcessingRecipient,
            requirement: InteractionRequirement {
                kind: crate::InteractionRequirementKind::ApproveProcessingRecipient,
                // For a processing requirement the source slot carries the
                // exact recipient identity under review.
                source_id: request.requirement.recipient().to_owned(),
                connection_id: None,
                consumer: request.requirement.consumer().to_owned(),
                purpose: request.requirement.purpose().to_owned(),
                inline: true,
            },
            target: ReviewedTarget::RecipientConsent(crate::RecipientConsentTarget {
                recipient: request.requirement.recipient().to_owned(),
                profile_id: request.requirement.profile_id().to_owned(),
                purpose: request.requirement.purpose().to_owned(),
                consumer: request.requirement.consumer().to_owned(),
                input_data_classes: request.requirement.input_data_classes().to_vec(),
                source_scopes: request.requirement.source_scopes().to_vec(),
                lineage: request.requirement.lineage(),
                device_id: request.device_id,
                projection_ref: request.requirement.projection_ref(),
                projection_revision: request.requirement.projection_revision(),
            }),
        },
        now_unix_ms,
    )
    .await
}

/// Decide a Pending interaction, or rejoin an identical recorded decision.
///
/// Deciding a lapsed interaction persists Expired and conflicts; it never
/// approves stale review. Terminal states only rejoin through the identical
/// command id.
pub async fn decide_interaction<Interactions>(
    interactions: &Interactions,
    command: DecideInteractionCommand,
    now_unix_ms: i64,
) -> Result<DecisionAdmission, AgentFailure>
where
    Interactions: InteractionRepository,
{
    command.validate()?;
    if now_unix_ms < 0 {
        return Err(AgentFailure::InvalidInput);
    }
    let person_id = parse_principal(&command.principal)?;
    let current = interactions
        .get_interaction(person_id, command.interaction_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    if current.projects_expired_at(now_unix_ms) {
        let expire = ExpireInteraction {
            interaction_id: command.interaction_id,
            person_id,
            now_unix_ms,
        };
        expire.validate()?;
        match interactions.mark_expired(expire).await {
            Ok(_) | Err(AgentFailure::Conflict) => return Err(AgentFailure::Conflict),
            Err(failure) => return Err(failure),
        }
    }
    let decision = InteractionDecision {
        command_id: command.command_id,
        interaction_id: command.interaction_id,
        interaction_revision: command.expected_revision,
        kind: command.kind,
        target_digest: command.target_digest,
        principal: command.principal,
        decided_at_unix_ms: now_unix_ms,
    };
    decision.validate()?;
    interactions.record_decision(decision).await
}

pub async fn resolve_interaction<Interactions>(
    interactions: &Interactions,
    resolution: InteractionResolution,
) -> Result<ConversationInteraction, AgentFailure>
where
    Interactions: InteractionRepository,
{
    resolution.validate()?;
    interactions.record_resolution(resolution).await
}

pub async fn supersede_interaction<Interactions>(
    interactions: &Interactions,
    supersede: SupersedeInteraction,
) -> Result<ConversationInteraction, AgentFailure>
where
    Interactions: InteractionRepository,
{
    supersede.validate()?;
    interactions.mark_superseded(supersede).await
}

pub async fn expire_interaction<Interactions>(
    interactions: &Interactions,
    expire: ExpireInteraction,
) -> Result<ExpireOutcome, AgentFailure>
where
    Interactions: InteractionRepository,
{
    expire.validate()?;
    interactions.mark_expired(expire).await
}

pub async fn load_interaction<Interactions>(
    interactions: &Interactions,
    principal: &str,
    interaction_id: Uuid,
) -> Result<ConversationInteraction, AgentFailure>
where
    Interactions: InteractionRepository,
{
    if interaction_id.is_nil() {
        return Err(AgentFailure::InvalidInput);
    }
    let person_id = parse_principal(principal)?;
    interactions
        .get_interaction(person_id, interaction_id)
        .await?
        .ok_or(AgentFailure::NotFound)
}

pub async fn list_run_interactions<Interactions>(
    interactions: &Interactions,
    principal: &str,
    origin_run_id: RunId,
) -> Result<Vec<ConversationInteraction>, AgentFailure>
where
    Interactions: InteractionRepository,
{
    if !origin_run_id.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let person_id = parse_principal(principal)?;
    interactions
        .list_run_interactions(person_id, origin_run_id)
        .await
}

fn origin_admitted(journal: &[crate::JournalEntry], origin: &InteractionOrigin) -> bool {
    journal.iter().any(|entry| match (&entry.event, origin) {
        (JournalEvent::ToolIntent { call }, InteractionOrigin::Tool { call_id }) => {
            call.call_id == *call_id
        }
        (JournalEvent::DelegationIntent { request }, InteractionOrigin::Task { task_id, .. }) => {
            request.task_id.as_uuid() == *task_id
        }
        (
            JournalEvent::ModelIntent { attempt_id, .. },
            InteractionOrigin::Model {
                attempt_id: expected,
            },
        ) => attempt_id == expected,
        _ => false,
    })
}

fn parse_principal(principal: &str) -> Result<PersonId, AgentFailure> {
    if principal.trim() != principal
        || principal.is_empty()
        || principal.len() > 256
        || principal.chars().any(char::is_control)
    {
        return Err(AgentFailure::InvalidInput);
    }
    let id = Uuid::parse_str(principal).map_err(|_| AgentFailure::InvalidInput)?;
    PersonId::from_uuid(id).ok_or(AgentFailure::InvalidInput)
}
