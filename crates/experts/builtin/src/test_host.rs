//! A scripted host for dispatch tests: per-method outcomes plus a model
//! that answers once or panics when a blocked path must never call it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use floe_agent_contract::{
    AgentFailure, BoxFuture, ExpertModel, ExpertModelAnswer, ExpertModelCall, ExpertModelOutcome,
    InferencePolicyDecision,
};
use floe_context_contract::{
    AttentionView, AuthorizedRead, CalendarContextView, ConnectionId, ConnectorId,
    ConsumerPolicyAuthority, ContextDependency, ExecutionOwnerId, GrantAuthority, GrantConsumer,
    GrantDataCategory, GrantId, GrantOperation, GrantPurpose, GrantSourceBinding, HeldGrant,
    PeopleView, ProcessingRestriction, ResourceHandle, SourceAccessBlockers,
    SourceAccessRequirement, SourceAccessRequirementKind, SourceAuthority, SourceReadOutcome,
    SourceUnavailable, WellbeingView, WorkContextView,
};
use floe_execution::Cancellation;
use tokio::time::Instant;

use crate::{
    Acquiring, BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest,
    RequirementReadOutcome, StatefulExpertDraft,
};

pub struct ScriptedModel {
    pub calls: AtomicUsize,
    pub answer: Mutex<String>,
}

impl ExpertModel for ScriptedModel {
    fn answer<'a>(
        &'a self,
        _: ExpertModelCall,
    ) -> BoxFuture<'a, Result<ExpertModelOutcome, AgentFailure>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let answer = self.answer.lock().unwrap().clone();
        Box::pin(async move {
            Ok(ExpertModelOutcome::Answered(ExpertModelAnswer {
                schema_version: floe_agent_contract::AGENT_VERSION,
                answer,
                used_tokens: 10,
                cost_micros: 0,
            }))
        })
    }
}

#[derive(Clone)]
pub struct TestRead {
    pub payload: serde_json::Value,
}

impl HeldGrant for TestRead {
    fn bindings(&self) -> &[floe_context_contract::AuthorizedSourceBinding] {
        &[]
    }
}

impl AuthorizedRead for TestRead {
    fn payload(&self) -> &serde_json::Value {
        &self.payload
    }

    fn is_fresh(&self) -> bool {
        true
    }
}

type Scripted<T> = Mutex<Option<Result<SourceReadOutcome<T>, AgentFailure>>>;

pub struct ScriptedHost {
    pub model: ScriptedModel,
    pub policy: InferencePolicyDecision,
    pub source: Scripted<TestRead>,
    pub people: Scripted<PeopleView>,
    pub wellbeing: Scripted<WellbeingView>,
    pub attention: Scripted<(AttentionView, ContextDependency)>,
    pub calendar: Scripted<Vec<CalendarContextView>>,
    pub work: Scripted<Vec<WorkContextView>>,
    pub recorded: AtomicUsize,
}

impl ScriptedHost {
    pub fn new(answer: &str) -> Self {
        Self {
            model: ScriptedModel {
                calls: AtomicUsize::new(0),
                answer: Mutex::new(answer.to_owned()),
            },
            policy: crate::schedule::run_policy(floe_context_contract::DataClass::Personal),
            source: Mutex::new(None),
            people: Mutex::new(None),
            wellbeing: Mutex::new(None),
            attention: Mutex::new(None),
            calendar: Mutex::new(None),
            work: Mutex::new(None),
            recorded: AtomicUsize::new(0),
        }
    }

    fn take<T: Clone>(
        slot: &Scripted<T>,
        what: &str,
    ) -> Result<SourceReadOutcome<T>, AgentFailure> {
        slot.lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| panic!("{what} was not scripted"))
    }

    fn encode<T: serde::Serialize>(
        outcome: SourceReadOutcome<T>,
    ) -> Result<RequirementReadOutcome<crate::DeclaredSourceRead<TestRead>>, AgentFailure> {
        Ok(match outcome {
            SourceReadOutcome::Ready(value) => {
                RequirementReadOutcome::Ready(crate::DeclaredSourceRead::new(
                    serde_json::to_value(value).map_err(|_| AgentFailure::InvalidInput)?,
                    vec![],
                    None,
                ))
            }
            SourceReadOutcome::Unavailable(reason) => RequirementReadOutcome::Unavailable(reason),
            SourceReadOutcome::NeedsUserAction(_) => RequirementReadOutcome::NeedsUserAction,
        })
    }
}

