use std::time::Duration;

use floe_agent_contract::{DataClass, ModelPlacement, TransferConsent};
use floe_context::{AgentContext, InferencePolicyDecision};
use floe_conversation::prompts::manager_prompt;
use floe_execution::Cancellation;
use floe_inference::{ModelTransport, ModelTransportRequest};
use floe_provider_adapters::models::{FoundationModelRunner, LocalModelAvailability};
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
    let transport = FoundationModelRunner::synthetic();
    let availability_of = transport.availability();
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
            purpose: "synthetic-local-model-smoke",
            response_contract: floe_conversation::MANAGER_OUTPUT_CONTRACT,
            correction: None,
            prompt: prompt.clone(),
            conversation,
            agent_context: &context,
            catalog: &Default::default(),
            active_experts: &[],
            authorized_history_dependencies: &[],
            input_data_classes: vec![DataClass::Synthetic],
            max_output_bytes: 16384,
        })
        .unwrap();
    let request = ModelTransportRequest {
        attempt_id: Uuid::new_v4(),
        envelope: projection.envelope,
        replay: vec![],
        schema_version: 1,
        prompt,
        policy: InferencePolicyDecision {
            purpose: "synthetic-local-model-smoke".into(),
            data_classes: vec![DataClass::Synthetic],
            allowed_placements: vec![ModelPlacement::DeviceLocal],
            performance_class: "fast".into(),
            projection_version: 1,
            external_transfer_consent: TransferConsent::NotGranted,
            bounded_sensitive_projection: false,
        },
        context,
        capabilities: vec![],
        active_agents: vec![],
        remaining_tokens: 4096,
        remaining_cost_micros: 0,
        max_output_bytes: 16384,
        deadline: tokio::time::Instant::now() + Duration::from_secs(30),
        cancellation: Cancellation::default(),
    };
    match transport.generate(request).await {
        Ok(response) => {
            println!(
                "{}",
                json!({"schema_version":1,"status":"passed","output":response.output,
                "reserved_tokens":response.used_tokens,"cost_micros":response.cost_micros,"personal_data":false})
            );
            std::process::ExitCode::SUCCESS
        }
        Err(failure) => {
            println!(
                "{}",
                json!({"schema_version":1,"failure":failure,"personal_data":false})
            );
            std::process::ExitCode::FAILURE
        }
    }
}
