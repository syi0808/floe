//! Work context insights.

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

pub struct PortfolioExpertInvocation {
    pub usage: UsageLedger,
    pub person_id: PersonId,
    pub invocation_id: Uuid,
    pub assignment: String,
    pub current_time_unix_ms: i64,
    pub context: AgentContext,
    pub max_output_bytes: usize,
    pub max_model_tokens: u64,
    pub max_model_cost_micros: u64,
    pub deadline: Instant,
    pub cancellation: floe_execution::Cancellation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkInsight {
    pub evidence_handle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    pub confidence_millis: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkContextExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    pub scope_handle: String,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub insights: Vec<WorkInsight>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
struct WorkOutput {
    summary: String,
    insights: Vec<WorkInsight>,
}

pub async fn run_work_context_expert<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PortfolioExpertInvocation,
    view: WorkContextView,
) -> Result<WorkContextExpertResult, AgentFailure> {
    validate_work_context_view(&view, invocation.current_time_unix_ms)?;
    let output: WorkOutput = run_model(
        model,
        policy,
        &invocation,
        work_context_evidence(&view)?,
        work_context_expert_prompt(),
    )
    .await?;
    validate_summary(&output.summary)?;
    if output.insights.len() > 16 {
        return Err(AgentFailure::BudgetExceeded);
    }
    for (index, insight) in output.insights.iter().enumerate() {
        if !view
            .items
            .iter()
            .any(|item| item.evidence_handle == insight.evidence_handle)
            || insight.blocker.is_none() && insight.next_action.is_none()
            || insight
                .blocker
                .as_ref()
                .is_some_and(|value| !valid_text(value, 512))
            || insight
                .next_action
                .as_ref()
                .is_some_and(|value| !valid_text(value, 512))
            || insight.confidence_millis == 0
            || insight.confidence_millis > 1000
            || output.insights[..index]
                .iter()
                .any(|other| other.evidence_handle == insight.evidence_handle)
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    }
    Ok(WorkContextExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: view.source_handle,
        scope_handle: view.scope_handle,
        expires_at_unix_ms: view.expires_at_unix_ms,
        summary: output.summary,
        insights: output.insights,
    })
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
