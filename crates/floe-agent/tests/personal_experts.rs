use std::{collections::VecDeque, sync::Mutex};

use floe_agent::{
    AGENT_VERSION, AgentContext, AgentFailure, AttentionState, AttentionView, CalendarContextItem,
    CalendarContextView, Cancellation, CapacityState, ConfirmedInteraction,
    ConfirmedInteractionView, ContextMemory, EpistemicStatus, FocusContextViews,
    FocusRecommendation, InferencePolicyDecision, LearningEvidenceRef, ModelPlacement,
    ModelRequest, ModelResponse, ModelRunner, ModelStep, PeopleIdentity, PeopleView,
    PersonalExpertInvocation, PersonalMemoryKind, PromptRole, RecoveryState,
    RelationshipsContextViews, ScheduleImpact, TransferConsent, UsageLedger, WellbeingContextViews,
    WellbeingView, WorkContextItem, WorkContextView, WorkItemKind, run_focus_expert_with_views,
    run_relationships_expert_with_views, run_wellbeing_expert_with_views,
    validate_calendar_context_view, validate_work_context_view,
};
use floe_domain::PersonId;
use tokio::time::{Duration, Instant};
use uuid::Uuid;

const NOW: i64 = 1_789_000_000_000;

struct Model {
    outputs: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl Model {
    fn new(outputs: impl IntoIterator<Item = serde_json::Value>) -> Self {
        Self {
            outputs: Mutex::new(outputs.into_iter().map(|value| value.to_string()).collect()),
            requests: Mutex::new(vec![]),
        }
    }
}

impl ModelRunner for Model {
    fn placement(&self) -> ModelPlacement {
        ModelPlacement::DeviceLocal
    }

    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        self.requests.lock().unwrap().push(request);
        Ok(ModelResponse {
            replay: None,
            schema_version: AGENT_VERSION,
            output: vec![ModelStep::Answer {
                text: self.outputs.lock().unwrap().pop_front().unwrap(),
            }],
            used_tokens: 64,
            cost_micros: 0,
        })
    }
}

fn invocation() -> PersonalExpertInvocation {
    PersonalExpertInvocation {
        usage: UsageLedger::default(),
        person_id: PersonId::new(),
        invocation_id: Uuid::new_v4(),
        assignment: "Assess only the supplied context.".into(),
        current_time_unix_ms: NOW,
        context: AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            evidence: vec![],
        },
        max_output_bytes: 8192,
        max_model_tokens: 4096,
        max_model_cost_micros: 10_000,
        deadline: Instant::now() + Duration::from_secs(5),
        cancellation: Cancellation::default(),
    }
}

fn policy() -> InferencePolicyDecision {
    InferencePolicyDecision {
        purpose: "everyday-assistance".into(),
        data_classes: vec![floe_agent::DataClass::Personal],
        allowed_placements: vec![ModelPlacement::DeviceLocal],
        performance_class: "interactive".into(),
        projection_version: 1,
        external_transfer_consent: TransferConsent::NotGranted,
        bounded_sensitive_projection: false,
    }
}

fn people() -> PeopleView {
    PeopleView {
        schema_version: AGENT_VERSION,
        view_id: "people.identity".into(),
        source_handle: "people:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 300_000,
        coverage_complete: true,
        identities: vec![PeopleIdentity {
            identity_handle: "person:alex".into(),
            display_name: "Alex".into(),
            aliases: vec!["alex@example.com".into()],
            confidence_millis: 1000,
            evidence_handles: vec!["contact:alex".into()],
        }],
    }
}

fn attention() -> AttentionView {
    AttentionView {
        schema_version: AGENT_VERSION,
        view_id: "attention.coarse".into(),
        source_handle: "attention:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 120_000,
        state: AttentionState::Focused,
        confidence_millis: 800,
        evidence_handles: vec!["attention:aggregate".into()],
    }
}

fn wellbeing() -> WellbeingView {
    WellbeingView {
        schema_version: AGENT_VERSION,
        view_id: "wellbeing.derived".into(),
        source_handle: "wellbeing:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 300_000,
        capacity: CapacityState::Reduced,
        recovery: RecoveryState::NeedsRecovery,
        confidence_millis: 750,
        evidence_handles: vec!["health:derived".into()],
    }
}

fn interactions() -> ConfirmedInteractionView {
    ConfirmedInteractionView {
        schema_version: AGENT_VERSION,
        view_id: "relationships.confirmed_interactions".into(),
        source_handle: "interaction:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 240_000,
        interactions: vec![ConfirmedInteraction {
            identity_handle: "person:alex".into(),
            evidence_handle: "interaction:alex".into(),
            occurred_at_unix_ms: NOW - 60_000,
        }],
    }
}