impl BuiltinExpertHost for ScriptedHost {
    type Model = ScriptedModel;
    type SourceRead = TestRead;

    fn model(&self) -> &Self::Model {
        &self.model
    }

    fn policy(&self) -> &InferencePolicyDecision {
        &self.policy
    }

    fn read_requirement<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        key: &'a str,
        _: serde_json::Value,
    ) -> Acquiring<'a, RequirementReadOutcome<crate::DeclaredSourceRead<Self::SourceRead>>> {
        Box::pin(async move {
            if !crate::manifests().into_iter().any(|manifest| {
                manifest.package.id == request.agent_id
                    && manifest
                        .source_requirements
                        .iter()
                        .any(|requirement| requirement.key == key)
            }) {
                return Err(AgentFailure::CapabilityDenied);
            }
            let has_work = self.work.lock().unwrap().is_some();
            match key {
                "floe.source.calendar" => {
                    Self::encode(Self::take(&self.calendar, "calendar views")?)
                }
                "floe.source.contacts" => Self::encode(Self::take(&self.people, "people view")?),
                "floe.source.wellbeing" => {
                    Self::encode(Self::take(&self.wellbeing, "wellbeing view")?)
                }
                "floe.source.attention" => {
                    Ok(match Self::take(&self.attention, "attention view")? {
                        SourceReadOutcome::Ready((view, dependency)) => {
                            RequirementReadOutcome::Ready(crate::DeclaredSourceRead::new(
                                serde_json::to_value(view)
                                    .map_err(|_| AgentFailure::InvalidInput)?,
                                vec![dependency],
                                None,
                            ))
                        }
                        SourceReadOutcome::Unavailable(reason) => {
                            RequirementReadOutcome::Unavailable(reason)
                        }
                        SourceReadOutcome::NeedsUserAction(_) => {
                            RequirementReadOutcome::NeedsUserAction
                        }
                    })
                }
                "floe.source.work-context" if has_work => {
                    Self::encode(Self::take(&self.work, "work context views")?)
                }
                "floe.source.confirmed-interactions" => Ok(RequirementReadOutcome::Unavailable(
                    SourceUnavailable::TemporarilyUnavailable,
                )),
                "floe.source.confirmed-memory" | "floe.source.tasks" => {
                    Err(AgentFailure::CapabilityUnavailable)
                }
                _ => Ok(match Self::take(&self.source, "source view")? {
                    SourceReadOutcome::Ready(read) => RequirementReadOutcome::Ready(
                        crate::DeclaredSourceRead::new(read.payload.clone(), vec![], Some(read)),
                    ),
                    SourceReadOutcome::Unavailable(reason) => {
                        RequirementReadOutcome::Unavailable(reason)
                    }
                    SourceReadOutcome::NeedsUserAction(_) => {
                        RequirementReadOutcome::NeedsUserAction
                    }
                }),
            }
        })
    }

    fn record_dependency(
        &self,
        _: uuid::Uuid,
        _: uuid::Uuid,
        _: ContextDependency,
    ) -> Result<(), AgentFailure> {
        self.recorded.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn settle_stateful_result<'a>(
        &'a self,
        _: &'a BuiltinExpertRequest,
        _: StatefulExpertDraft,
    ) -> Acquiring<'a, BuiltinExpertOutput> {
        Box::pin(async { panic!("no stateful settlement in dispatch tests") })
    }
}

