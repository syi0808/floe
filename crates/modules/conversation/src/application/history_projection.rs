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

#[cfg(test)]
mod tests {
    use super::project_model_conversation_history;
    use floe_access::{
        ConnectionId, DependencyAuthorization, DependencyResolver, ExecutionOwnerId,
        GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use floe_agent_contract::{
        AgentFailure, DependencyCoverage, InvocationKey, ModelConversation, ModelConversationEntry,
        ToolCall, ToolResult,
    };
    use floe_context::EvidenceReader;
    use floe_context_contract::ContextDependency;
    use floe_execution::BoxFuture;
    use floe_kernel::PersonId;
    use std::{collections::BTreeMap, time::Duration};
    use uuid::Uuid;

    struct ScriptedEvidence(BTreeMap<Uuid, DependencyCoverage>);

    impl EvidenceReader for ScriptedEvidence {
        fn read_turn_coverage<'a>(
            &'a self,
            _session_id: Uuid,
            turn_id: Uuid,
        ) -> BoxFuture<'a, Result<DependencyCoverage, AgentFailure>> {
            Box::pin(async move {
                Ok(self
                    .0
                    .get(&turn_id)
                    .cloned()
                    .unwrap_or(DependencyCoverage::Unknown))
            })
        }
    }

    struct StaleResolver;

    impl DependencyResolver for StaleResolver {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> BoxFuture<'a, Result<(), AgentFailure>> {
            Box::pin(async { Err(AgentFailure::StaleContext) })
        }
    }

    fn dependency() -> ContextDependency {
        let person_id = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("fixture.connection").expect("valid connection"),
            floe_access::ConnectorId::try_new("fixture.connector").expect("valid connector"),
            ExecutionOwnerId::try_new("fixture.owner").expect("valid execution owner"),
        )
        .expect("valid source binding");
        let now = chrono::Utc::now();
        ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("fixture.resource").expect("valid resource")],
            SourceAuthority::new(),
            vec![
                ResourceHandle::try_new("fixture.source-resource").expect("valid source resource"),
            ],
            vec![GrantDataCategory::Content],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("fixture.manager").expect("valid consumer"),
            ProcessingRestriction::DeviceOnly,
            Uuid::new_v4(),
            vec![b'x'; 16],
            Uuid::new_v4(),
            Uuid::new_v4(),
            now,
            now + chrono::Duration::minutes(5),
        )
        .expect("valid source dependency")
    }

    fn tool_exchange(call_id: Uuid, coverage: DependencyCoverage) -> ModelConversationEntry {
        ModelConversationEntry::ToolExchange {
            call: ToolCall {
                call_id,
                invocation_key: InvocationKey::new(),
                tool_id: "fixture.tool".into(),
                definition_revision: 1,
                input: "{}".into(),
            },
            result: ToolResult {
                call_id,
                text: "tool output".into(),
                artifacts: vec![],
                coverage,
                issue: None,
            },
        }
    }

    #[tokio::test]
    async fn stale_tool_history_is_removed_as_one_entry_and_current_turn_is_preserved() {
        let stale_call_id = Uuid::new_v4();
        let independent_call_id = Uuid::new_v4();
        let current_call_id = Uuid::new_v4();
        let stale_coverage = DependencyCoverage::dependent(dependency())
            .expect("valid source-dependent tool coverage");
        let reader = ScriptedEvidence(BTreeMap::from([
            (stale_call_id, stale_coverage.clone()),
            (independent_call_id, DependencyCoverage::Independent),
        ]));
        let current_exchange = tool_exchange(current_call_id, DependencyCoverage::Independent);
        let conversation = ModelConversation {
            history: vec![
                ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: "retain the person's own input".into(),
                },
                tool_exchange(stale_call_id, stale_coverage),
                tool_exchange(independent_call_id, DependencyCoverage::Independent),
            ],
            current_turn: vec![
                ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: "current input".into(),
                },
                current_exchange,
            ],
        };
        let authorization = DependencyAuthorization {
            deadline: tokio::time::Instant::now() + Duration::from_secs(5),
            cancellation: floe_execution::Cancellation::new(),
        };
        let projected = project_model_conversation_history(
            &reader,
            Uuid::new_v4(),
            &conversation,
            Some(&StaleResolver),
            &authorization,
        )
        .await
        .expect("stale history is filtered without rewriting the current turn");

        assert_eq!(projected.conversation.history.len(), 2);
        assert!(matches!(
            projected.conversation.history[0],
            ModelConversationEntry::User { .. }
        ));
        assert!(matches!(
            &projected.conversation.history[1],
            ModelConversationEntry::ToolExchange { call, result }
                if call.call_id == independent_call_id && result.call_id == independent_call_id
        ));
        assert_eq!(
            projected.conversation.current_turn,
            conversation.current_turn
        );
    }
}
