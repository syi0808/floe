use std::time::Duration;

use floe_agent_contract::{
    DataClass, ModelCapabilities, ModelPlanRequest, ModelPort, ModelProjectionOutcome,
    ProcessingBoundary,
};
use floe_context::AgentContext;
use floe_conversation::prompts::manager_prompt;
use floe_execution::Cancellation;
use serde_json::json;
use uuid::Uuid;

#[path = "local_model_smoke/support.rs"]
mod support;

#[path = "local_model_smoke/learner.rs"]
mod learner;

#[path = "local_model_smoke/manager_guidance.rs"]
mod manager_guidance;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments == ["--exercise-manager-guidance"] {
        return manager_guidance::run(manager_guidance::Mode::Foundation).await;
    }
    if arguments == ["--exercise-manager-guidance-server"] {
        return manager_guidance::run(manager_guidance::Mode::Server).await;
    }
    let optional_memory = arguments == ["--exercise-optional-memory"];
    let learner_mode = arguments.first().map(String::as_str);
    let learner = learner_mode == Some("--exercise-learner");
    let learner_expiry = learner_mode == Some("--exercise-learner-expiry");
    let learner_profile = if learner || learner_expiry {
        match arguments.as_slice() {
            [_] => None,
            [_, flag, path] if flag == "--profile" => Some(std::path::PathBuf::from(path)),
            _ => {
                eprintln!(
                    "Learner modes accept --profile /absolute/path/to/an/isolated/prepared/people/PERSON/floe.db"
                );
                return std::process::ExitCode::FAILURE;
            }
        }
    } else {
        None
    };
    if arguments != ["--availability"]
        && arguments != ["--exercise"]
        && !optional_memory
        && !learner
        && !learner_expiry
    {
        eprintln!(
            "Use --availability, --exercise, --exercise-optional-memory, --exercise-learner [--profile PATH], --exercise-learner-expiry [--profile PATH], --exercise-manager-guidance, or --exercise-manager-guidance-server (synthetic only). Learner requires a fresh isolated prepared profile; it does not create or replace keys."
        );
        return std::process::ExitCode::FAILURE;
    }
    if learner || learner_expiry {
        return match learner::run(learner_expiry, learner_profile).await {
            Ok(result) => {
                println!("{result}");
                if result["status"] == "passed" {
                    std::process::ExitCode::SUCCESS
                } else {
                    std::process::ExitCode::FAILURE
                }
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
    let profile = match support::SyntheticProfile::create().await {
        Ok(profile) => profile,
        Err(failure) => {
            println!(
                "{}",
                json!({"schema_version":1,"failure":failure,"personal_data":false})
            );
            return std::process::ExitCode::FAILURE;
        }
    };
    let service = profile.model();
    let principal = profile.actor.person_id.to_string();
    let device_id = profile.actor.device_id.clone();
    let prepared = match service
        .prepare(
            ModelPlanRequest {
                principal: principal.clone(),
                device_id: device_id.clone(),
                purpose: "everyday_assistance".into(),
                consumer: "conversation.root".into(),
                required_capabilities: ModelCapabilities::chat(),
            },
            &scope,
        )
        .await
    {
        Ok(prepared) => prepared,
        Err(failure) => {
            println!(
                "{}",
                json!({"schema_version":1,"failure":failure,"personal_data":false})
            );
            return std::process::ExitCode::FAILURE;
        }
    };
    if prepared.plan().boundary != ProcessingBoundary::Device {
        println!(
            "{}",
            json!({"status":"UNVERIFIED","reason":"local_fallback_not_selected","personal_data":false})
        );
        return std::process::ExitCode::FAILURE;
    }
    println!(
        "{}",
        json!({"schema_version":1,"availability":"available","boundary":"device","personal_data":false})
    );
    if arguments == ["--availability"] {
        return std::process::ExitCode::SUCCESS;
    }
    let projection =
        floe_context::assemble_context_projection(floe_context::ContextProjectionInput {
            role: floe_context::ContextProjectionRole::Manager,
            plan: prepared.plan(),
            projection_operation_id: Uuid::new_v4(),
            purpose: "everyday_assistance",
            response_contract: floe_conversation::MANAGER_OUTPUT_CONTRACT,
            output_format: &floe_agent_contract::ModelOutputFormat::Text,
            correction: None,
            prompt: prompt.clone(),
            conversation,
            agent_context: &context,
            catalog: &floe_agent_contract::AllowedCatalog {
                cards: vec![],
                tools: vec![],
                revision: 1,
            },
            expert_environment: None,
            authorized_history_dependencies: &[],
            input_data_classes: vec![DataClass::Synthetic],
            max_output_bytes: 16384,
        })
        .unwrap();
    let projection = match projection {
        ModelProjectionOutcome::Ready(projection) => projection,
        ModelProjectionOutcome::NeedsSourceReview(_) => return std::process::ExitCode::FAILURE,
    };
    let journal = match support::DiagnosticJournal::new() {
        Ok(journal) => journal,
        Err(_) => return std::process::ExitCode::FAILURE,
    };
    let request = floe_agent_contract::ModelRequest {
        reservation_ceiling: floe_execution::budget::ModelReservationCeiling::for_lease(
            scope.budget(),
        ),
        attempt_id: Uuid::new_v4(),
        principal,
        device_id,
        projection,
        catalog: floe_agent_contract::AllowedCatalog {
            cards: vec![],
            tools: vec![],
            revision: 1,
        },
        purpose: "everyday_assistance".into(),
        consumer: "conversation.root".into(),
        replay: vec![],
    };
    match support::invoke(prepared.as_ref(), request, &scope, &journal).await {
        Ok(response) => {
            println!(
                "{}",
                json!({"schema_version":1,"status":"passed","output":response.steps,
                "charged_tokens":response.usage.tokens,"cost_micros":response.usage.cost_micros,
                "accounting":response.accounting,"journal":journal.path(),"personal_data":false})
            );
            std::process::ExitCode::SUCCESS
        }
        Err(failure) => {
            println!(
                "{}",
                json!({"schema_version":1,"failure":failure,"journal":journal.path(),"personal_data":false})
            );
            std::process::ExitCode::FAILURE
        }
    }
}