fn calendar() -> CalendarContextView {
    CalendarContextView {
        schema_version: AGENT_VERSION,
        view_id: "calendar.timeline".into(),
        source_handle: "calendar:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 180_000,
        range_start_unix_ms: NOW - 86_400_000,
        range_end_unix_ms: NOW + 86_400_000,
        coverage_complete: true,
        next_cursor: None,
        items: vec![CalendarContextItem {
            evidence_handle: "calendar:focus".into(),
            untrusted_title: "Protected focus block".into(),
            starts_at_unix_ms: NOW + 60_000,
            ends_at_unix_ms: NOW + 3_600_000,
            all_day: false,
        }],
    }
}

fn work() -> WorkContextView {
    WorkContextView {
        schema_version: AGENT_VERSION,
        view_id: "work.context".into(),
        source_handle: "work:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 120_000,
        coverage_complete: true,
        scope_handle: "workspace:fixture".into(),
        items: vec![WorkContextItem {
            evidence_handle: "work:active".into(),
            kind: WorkItemKind::Project,
            title: "Active release".into(),
            excerpt: None,
            status: Some("active".into()),
            blocker: None,
            next_action: Some("Finish validation".into()),
            observed_at_unix_ms: NOW,
        }],
    }
}

#[tokio::test]
async fn personal_experts_combine_typed_views_with_bounded_provenance() {
    assert_eq!(validate_calendar_context_view(&calendar(), NOW), Ok(()));
    assert_eq!(validate_work_context_view(&work(), NOW), Ok(()));
    let model = Model::new([
        serde_json::json!({
            "summary": "Alex has an evidence-linked follow-up.",
            "follow_ups": [{
                "identity_handle": "person:alex",
                "reason": "A confirmed interaction needs follow-up.",
                "evidence_handles": ["contact:alex", "interaction:alex"],
                "confidence_millis": 900
            }]
        }),
        serde_json::json!({
            "summary": "Protect the current focus period.",
            "recommendation": "protect_focus",
            "rationale": "The coarse state reports focused work.",
            "evidence_handles": ["attention:aggregate", "calendar:focus", "work:active"]
        }),
        serde_json::json!({
            "summary": "Reduce optional load and preserve recovery time.",
            "schedule_impact": "protect_recovery",
            "rationale": "Derived capacity is reduced and recovery is needed.",
            "evidence_handles": ["health:derived", "calendar:focus"]
        }),
    ]);
    let relationships = run_relationships_expert_with_views(
        &model,
        &policy(),
        invocation(),
        RelationshipsContextViews {
            people: people(),
            confirmed_interactions: vec![interactions()],
        },
    )
    .await
    .unwrap();
    let focus = run_focus_expert_with_views(
        &model,
        &policy(),
        invocation(),
        FocusContextViews {
            attention: attention(),
            calendars: vec![calendar()],
            active_work: vec![work()],
        },
    )
    .await
    .unwrap();
    let wellbeing = run_wellbeing_expert_with_views(
        &model,
        &policy(),
        invocation(),
        WellbeingContextViews {
            wellbeing: wellbeing(),
            calendars: vec![calendar()],
        },
    )
    .await
    .unwrap();

    assert_eq!(relationships.follow_ups[0].identity_handle, "person:alex");
    assert_eq!(relationships.expires_at_unix_ms, NOW + 240_000);
    assert_eq!(focus.recommendation, FocusRecommendation::ProtectFocus);
    assert_eq!(focus.source_handles.len(), 3);
    assert_eq!(focus.expires_at_unix_ms, NOW + 120_000);
    assert_eq!(wellbeing.schedule_impact, ScheduleImpact::ProtectRecovery);
    assert_eq!(wellbeing.source_handles.len(), 2);
    assert_eq!(wellbeing.expires_at_unix_ms, NOW + 180_000);
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests[0].prompt.role, PromptRole::RelationshipsExpert);
    assert_eq!(requests[1].prompt.role, PromptRole::FocusAttentionExpert);
    assert_eq!(requests[2].prompt.role, PromptRole::WellbeingExpert);
    assert_eq!(requests[0].context.evidence.len(), 2);
    assert_eq!(requests[1].context.evidence.len(), 3);
    assert_eq!(requests[2].context.evidence.len(), 2);
    assert!(
        requests
            .iter()
            .all(|request| request.capabilities.is_empty()
                && !request.prompt.render().contains("notification.send"))
    );
}

