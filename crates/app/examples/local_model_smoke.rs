use std::time::Duration;

use floe_agent_contract::{DataClass, ModelCallOutcome, ModelPort};
use floe_context::AgentContext;
use floe_conversation::prompts::manager_prompt;
use floe_execution::Cancellation;
use floe_provider_adapters::models::{FoundationModelProvider, LocalModelAvailability};
use serde_json::json;
use uuid::Uuid;

#[path = "local_model_smoke/learner.rs"]
mod learner;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let optional_memory = arguments == ["--exercise-optional-memory"];
    let learner = arguments == ["--exercise-learner"];
    let learner_expiry = arguments == ["--exercise-learner-expiry"];
    if arguments != ["--availability"]
        && arguments != ["--exercise"]
        && !optional_memory
        && !learner
        && !learner_expiry
    {
        eprintln!(
            "Use --availability, --exercise, --exercise-optional-memory, --exercise-learner, or --exercise-learner-expiry (synthetic only)"
        );
        return std::process::ExitCode::FAILURE;
    }
    let provider = FoundationModelProvider::synthetic();
    let availability_of = provider.availability();
    let availability = match availability_of {
        Ok(availability) => availability,
        Err(failure) => {
            println!(
                "{}",
                json!({"schema_version":1,"failure":failure,"personal_data":false})
            );
            return std::process::ExitCode::FAILURE;
        }
    };
    println!(
        "{}",
        json!({"schema_version":1,"availability":format!("{availability:?}"),"profile":"apple_foundation_models_26_on_device","personal_data":false})
    );
    if arguments == ["--availability"] {
        return std::process::ExitCode::SUCCESS;
    }
    if availability != LocalModelAvailability::Available {
        return std::process::ExitCode::FAILURE;
    }
    if learner || learner_expiry {
        return match learner::run(learner_expiry).await {
            Ok(result) => {
                println!("{result}");
                std::process::ExitCode::SUCCESS
            }
            Err(failure) => {
                println!(
                    "{}",
                    json!({"schema_version":1,"failure":failure,"personal_data":false})
                );
                std::process::ExitCode::FAILURE
            }
        };
    }
    let turn_id = Uuid::new_v4();
    let prompt = manager_prompt(None).unwrap();
    let context = AgentContext {
        projection_version: 1,
        persona: None,
        optional_context_issues: if optional_memory {
            vec![floe_agent_contract::ContextIssue {
                source: floe_agent_contract::ContextSource::Memory,
                reason: floe_agent_contract::ContextIssueReason::Unavailable,
            }]
        } else {
            vec![]
        },
        memories: vec![],
        evidence: vec![],
    };
    let conversation = floe_agent_contract::ModelConversation {
        history: vec![],
        current_turn: vec![floe_agent_contract::ModelConversationEntry::User {
            message_id: turn_id,
            text: if optional_memory {
                "This is a synthetic test about a fictional person, not my personal data. What meeting time does saved memory say the fictional person prefers? If you cannot determine that, can you still suggest one general preparation tip?"
            } else {
                "This is a fictional test, not my calendar. A fictional person has a meeting at 14:00 and a free hour at 11:00. Suggest a preparation time in one sentence, explicitly noting that this is synthetic data. Do not call a tool."
            }.into(),
        }],
    };
    let projection =
        floe_context::assemble_context_projection(floe_context::ContextProjectionInput {
            role: floe_context::ContextProjectionRole::Manager,
            purpose: floe_inference::CANONICAL_MODEL_PURPOSE,
            response_contract: floe_conversation::MANAGER_OUTPUT_CONTRACT,
            correction: None,
            prompt: prompt.clone(),
            conversation,
            agent_context: &context,
            catalog: &floe_agent_contract::AllowedCatalog {
                cards: vec![],
                tools: vec![],
                revision: 1,
            },
            active_experts: &[],
            authorized_history_dependencies: &[],
            input_data_classes: vec![DataClass::Synthetic],
            max_output_bytes: 16384,
        })
        .unwrap();
    let ledger = floe_execution::budget::BudgetLedger::new(
        floe_execution::budget::BudgetConfig::new(8192, 1_000_000),
        floe_execution::budget::ModelUsage::default(),
    );
    let scope = floe_execution::ExecutionScope::root(
        Cancellation::default(),
        tokio::time::Instant::now() + Duration::from_secs(30),
        ledger.work_lease(),
        floe_kernel::TraceContext::new(turn_id),
    );
    let service = floe_inference::InferenceService::new(
        provider,
        learner::SmokeResolver,
        learner::SmokeAuthority,
    );
    let request = floe_agent_contract::ModelRequest {
        attempt_id: Uuid::new_v4(),
        principal: floe_kernel::PersonId::new().to_string(),
        projection,
        catalog: floe_agent_contract::AllowedCatalog {
            cards: vec![],
            tools: vec![],
            revision: 1,
        },
        purpose: floe_inference::CANONICAL_MODEL_PURPOSE.into(),
        consumer: floe_inference::CANONICAL_MODEL_CONSUMER.into(),
        preferred_profile_id: Some("foundation-device".into()),
        replay: vec![],
        lineage: None,
    };
    match service.generate(request, &scope).await {
        Ok(ModelCallOutcome::Ready(response)) => {
            println!(
                "{}",
                json!({"schema_version":1,"status":"passed","output":response.steps,
                "reserved_tokens":response.usage.tokens,"cost_micros":response.usage.cost_micros,"personal_data":false})
            );
            std::process::ExitCode::SUCCESS
        }
        Ok(ModelCallOutcome::NeedsUserAction(_)) => std::process::ExitCode::FAILURE,
        Err(failure) => {
            println!(
                "{}",
                json!({"schema_version":1,"failure":failure,"personal_data":false})
            );
            std::process::ExitCode::FAILURE
        }
    }
}
