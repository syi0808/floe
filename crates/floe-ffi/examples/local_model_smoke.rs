use std::time::Duration;

use floe_agent::{
    AGENT_SYSTEM_INSTRUCTIONS, AgentContext, AgentMessage, Cancellation, DataClass,
    InferencePolicyDecision, ModelPlacement, ModelRequest, ModelRunner, TransferConsent,
};
use floe_domain::PersonId;
use floe_ffi::local_model::{FoundationModelRunner, LocalModelAvailability};
use serde_json::json;
use uuid::Uuid;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments != ["--availability"] && arguments != ["--exercise"] {
        eprintln!("Use --availability (no generation) or --exercise (one synthetic request)");
        return std::process::ExitCode::FAILURE;
    }
    let model = FoundationModelRunner::synthetic();
    let availability = match model.availability() {
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
    let turn_id = Uuid::new_v4();
    let request = ModelRequest {
        usage: Default::default(),
        replay: vec![],
        schema_version: 1,
        system_instructions: AGENT_SYSTEM_INSTRUCTIONS,
        person_id: PersonId::new(),
        session_id: Uuid::new_v4(),
        turn_id,
        policy: InferencePolicyDecision {
            purpose: "synthetic-local-model-smoke".into(),
            data_classes: vec![DataClass::Synthetic],
            allowed_placements: vec![ModelPlacement::DeviceLocal],
            performance_class: "fast".into(),
            projection_version: 1,
            external_transfer_consent: TransferConsent::NotGranted,
            bounded_sensitive_projection: false,
        },
        context: AgentContext { projection_version: 1, evidence: vec![] },
        messages: vec![AgentMessage::User {
            turn_id,
            text: "This is a fictional test, not my calendar. A fictional person has a meeting at 14:00 and a free hour at 11:00. Suggest a preparation time in one sentence, explicitly noting that this is synthetic data. Do not call a tool.".into(),
        }],
        capabilities: vec![],
        remaining_tokens: 4096,
        remaining_cost_micros: 0,
        max_output_bytes: 16384,
        deadline: tokio::time::Instant::now() + Duration::from_secs(30),
        cancellation: Cancellation::default(),
    };
    match model.generate(request).await {
        Ok(response) => {
            println!(
                "{}",
                json!({"schema_version":1,"status":"passed","step":response.step,
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