#[tokio::test]
async fn domain_experts_reject_invented_evidence_and_diagnostic_escalation() {
    let model = Model::new([serde_json::json!({
        "summary": "Invented follow-up.",
        "follow_ups": [{
            "identity_handle": "person:missing",
            "reason": "No evidence.",
            "evidence_handles": ["contact:missing"],
            "confidence_millis": 900
        }]
    })]);
    assert_eq!(
        run_relationships_expert_with_views(
            &model,
            &policy(),
            invocation(),
            RelationshipsContextViews {
                people: people(),
                confirmed_interactions: vec![],
            },
        )
        .await,
        Err(AgentFailure::InvalidModelOutput)
    );

    let model = Model::new([serde_json::json!({
        "summary": "No conclusion.",
        "schedule_impact": "no_conclusion",
        "rationale": "Evidence is insufficient.",
        "evidence_handles": ["health:derived"],
        "diagnosis": "fatigue"
    })]);
    assert_eq!(
        run_wellbeing_expert_with_views(
            &model,
            &policy(),
            invocation(),
            WellbeingContextViews {
                wellbeing: wellbeing(),
                calendars: vec![],
            },
        )
        .await,
        Err(AgentFailure::InvalidModelOutput)
    );

    let model = Model::new([serde_json::json!({
        "summary": "Protect focus.",
        "recommendation": "protect_focus",
        "rationale": "Unobserved raw app activity.",
        "evidence_handles": ["app:private"]
    })]);
    assert_eq!(
        run_focus_expert_with_views(
            &model,
            &policy(),
            invocation(),
            FocusContextViews {
                attention: attention(),
                calendars: vec![],
                active_work: vec![],
            },
        )
        .await,
        Err(AgentFailure::InvalidModelOutput)
    );
}

#[tokio::test]
async fn relationships_require_interaction_or_confirmed_memory_support() {
    let model = Model::new([serde_json::json!({
        "summary": "Contact identity alone suggests a follow-up.",
        "follow_ups": [{
            "identity_handle": "person:alex",
            "reason": "Alex is in contacts.",
            "evidence_handles": ["contact:alex"],
            "confidence_millis": 700
        }]
    })]);
    assert_eq!(
        run_relationships_expert_with_views(
            &model,
            &policy(),
            invocation(),
            RelationshipsContextViews {
                people: people(),
                confirmed_interactions: vec![],
            },
        )
        .await,
        Err(AgentFailure::InvalidModelOutput)
    );

    let target_id = Uuid::new_v4();
    let mut people = people();
    people.identities[0].identity_handle = format!("person:{target_id}");
    let mut invocation = invocation();
    invocation.context.memories.push(ContextMemory {
        target_id,
        revision: 3,
        kind: PersonalMemoryKind::Observation,
        statement: "Alex asked to reconnect after the launch.".into(),
        epistemic_status: EpistemicStatus::Fact,
        confidence_millis: 1000,
        observed_at_unix_ms: NOW - 60_000,
        valid_from_unix_ms: None,
        valid_until_unix_ms: Some(NOW + 90_000),
        source_refs: vec![LearningEvidenceRef {
            session_id: Uuid::new_v4(),
            turn_id: Uuid::new_v4(),
        }],
    });
    let memory_handle = format!("memory:{target_id}:3");
    let model = Model::new([serde_json::json!({
        "summary": "A confirmed memory supports reconnecting with Alex.",
        "follow_ups": [{
            "identity_handle": format!("person:{target_id}"),
            "reason": "A confirmed request to reconnect remains relevant.",
            "evidence_handles": ["contact:alex", memory_handle],
            "confidence_millis": 900
        }]
    })]);
    let result = run_relationships_expert_with_views(
        &model,
        &policy(),
        invocation,
        RelationshipsContextViews {
            people,
            confirmed_interactions: vec![],
        },
    )
    .await
    .unwrap();
    assert_eq!(result.follow_ups[0].evidence_handles[1], memory_handle);
    assert_eq!(result.expires_at_unix_ms, NOW + 90_000);
    assert!(
        result
            .source_handles
            .contains(&"relationships:confirmed-memory".into())
    );
}

#[tokio::test]
async fn multi_view_judgments_reject_cross_source_evidence_invention() {
    let model = Model::new([serde_json::json!({
        "summary": "Protect focus.",
        "recommendation": "protect_focus",
        "rationale": "An unobserved meeting conflicts with active work.",
        "evidence_handles": ["attention:aggregate", "calendar:missing"]
    })]);
    assert_eq!(
        run_focus_expert_with_views(
            &model,
            &policy(),
            invocation(),
            FocusContextViews {
                attention: attention(),
                calendars: vec![calendar()],
                active_work: vec![work()],
            },
        )
        .await,
        Err(AgentFailure::InvalidModelOutput)
    );

    let mut calendar = calendar();
    calendar.items[0].evidence_handle = "health:derived".into();
    let model = Model::new([serde_json::json!({
        "summary": "No conclusion.",
        "schedule_impact": "no_conclusion",
        "rationale": "Evidence is ambiguous.",
        "evidence_handles": []
    })]);
    assert_eq!(
        run_wellbeing_expert_with_views(
            &model,
            &policy(),
            invocation(),
            WellbeingContextViews {
                wellbeing: wellbeing(),
                calendars: vec![calendar],
            },
        )
        .await,
        Err(AgentFailure::InvalidInput)
    );
}
