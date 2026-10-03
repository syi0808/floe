use floe_agent_contract::BoxFuture;
use floe_kernel::{AgentFailure, PersonId, RunId};
use uuid::Uuid;

use crate::{
    ConversationInteraction, DecisionAdmission, ExpireInteraction, ExpireOutcome,
    InteractionDecision, SupersedeInteraction,
};

/// Durable Conversation interaction storage.
///
/// Implementations keep interactions, decisions and publication linkage beside
/// Conversation state and enforce the atomicity the lifecycle needs: stable
/// publication identity, per-run limits, decision command identity and
/// compare-and-swap transitions. No method here grants authority, reads a
/// source or calls a model.
pub trait InteractionRepository: Send + Sync {
    fn resolving_interactions<'a>(
        &'a self,
        person_id: PersonId,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<ConversationInteraction>, AgentFailure>>;
    fn admit_refresh<'a>(
        &'a self,
        request: crate::InteractionRefresh,
    ) -> BoxFuture<'a, Result<ConversationInteraction, AgentFailure>>;
    fn resolve_and_request_resume<'a>(
        &'a self,
        commit: crate::InteractionResolutionCommit,
    ) -> BoxFuture<'a, Result<crate::InteractionResolutionReceipt, AgentFailure>>;
    fn pending_resume_requests<'a>(
        &'a self,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<crate::ResumeRequired>, AgentFailure>>;
    fn claim_resume<'a>(
        &'a self,
        request: crate::ResumeChildAdmission,
    ) -> BoxFuture<'a, Result<crate::TurnAdmission, AgentFailure>>;

    /// Trusted lookup by Person and id. A forged well-formed id reads back
    /// `None`; reference shape is never authorization.
    fn get_interaction<'a>(
        &'a self,
        person_id: PersonId,
        interaction_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<ConversationInteraction>, AgentFailure>>;

    /// Every interaction published by one origin Run, oldest first.
    fn list_run_interactions<'a>(
        &'a self,
        person_id: PersonId,
        origin_run_id: RunId,
    ) -> BoxFuture<'a, Result<Vec<ConversationInteraction>, AgentFailure>>;

    /// Persist decision intent and move the lifecycle forward. An identical
    /// command id rejoins the recorded decision before any revision check; a
    /// reused command id with a different digest conflicts, as do races on a
    /// now-advanced revision.
    fn record_decision<'a>(
        &'a self,
        decision: InteractionDecision,
    ) -> BoxFuture<'a, Result<DecisionAdmission, AgentFailure>>;

    /// Replace a Pending or Resolving interaction with newer review.
    fn mark_superseded<'a>(
        &'a self,
        supersede: SupersedeInteraction,
    ) -> BoxFuture<'a, Result<ConversationInteraction, AgentFailure>>;

    /// Persist Expired for a lapsed Pending or Resolving interaction.
    fn mark_expired<'a>(
        &'a self,
        expire: ExpireInteraction,
    ) -> BoxFuture<'a, Result<ExpireOutcome, AgentFailure>>;
}
