//! Life logistics preparation and urgency.

use floe_kernel::PersonId;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::time::Instant;
use uuid::Uuid;

use crate::prompts::{life_logistics_expert_prompt, work_context_expert_prompt};
use floe_context::{LogisticsView, WorkContextView, logistics_context_evidence, validate_logistics_view, validate_work_context_view, work_context_evidence};
use floe_agent_contract::{AgentFailure, SessionProtection};
use floe_context::{AgentContext, InferencePolicyDecision};
use floe_kernel::AGENT_VERSION;
use floe_conversation::{AgentMessage, ModelRequest, ModelRunner, ModelStep};
use floe_conversation::{UsageLedger, generate_with_recovery};
use floe_knowledge::prompts::{PromptAssembly};

pub enum LogisticsUrgency {
    Now,
    Soon,
    Later,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogisticsPreparation {
    pub evidence_handle: String,
    pub recommendation: String,
    pub urgency: LogisticsUrgency,
    pub requires_approval: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifeLogisticsExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub preparations: Vec<LogisticsPreparation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogisticsOutput {
    summary: String,
    preparations: Vec<LogisticsPreparation>,
}

pub async fn run_life_logistics_expert<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PortfolioExpertInvocation,
    view: LogisticsView,
) -> Result<LifeLogisticsExpertResult, AgentFailure> {
    validate_logistics_view(&view, invocation.current_time_unix_ms)?;
    let output: LogisticsOutput = run_model(
        model,
        policy,
        &invocation,
        logistics_context_evidence(&view)?,
        life_logistics_expert_prompt(),
    )
    .await?;
    validate_summary(&output.summary)?;
    if output.preparations.len() > 16 {
        return Err(AgentFailure::BudgetExceeded);
    }
    for (index, preparation) in output.preparations.iter().enumerate() {
        if !view
            .items
            .iter()
            .any(|item| item.evidence_handle == preparation.evidence_handle)
            || !valid_text(&preparation.recommendation, 512)
            || output.preparations[..index]
                .iter()
                .any(|other| other.evidence_handle == preparation.evidence_handle)
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    }
    Ok(LifeLogisticsExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: view.source_handle,
        expires_at_unix_ms: view.expires_at_unix_ms,
        summary: output.summary,
        preparations: output.preparations,
    })
}

async fn run_model<Output: DeserializeOwned, Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: &PortfolioExpertInvocation,
    evidence: floe_context::ContextEvidence,
    prompt: PromptAssembly,
) -> Result<Output, AgentFailure> {
    if !valid_text(&invocation.assignment, 2048)
        || invocation.max_output_bytes == 0
        || invocation.max_model_tokens == 0
        || invocation.deadline <= Instant::now()
        || invocation.cancellation.is_cancelled()
    {
        return Err(AgentFailure::InvalidInput);
    }
    let mut context = invocation.context.clone();
    context.evidence.push(evidence);
    policy.authorize(
        model.placement(),
        SessionProtection::Encrypted,
        &context,
        u64::try_from(invocation.current_time_unix_ms).map_err(|_| AgentFailure::InvalidInput)?,
    )?;
    let turn_id = Uuid::new_v4();
    let response = generate_with_recovery(
        model,
        ModelRequest {
            usage: invocation.usage.clone(),
            replay: vec![],
            schema_version: AGENT_VERSION,
            prompt,
            person_id: invocation.person_id,
            session_id: invocation.invocation_id,
            turn_id,
            policy: policy.clone(),
            context,
            messages: vec![AgentMessage::User {
                turn_id,
                text: invocation.assignment.clone(),
            }],
            capabilities: vec![],
            active_agents: vec![],
            remaining_tokens: invocation.max_model_tokens,
            remaining_cost_micros: invocation.max_model_cost_micros,
            max_output_bytes: invocation.max_output_bytes.min(8192),
            deadline: invocation.deadline,
            cancellation: invocation.cancellation.clone(),
        },
    )
    .await?;
    if response.schema_version != AGENT_VERSION
        || response.used_tokens > invocation.max_model_tokens
        || response.cost_micros > invocation.max_model_cost_micros
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    let [ModelStep::Answer { text }] = response.output.as_slice() else {
        return Err(AgentFailure::InvalidModelOutput);
    };
    if text.len() > invocation.max_output_bytes.min(8192) {
        return Err(AgentFailure::BudgetExceeded);
    }
    serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)
}

fn validate_summary(value: &str) -> Result<(), AgentFailure> {
    if valid_text(value, 2048) {
        Ok(())
    } else {
        Err(AgentFailure::InvalidModelOutput)
    }
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum
}
