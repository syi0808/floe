//! Applying Context's history decision to one model input.
//!
//! Context says, per recorded turn, what may still be shown; this is the
//! Session owner applying that: a Person's own message always stays, anything
//! derived from a turn that no longer re-admits is dropped, and a replay that
//! stood on the transcript that just changed goes with it.

use std::collections::BTreeMap;

use floe_agent_contract::{
    AgentFailure, ContextDependency, ModelConversation, ModelConversationEntry,
};
use floe_context::{
    DependencyAuthorization, DependencyResolver, EvidenceReader, TurnCoverageDecision,
};
use uuid::Uuid;

/// What a projection is applied against.
pub struct HistoryProjection<'a> {
    pub decisions: &'a BTreeMap<Uuid, TurnCoverageDecision>,
    /// The turn being produced now. Its own messages are never projected away:
    /// the run is writing them, not recalling them.
    pub current_turn: Option<Uuid>,
}

/// A typed history projection: the filtered conversation plus the exact
/// dependencies its retained history reauthorized under.
pub struct ProjectedModelConversation {
    pub conversation: ModelConversation,
    pub authorized_history_dependencies: Vec<ContextDependency>,
}

/// The recorded-coverage identity one history entry is reauthorized under.
///
/// Settled history entries quote their committed turn/message identity; a
/// history Tool/Delegation exchange quotes its stable call/task identity. An
/// identity with no recorded coverage reads back `Unknown`, which never
/// retains derived content.
fn history_entry_identity(entry: &ModelConversationEntry) -> Uuid {
    match entry {
        ModelConversationEntry::User { message_id, .. }
        | ModelConversationEntry::Preamble { message_id, .. }
        | ModelConversationEntry::Assistant { message_id, .. } => *message_id,
        ModelConversationEntry::ToolExchange { call, .. } => call.call_id,
        ModelConversationEntry::DelegationExchange { request, .. } => request.task_id.as_uuid(),
    }
}

/// Apply Context's history decision to a typed model conversation.
///
/// Context decides, per recorded identity, what may still be shown; this
/// applies that decision: the current turn is never filtered, a Person's own
/// historical message always stays, and historical derived entries survive only
/// when their recorded coverage reauthorizes. A denied or `Unknown` historical
/// entry never contributes dependency coverage. Replay pairs with current-turn
/// exchanges, which are never removed, so no receipt is dropped here.
pub async fn project_model_conversation_history(
    reader: &impl EvidenceReader,
    session_id: Uuid,
    conversation: &ModelConversation,
    resolver: Option<&dyn DependencyResolver>,
    authorization: &DependencyAuthorization,
) -> Result<ProjectedModelConversation, AgentFailure> {
    let decisions = floe_context::project_history(
        reader,
        session_id,
        conversation.history.iter().map(history_entry_identity),
        resolver,
        authorization,
    )
    .await?;
    let mut history = Vec::with_capacity(conversation.history.len());
    let mut authorized_history_dependencies = Vec::new();
    for entry in &conversation.history {
        let decision = decisions.get(&history_entry_identity(entry));
        let retain_derived = decision.is_some_and(|decision| decision.retain_derived);
        let retain = retain_derived || matches!(entry, ModelConversationEntry::User { .. });
        if !retain {
            continue;
        }
        if let Some(decision) = decision {
            for dependency in &decision.authorized_dependencies {
                if !authorized_history_dependencies.contains(dependency) {
                    authorized_history_dependencies.push(dependency.clone());
                }
            }
        }
        history.push(entry.clone());
    }
    Ok(ProjectedModelConversation {
        conversation: ModelConversation {
            history,
            current_turn: conversation.current_turn.clone(),
        },
        authorized_history_dependencies,
    })
}
