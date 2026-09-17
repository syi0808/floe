use std::{collections::VecDeque, sync::Mutex};

use floe_agent_contract::{AgentFailure, ModelPlacement, TransferConsent};
use floe_agent_contract::{
    AgentContext, BoxFuture, ExpertModel, ExpertModelAnswer, ExpertModelCall,
    InferencePolicyDecision,
};
use floe_agent_contract::AGENT_VERSION;
use floe_execution::{Cancellation};
use floe_context_contract::{LogisticsView, WorkContextItem, WorkContextView, WorkItemKind};
use floe_experts_builtin::life_logistics::{LogisticsUrgency, run_life_logistics_expert};
use floe_experts_builtin::work_context::{run_work_context_expert};
use floe_experts_builtin::{PortfolioExpertInvocation};
use floe_context_contract::{LogisticsItem, LogisticsItemKind};
use floe_agent_contract::prompts::PromptRole;
use floe_agent_contract::PersonId;
use tokio::time::{Duration, Instant};
use uuid::Uuid;

const NOW: i64 = 1_789_000_000_000;

struct Model {
    outputs: Mutex<VecDeque<String>>,
    calls: Mutex<Vec<ExpertModelCall>>,
}

impl Model {
    fn new(outputs: impl IntoIterator<Item = serde_json::Value>) -> Self {
        Self {
            outputs: Mutex::new(outputs.into_iter().map(|value| value.to_string()).collect()),
            calls: Mutex::new(vec![]),
        }
    }
}

impl ExpertModel for Model {
    fn placement(&self) -> ModelPlacement {
        ModelPlacement::DeviceLocal
    }

    fn answer<'a>(
        &'a self,
        call: ExpertModelCall,
    ) -> BoxFuture<'a, Result<ExpertModelAnswer, AgentFailure>> {
        let answer = self.outputs.lock().unwrap().pop_front().unwrap();
        self.calls.lock().unwrap().push(call);
        Box::pin(async move {
            Ok(ExpertModelAnswer {
                schema_version: AGENT_VERSION,
                answer,
                used_tokens: 64,
                cost_micros: 0,
            })
        })
    }
}

fn invocation() -> PortfolioExpertInvocation {
    PortfolioExpertInvocation {
        person_id: PersonId::new(),
        invocation_id: Uuid::new_v4(),
        assignment: "Find grounded preparation and next actions.".into(),
        current_time_unix_ms: NOW,
        context: AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
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
        data_classes: vec![floe_agent_contract::DataClass::Personal],
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
    let calls = model.calls.lock().unwrap();
    assert_eq!(calls[0].prompt.role, PromptRole::WorkContextExpert);
    assert_eq!(calls[1].prompt.role, PromptRole::LifeLogisticsExpert);
    assert!(calls.iter().all(|call| call.assignment.trim().len() > 0));
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

#[tokio::test]
async fn an_expert_refuses_an_answer_that_overspends_or_overflows_its_bound() {
    struct Overspending {
        used_tokens: u64,
        cost_micros: u64,
        answer: String,
    }

    impl ExpertModel for Overspending {
        fn placement(&self) -> ModelPlacement {
            ModelPlacement::DeviceLocal
        }

        fn answer<'a>(
            &'a self,
            _: ExpertModelCall,
        ) -> BoxFuture<'a, Result<ExpertModelAnswer, AgentFailure>> {
            Box::pin(async move {
                Ok(ExpertModelAnswer {
                    schema_version: AGENT_VERSION,
                    answer: self.answer.clone(),
                    used_tokens: self.used_tokens,
                    cost_micros: self.cost_micros,
                })
            })
        }
    }

    let sound = serde_json::json!({
        "summary": "Prepared.",
        "scope_handle": "workspace:selected",
        "expires_at_unix_ms": NOW + 60_000,
        "source_handles": ["work:selected"],
        "preparations": [],
    })
    .to_string();
    let budget = invocation();
    for (used_tokens, cost_micros) in [
        (budget.max_model_tokens + 1, 0),
        (0, budget.max_model_cost_micros + 1),
    ] {
        let model = Overspending {
            used_tokens,
            cost_micros,
            answer: sound.clone(),
        };
        assert_eq!(
            run_work_context_expert(&model, &policy(), invocation(), work())
                .await
                .unwrap_err(),
            AgentFailure::BudgetExceeded
        );
    }

    // An answer inside the budget but past the Expert's own output bound is
    // refused too, before anything tries to read it.
    let mut invocation = invocation();
    invocation.max_output_bytes = 8;
    let model = Overspending {
        used_tokens: 1,
        cost_micros: 1,
        answer: sound,
    };
    assert_eq!(
        run_work_context_expert(&model, &policy(), invocation, work())
            .await
            .unwrap_err(),
        AgentFailure::BudgetExceeded
    );
}
