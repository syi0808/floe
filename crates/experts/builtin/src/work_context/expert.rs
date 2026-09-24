//! Work context insights.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::AgentFailure;
use floe_agent_contract::InferencePolicyDecision;
use floe_agent_contract::{ExpertModel, ExpertModelRequirement};
use floe_context_contract::{WorkContextView, validate_work_context_view, work_context_evidence};

use crate::prompts::work_context_expert_prompt;
use crate::shared::{
    ExpertJudgment, PortfolioExpertInvocation, run_portfolio_model, valid_text, validate_summary,
};

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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkOutput {
    summary: String,
    insights: Vec<WorkInsight>,
}

pub async fn run_work_context_expert<Model: ExpertModel>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PortfolioExpertInvocation,
    view: WorkContextView,
) -> Result<ExpertJudgment<WorkContextExpertResult>, AgentFailure> {
    validate_work_context_view(&view, invocation.current_time_unix_ms)?;
    let output: WorkOutput = match run_portfolio_model(
        model,
        policy,
        ExpertModelRequirement::RemoteOnly,
        &invocation,
        work_context_evidence(&view)?,
        work_context_expert_prompt(),
    )
    .await?
    {
        ExpertJudgment::Decided(output) => output,
        ExpertJudgment::Blocked(requirement) => {
            return Ok(ExpertJudgment::Blocked(requirement));
        }
    };
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
    Ok(ExpertJudgment::Decided(WorkContextExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: view.source_handle,
        scope_handle: view.scope_handle,
        expires_at_unix_ms: view.expires_at_unix_ms,
        summary: output.summary,
        insights: output.insights,
    }))
}
