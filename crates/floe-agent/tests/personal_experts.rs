use std::{collections::VecDeque, sync::Mutex};

use floe_agent::{
    AGENT_VERSION, AgentContext, AgentFailure, AttentionState, AttentionView, Cancellation,
    CapacityState, FocusRecommendation, InferencePolicyDecision, ModelPlacement, ModelRequest,
    ModelResponse, ModelRunner, ModelStep, PeopleIdentity, PeopleView, PersonalExpertInvocation,
    PromptRole, RecoveryState, ScheduleImpact, TransferConsent, UsageLedger, WellbeingView,
    run_focus_expert, run_relationships_expert, run_wellbeing_expert,
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
        expires_at_unix_ms: NOW + 300_000,
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

#[tokio::test]
async fn relationships_focus_and_wellbeing_use_isolated_bounded_judgments() {
    let model = Model::new([
        serde_json::json!({
            "summary": "Alex has an evidence-linked follow-up.",
            "follow_ups": [{
                "identity_handle": "person:alex",
                "reason": "A confirmed interaction needs follow-up.",
                "evidence_handles": ["contact:alex"],
                "confidence_millis": 900
            }]
        }),
        serde_json::json!({
            "summary": "Protect the current focus period.",
            "recommendation": "protect_focus",
            "rationale": "The coarse state reports focused work.",
            "evidence_handles": ["attention:aggregate"]
        }),
        serde_json::json!({
            "summary": "Reduce optional load and preserve recovery time.",
            "schedule_impact": "protect_recovery",
            "rationale": "Derived capacity is reduced and recovery is needed.",
            "evidence_handles": ["health:derived"]
        }),
    ]);
    let relationships = run_relationships_expert(&model, &policy(), invocation(), people())
        .await
        .unwrap();
    let focus = run_focus_expert(&model, &policy(), invocation(), attention())
        .await
        .unwrap();
    let wellbeing = run_wellbeing_expert(&model, &policy(), invocation(), wellbeing())
        .await
        .unwrap();

    assert_eq!(relationships.follow_ups[0].identity_handle, "person:alex");
    assert_eq!(focus.recommendation, FocusRecommendation::ProtectFocus);
    assert_eq!(wellbeing.schedule_impact, ScheduleImpact::ProtectRecovery);
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests[0].prompt.role, PromptRole::RelationshipsExpert);
    assert_eq!(requests[1].prompt.role, PromptRole::FocusAttentionExpert);
    assert_eq!(requests[2].prompt.role, PromptRole::WellbeingExpert);
    assert!(requests.iter().all(|request| {
        request.capabilities.is_empty()
            && request.context.evidence.len() == 1
            && !request.prompt.render().contains("notification.send")
    }));
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
        run_relationships_expert(&model, &policy(), invocation(), people()).await,
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
        run_wellbeing_expert(&model, &policy(), invocation(), wellbeing()).await,
        Err(AgentFailure::InvalidModelOutput)
    );

    let model = Model::new([serde_json::json!({
        "summary": "Protect focus.",
        "recommendation": "protect_focus",
        "rationale": "Unobserved raw app activity.",
        "evidence_handles": ["app:private"]
    })]);
    assert_eq!(
        run_focus_expert(&model, &policy(), invocation(), attention()).await,
        Err(AgentFailure::InvalidModelOutput)
    );
}