pub fn blocked(source_id: &str) -> SourceAccessBlockers {
    let requirement = SourceAccessRequirement::try_new(
        source_id,
        Some(ConnectorId::try_new("test.connector").unwrap()),
        Some(ConnectionId::try_new("test-connection").unwrap()),
        GrantOperation::Read,
        GrantConsumer::builtin("floe.builtin.test").unwrap(),
        GrantPurpose::Assistant,
        vec![ResourceHandle::try_new("test.resource").unwrap()],
        None,
        SourceAccessRequirementKind::EnableObserve,
        None,
        None,
        true,
    )
    .unwrap();
    SourceAccessBlockers::try_new(vec![requirement]).unwrap()
}

pub fn unavailable<T>() -> Result<SourceReadOutcome<T>, AgentFailure> {
    Ok(SourceReadOutcome::Unavailable(
        SourceUnavailable::TemporarilyUnavailable,
    ))
}

pub fn needs_user_action<T>(source_id: &str) -> Result<SourceReadOutcome<T>, AgentFailure> {
    Ok(SourceReadOutcome::NeedsUserAction(blocked(source_id)))
}

pub fn request(agent_id: &str) -> BuiltinExpertRequest {
    BuiltinExpertRequest {
        agent_id: agent_id.into(),
        person_id: floe_agent_contract::PersonId(uuid::Uuid::new_v4()),
        task_id: uuid::Uuid::new_v4(),
        invocation_id: uuid::Uuid::new_v4(),
        assignment: "test".into(),
        current_time_unix_ms: chrono::Utc::now().timestamp_millis(),
        context: floe_agent_contract::AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        },
        staged_task_views: vec![],
        context_inputs_available: false,
        max_output_bytes: 16_384,
        deadline: Instant::now() + std::time::Duration::from_secs(30),
        cancellation: Cancellation::default(),
    }
}

