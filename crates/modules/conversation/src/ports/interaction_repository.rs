use floe_agent_contract::BoxFuture;
use floe_kernel::{AgentFailure, CommandFailure, OwnerActor, PersonId, RunId};
use uuid::Uuid;

use crate::{
    ConversationInteraction, DecisionAdmission, ExpireInteraction, ExpireOutcome,
    InteractionDecision, SupersedeInteraction,
};

pub struct RecoveryPage<Item, Cursor> {
    pub items: Vec<Item>,
    pub next_cursor: Option<Cursor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractionRecoveryCursor {
    pub created_at_unix_ms: i64,
    pub interaction_id: Uuid,
}

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
        actor: &'a OwnerActor,
        after: Option<InteractionRecoveryCursor>,
        limit: usize,
    ) -> BoxFuture<
        'a,
        Result<RecoveryPage<ConversationInteraction, InteractionRecoveryCursor>, AgentFailure>,
    >;
    fn admit_refresh<'a>(
        &'a self,
        request: crate::InteractionRefresh,
    ) -> BoxFuture<'a, Result<ConversationInteraction, CommandFailure<AgentFailure>>>;
    fn resolve_and_request_resume<'a>(
        &'a self,
        commit: crate::InteractionResolutionCommit,
    ) -> BoxFuture<'a, Result<crate::InteractionResolutionReceipt, AgentFailure>>;
    fn pending_resume_requests<'a>(
        &'a self,
        actor: &'a OwnerActor,
        after: Option<RunId>,
        limit: usize,
    ) -> BoxFuture<'a, Result<RecoveryPage<crate::ResumeRequired, RunId>, AgentFailure>>;
    fn pending_resume_request<'a>(
        &'a self,
        actor: &'a OwnerActor,
        origin: RunId,
    ) -> BoxFuture<'a, Result<Option<crate::ResumeRequired>, AgentFailure>>;
    /// Retire only a request whose exact Session has moved on; no child is admitted.
    fn reconcile_resume_request<'a>(
        &'a self,
        actor: &'a OwnerActor,
        origin: RunId,
    ) -> BoxFuture<'a, Result<Option<crate::ResumeRequired>, AgentFailure>>;
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

    /// Bounded Session interaction query using the owner Session index and
    /// current device. Implementations must not enumerate every Session Run.
    fn list_session_interactions<'a>(
        &'a self,
        person_id: PersonId,
        session_id: Uuid,
        device_id: &'a str,
    ) -> BoxFuture<'a, Result<Vec<ConversationInteraction>, AgentFailure>>;

    /// Persist decision intent and move the lifecycle forward. An identical
    /// command id rejoins the recorded decision before any revision check; a
    /// reused command id with a different digest conflicts, as do races on a
    /// now-advanced revision.
    fn record_decision<'a>(
        &'a self,
        decision: InteractionDecision,
    ) -> BoxFuture<'a, Result<DecisionAdmission, CommandFailure<AgentFailure>>>;

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
