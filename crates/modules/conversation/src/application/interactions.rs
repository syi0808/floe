//! Trusted publication and decision commands for durable interactions.
//!
//! Publication is handled by the typed review path and authenticates the
//! recorded Run projection or actual Task receipt against its durable journal.
//! This module admits decisions against the immutable reviewed target. An
//! identical command rejoins its receipt; owner resolution commits the durable
//! resume request in the same encrypted transaction.

use floe_agent_contract::AgentFailure;
use floe_kernel::{PersonId, RunId};
use uuid::Uuid;
use crate::{ConversationInteraction, DecisionAdmission, ExpireInteraction, ExpireOutcome,
    InteractionDecision, InteractionDecisionKind, InteractionRepository, SupersedeInteraction};

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
    Interactions: InteractionRepository + ?Sized,
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

pub async fn resolve_interaction<Interactions: InteractionRepository + ?Sized>(
    interactions: &Interactions,
    commit: crate::InteractionResolutionCommit,
) -> Result<crate::InteractionResolutionReceipt, AgentFailure> {
    commit.resolution.validate()?;
    commit.owner_receipt.validate()?;
    interactions.resolve_and_request_resume(commit).await
}

pub async fn supersede_interaction<Interactions>(
    interactions: &Interactions,
    supersede: SupersedeInteraction,
) -> Result<ConversationInteraction, AgentFailure>
where
    Interactions: InteractionRepository + ?Sized,
{
    supersede.validate()?;
    interactions.mark_superseded(supersede).await
}

pub async fn expire_interaction<Interactions>(
    interactions: &Interactions,
    expire: ExpireInteraction,
) -> Result<ExpireOutcome, AgentFailure>
where
    Interactions: InteractionRepository + ?Sized,
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
    Interactions: InteractionRepository + ?Sized,
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
    Interactions: InteractionRepository + ?Sized,
{
    if !origin_run_id.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let person_id = parse_principal(principal)?;
    interactions
        .list_run_interactions(person_id, origin_run_id)
        .await
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
