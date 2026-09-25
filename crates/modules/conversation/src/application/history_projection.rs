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
    use std::collections::HashMap;
    use std::sync::Mutex;

    use floe_context_contract::{
        ConnectionId, ConnectorId, ConsumerPolicyAuthority, DependencyCoverage, ExecutionOwnerId,
        GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use floe_execution::Cancellation;
    use tokio::time::Instant;

    use super::*;

    fn person() -> floe_kernel::PersonId {
        floe_kernel::PersonId::new()
    }

    fn dependency(person_id: floe_kernel::PersonId) -> ContextDependency {
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("connection").unwrap(),
            ConnectorId::try_new("connector").unwrap(),
            ExecutionOwnerId::try_new("owner").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let now = chrono::Utc::now();
        ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("resource").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("assistant").unwrap(),
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"fingerprint".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now - chrono::Duration::minutes(1),
            now + chrono::Duration::minutes(5),
        )
        .unwrap()
    }

    fn authorization() -> DependencyAuthorization {
        DependencyAuthorization {
            deadline: Instant::now() + std::time::Duration::from_secs(30),
            cancellation: Cancellation::default(),
        }
    }

    struct MapReader {
        coverage: Mutex<HashMap<Uuid, DependencyCoverage>>,
    }

    impl MapReader {
        fn with(entries: Vec<(Uuid, DependencyCoverage)>) -> Self {
            Self {
                coverage: Mutex::new(entries.into_iter().collect()),
            }
        }
    }

    impl EvidenceReader for MapReader {
        fn read_turn_coverage(
            &self,
            _session_id: Uuid,
            turn_id: Uuid,
        ) -> impl std::future::Future<Output = Result<DependencyCoverage, AgentFailure>> + Send
        {
            let coverage = self
                .coverage
                .lock()
                .unwrap()
                .get(&turn_id)
                .cloned()
                .unwrap_or(DependencyCoverage::Unknown);
            async move { Ok(coverage) }
        }
    }

    struct AcceptAll;

    impl DependencyResolver for AcceptAll {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), AgentFailure>> + Send + 'a>,
        > {
            Box::pin(async move { Ok(()) })
        }
    }

    struct DenyAll;

    impl DependencyResolver for DenyAll {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), AgentFailure>> + Send + 'a>,
        > {
            Box::pin(async move { Err(AgentFailure::PolicyDenied) })
        }
    }

    fn user(text: &str) -> ModelConversationEntry {
        ModelConversationEntry::User {
            message_id: Uuid::new_v4(),
            text: text.into(),
        }
    }

    fn preamble(text: &str) -> ModelConversationEntry {
        ModelConversationEntry::Preamble {
            message_id: Uuid::new_v4(),
            text: text.into(),
        }
    }

    fn assistant(text: &str) -> ModelConversationEntry {
        ModelConversationEntry::Assistant {
            message_id: Uuid::new_v4(),
            text: text.into(),
        }
    }

    fn entry_id(entry: &ModelConversationEntry) -> Uuid {
        match entry {
            ModelConversationEntry::User { message_id, .. }
            | ModelConversationEntry::Preamble { message_id, .. }
            | ModelConversationEntry::Assistant { message_id, .. } => *message_id,
            ModelConversationEntry::ToolExchange { .. }
            | ModelConversationEntry::DelegationExchange { .. } => {
                panic!("test helper builds text entries only")
            }
        }
    }

    #[tokio::test]
    async fn fresh_history_is_retained_with_its_dependencies() {
        let person_id = person();
        let user_entry = user("what did the source say?");
        let assistant_entry = assistant("the source said yes");
        let user_id = entry_id(&user_entry);
        let assistant_id = entry_id(&assistant_entry);
        let held = dependency(person_id);
        let reader = MapReader::with(vec![
            (user_id, DependencyCoverage::Independent),
            (
                assistant_id,
                DependencyCoverage::dependent(held.clone()).unwrap(),
            ),
        ]);
        let conversation = ModelConversation {
            history: vec![user_entry, assistant_entry],
            current_turn: vec![user("and now?")],
        };

        let projected = project_model_conversation_history(
            &reader,
            Uuid::new_v4(),
            &conversation,
            Some(&AcceptAll),
            &authorization(),
        )
        .await
        .unwrap();

        assert_eq!(projected.conversation.history.len(), 2);
        assert_eq!(projected.conversation.current_turn.len(), 1);
        assert_eq!(projected.authorized_history_dependencies, vec![held]);
    }

    #[tokio::test]
    async fn revoked_history_drops_derived_but_retains_user() {
        let person_id = person();
        let user_entry = user("what did the source say?");
        let preamble_entry = preamble("thinking");
        let assistant_entry = assistant("the source said yes");
        let user_id = entry_id(&user_entry);
        let preamble_id = entry_id(&preamble_entry);
        let assistant_id = entry_id(&assistant_entry);
        let stale = dependency(person_id);
        let reader = MapReader::with(vec![
            (user_id, DependencyCoverage::Independent),
            (
                preamble_id,
                DependencyCoverage::dependent(stale.clone()).unwrap(),
            ),
            (
                assistant_id,
                DependencyCoverage::dependent(stale.clone()).unwrap(),
            ),
        ]);
        let conversation = ModelConversation {
            history: vec![user_entry, preamble_entry, assistant_entry],
            current_turn: vec![user("and now?")],
        };

        let projected = project_model_conversation_history(
            &reader,
            Uuid::new_v4(),
            &conversation,
            Some(&DenyAll),
            &authorization(),
        )
        .await
        .unwrap();

        assert_eq!(projected.conversation.history.len(), 1);
        assert!(matches!(
            projected.conversation.history[0],
            ModelConversationEntry::User { .. }
        ));
        assert_eq!(projected.conversation.current_turn.len(), 1);
        assert!(projected.authorized_history_dependencies.is_empty());
    }

    #[tokio::test]
    async fn unknown_history_never_reaches_the_model_as_derived_context() {
        let assistant_entry = assistant("unrecorded derived text");
        let conversation = ModelConversation {
            history: vec![user("hello"), assistant_entry],
            current_turn: vec![user("and now?")],
        };
        // No recorded coverage at all: every history identity reads Unknown.
        let reader = MapReader::with(vec![]);

        let projected = project_model_conversation_history(
            &reader,
            Uuid::new_v4(),
            &conversation,
            Some(&AcceptAll),
            &authorization(),
        )
        .await
        .unwrap();

        assert_eq!(projected.conversation.history.len(), 1);
        assert!(matches!(
            projected.conversation.history[0],
            ModelConversationEntry::User { .. }
        ));
        assert!(projected.authorized_history_dependencies.is_empty());
    }

    #[tokio::test]
    async fn current_turn_is_never_filtered() {
        let stale_id = Uuid::new_v4();
        let conversation = ModelConversation {
            history: vec![],
            current_turn: vec![
                user("live question"),
                ModelConversationEntry::Assistant {
                    message_id: stale_id,
                    text: "live draft".into(),
                },
            ],
        };
        // Even denied coverage cannot remove current-turn entries.
        let reader = MapReader::with(vec![(
            stale_id,
            DependencyCoverage::dependent(dependency(person())).unwrap(),
        )]);

        let projected = project_model_conversation_history(
            &reader,
            Uuid::new_v4(),
            &conversation,
            Some(&DenyAll),
            &authorization(),
        )
        .await
        .unwrap();

        assert_eq!(projected.conversation.history.len(), 0);
        assert_eq!(projected.conversation.current_turn.len(), 2);
        assert!(projected.authorized_history_dependencies.is_empty());
    }

    #[tokio::test]
    async fn missing_resolver_denies_all_derived_history() {
        let conversation = ModelConversation {
            history: vec![user("hello"), assistant("derived")],
            current_turn: vec![user("and now?")],
        };
        let reader = MapReader::with(vec![]);

        let projected = project_model_conversation_history(
            &reader,
            Uuid::new_v4(),
            &conversation,
            None,
            &authorization(),
        )
        .await
        .unwrap();

        assert_eq!(projected.conversation.history.len(), 1);
        assert!(matches!(
            projected.conversation.history[0],
            ModelConversationEntry::User { .. }
        ));
    }
}