pub fn attention_fixture(
    person: floe_agent_contract::PersonId,
) -> (AttentionView, ContextDependency) {
    let now = chrono::Utc::now();
    let view = AttentionView {
        schema_version: floe_agent_contract::AGENT_VERSION,
        view_id: floe_context_contract::ATTENTION_VIEW_ID.into(),
        source_handle: "attention:test".into(),
        observed_at_unix_ms: (now - chrono::Duration::seconds(1)).timestamp_millis(),
        expires_at_unix_ms: (now + chrono::Duration::seconds(60)).timestamp_millis(),
        state: floe_context_contract::AttentionState::Focused,
        confidence_millis: 800,
        evidence_handles: vec!["att:1".into()],
    };
    let source = GrantSourceBinding::try_new(
        person,
        ConnectionId::try_new("attention-connection").unwrap(),
        ConnectorId::try_new("attention.macos").unwrap(),
        ExecutionOwnerId::try_new("device").unwrap(),
        SourceAuthority::new(),
    )
    .unwrap();
    let dependency = ContextDependency::try_new(
        person,
        GrantId::new(),
        GrantAuthority::new(),
        source,
        vec![ResourceHandle::try_new("attention.coarse").unwrap()],
        vec![GrantDataCategory::Derived],
        GrantOperation::Read,
        GrantPurpose::Assistant,
        GrantConsumer::builtin("floe.builtin.focus-attention").unwrap(),
        ProcessingRestriction::LocalOnly,
        ConsumerPolicyAuthority::new(),
        uuid::Uuid::new_v4(),
        vec![7; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        now - chrono::Duration::seconds(1),
        now + chrono::Duration::minutes(30),
    )
    .unwrap();
    (view, dependency)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BuiltinExpertKind;

    fn first_data(output: &BuiltinExpertOutput) -> &str {
        match &output.artifacts[0].parts[0] {
            floe_agent_contract::ArtifactPart::Data { data, .. } => data,
            _ => panic!("expected package data"),
        }
    }

    fn assert_blocked(output: &BuiltinExpertOutput, status: &str, host: &ScriptedHost) {
        assert!(
            first_data(output).contains(status),
            "blocked report must name {status}: {}",
            first_data(output)
        );
        assert!(
            output.artifacts.len() == 1,
            "blocked reports propose no requirement of their own"
        );
        assert_eq!(
            host.model.calls.load(Ordering::SeqCst),
            0,
            "blocked dispatch must never call the model"
        );
    }

    #[tokio::test]
    async fn remote_mandatory_sources_report_blocked_without_a_model_call() {
        for (kind, source_id) in [
            (BuiltinExpertKind::Communication, "floe.source.mail"),
            (BuiltinExpertKind::WorkContext, "floe.source.work-context"),
            (BuiltinExpertKind::LifeLogistics, "floe.source.logistics"),
            (BuiltinExpertKind::Commitments, "floe.source.mail"),
        ] {
            for status in ["needs_user_action", "unavailable"] {
                let host = ScriptedHost::new("unused");
                *host.source.lock().unwrap() = Some(if status == "needs_user_action" {
                    needs_user_action(source_id)
                } else {
                    unavailable()
                });
                let expert_request = request(kind.package_id());
                let output = match kind {
                    BuiltinExpertKind::Communication => {
                        crate::communication::dispatch(&host, &expert_request).await
                    }
                    BuiltinExpertKind::WorkContext => {
                        crate::work_context::dispatch(&host, &expert_request).await
                    }
                    BuiltinExpertKind::LifeLogistics => {
                        crate::life_logistics::dispatch(&host, &expert_request).await
                    }
                    BuiltinExpertKind::Commitments => {
                        crate::commitments::dispatch(&host, &expert_request).await
                    }
                    _ => unreachable!("remote-source experts only"),
                }
                .unwrap();
                assert_blocked(&output, status, &host);
            }
        }
    }

    #[tokio::test]
    async fn personal_mandatory_sources_report_blocked_without_a_model_call() {
        let host = ScriptedHost::new("unused");
        *host.people.lock().unwrap() = Some(needs_user_action("floe.source.contacts"));
        let output = crate::relationships::dispatch(
            &host,
            &request(BuiltinExpertKind::Relationships.package_id()),
        )
        .await
        .unwrap();
        assert_blocked(&output, "needs_user_action", &host);

        let host = ScriptedHost::new("unused");
        *host.wellbeing.lock().unwrap() = Some(unavailable());
        let output =
            crate::wellbeing::dispatch(&host, &request(BuiltinExpertKind::Wellbeing.package_id()))
                .await
                .unwrap();
        assert_blocked(&output, "unavailable", &host);

        let host = ScriptedHost::new("unused");
        *host.attention.lock().unwrap() = Some(needs_user_action("floe.source.attention"));
        let output = crate::focus_attention::dispatch(
            &host,
            &request(BuiltinExpertKind::FocusAttention.package_id()),
        )
        .await
        .unwrap();
        assert_blocked(&output, "needs_user_action", &host);
    }

    #[tokio::test]
    async fn optional_work_blocker_preserves_the_requirement_and_continues() {
        let host = ScriptedHost::new(
            r#"{"summary": "Stay focused.", "recommendation": "protect_focus", "rationale": "Deep work.", "evidence_handles": ["att:1"]}"#,
        );
        let expert_request = request(BuiltinExpertKind::FocusAttention.package_id());
        *host.attention.lock().unwrap() = Some(Ok(SourceReadOutcome::Ready(attention_fixture(
            expert_request.person_id,
        ))));
        *host.calendar.lock().unwrap() = Some(Ok(SourceReadOutcome::Ready(vec![])));
        *host.work.lock().unwrap() = Some(needs_user_action("floe.source.work-context"));
        let output = crate::focus_attention::dispatch(&host, &expert_request)
            .await
            .unwrap();
        // The judgment succeeds over admitted evidence: the optional blocker
        // is preserved by the host, not by failing or by relabelling.
        assert!(
            first_data(&output).contains("protect_focus"),
            "optional blocker must not gate the judgment: {}",
            first_data(&output)
        );
        assert_eq!(host.model.calls.load(Ordering::SeqCst), 1);
    }
}
