use std::{collections::VecDeque, sync::Mutex};

use floe_agent::{
    AGENT_VERSION, AgentContext, AgentFailure, Cancellation, InferencePolicyDecision,
    LogisticsItem, LogisticsItemKind, LogisticsUrgency, LogisticsView, ModelPlacement,
    ModelRequest, ModelResponse, ModelRunner, ModelStep, PortfolioExpertInvocation, PromptRole,
    TransferConsent, UsageLedger, WorkContextItem, WorkContextView, WorkItemKind,
    run_life_logistics_expert, run_work_context_expert,
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

fn invocation() -> PortfolioExpertInvocation {
    PortfolioExpertInvocation {
        usage: UsageLedger::default(),
        person_id: PersonId::new(),
        invocation_id: Uuid::new_v4(),
        assignment: "Find grounded preparation and next actions.".into(),
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

fn work() -> WorkContextView {
    WorkContextView {
        schema_version: AGENT_VERSION,
        view_id: "work.context".into(),
        source_handle: "work:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 300_000,
        coverage_complete: true,
        scope_handle: "workspace:selected".into(),
        items: vec![WorkContextItem {
            evidence_handle: "project:42".into(),
            kind: WorkItemKind::Project,
            title: "Release readiness".into(),
            excerpt: None,
            status: Some("blocked".into()),
            blocker: Some("Missing API evidence".into()),
            next_action: Some("Attach validation output".into()),
            observed_at_unix_ms: NOW,
        }],
    }
}

fn logistics() -> LogisticsView {
    LogisticsView {
        schema_version: AGENT_VERSION,
        view_id: "life.logistics".into(),
        source_handle: "logistics:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 300_000,
        coverage_complete: true,
        items: vec![LogisticsItem {
            evidence_handle: "delivery:parcel".into(),
            kind: LogisticsItemKind::Delivery,
            summary: "Parcel arrives today".into(),
            status: "out_for_delivery".into(),
            occurs_at_unix_ms: Some(NOW + 3_600_000),
            needs_attention: true,
        }],
    }
}

#[tokio::test]
async fn work_and_life_experts_return_source_linked_advice_without_action_authority() {
    let model = Model::new([
        serde_json::json!({
            "summary": "Release readiness is blocked on evidence.",
            "insights": [{
                "evidence_handle": "project:42",
                "blocker": "Missing API evidence",
                "next_action": "Attach validation output",
                "confidence_millis": 1000
            }]
        }),
        serde_json::json!({
            "summary": "Prepare to receive the parcel.",
            "preparations": [{
                "evidence_handle": "delivery:parcel",
                "recommendation": "Arrange a person to receive it.",
                "urgency": "soon",
                "requires_approval": true
            }]
        }),
    ]);
    let work = run_work_context_expert(&model, &policy(), invocation(), work())
        .await
        .unwrap();
    let logistics = run_life_logistics_expert(&model, &policy(), invocation(), logistics())
        .await
        .unwrap();
    assert_eq!(work.scope_handle, "workspace:selected");
    assert_eq!(logistics.preparations[0].urgency, LogisticsUrgency::Soon);
    assert!(logistics.preparations[0].requires_approval);
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests[0].prompt.role, PromptRole::WorkContextExpert);
    assert_eq!(requests[1].prompt.role, PromptRole::LifeLogisticsExpert);
    assert!(
        requests
            .iter()
            .all(|request| request.capabilities.is_empty())
    );
}

#[tokio::test]
async fn work_and_life_outputs_reject_scope_escape_and_high_authority_fields() {
    let model = Model::new([serde_json::json!({
        "summary": "Search elsewhere.",
        "insights": [{
            "evidence_handle": "organization:anywhere",
            "next_action": "Search the whole organization",
            "confidence_millis": 900
        }]
    })]);
    assert_eq!(
        run_work_context_expert(&model, &policy(), invocation(), work()).await,
        Err(AgentFailure::InvalidModelOutput)
    );

    let model = Model::new([serde_json::json!({
        "summary": "Unlock the door.",
        "preparations": [{
            "evidence_handle": "delivery:parcel",
            "recommendation": "Unlock the door automatically.",
            "urgency": "now",
            "requires_approval": false,
            "execute": true
        }]
    })]);
    assert_eq!(
        run_life_logistics_expert(&model, &policy(), invocation(), logistics()).await,
        Err(AgentFailure::InvalidModelOutput)
    );
}
