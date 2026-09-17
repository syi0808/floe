//! Life logistics preparation and urgency.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::AgentFailure;
use floe_agent_contract::AGENT_VERSION;
use floe_context_contract::{LogisticsView, logistics_context_evidence, validate_logistics_view};
use floe_agent_contract::{InferencePolicyDecision};
use floe_agent_contract::ExpertModel;

use crate::prompts::{life_logistics_expert_prompt};
use crate::shared::{PortfolioExpertInvocation, run_portfolio_model, valid_text, validate_summary};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
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

pub async fn run_life_logistics_expert<Model: ExpertModel>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PortfolioExpertInvocation,
    view: LogisticsView,
) -> Result<LifeLogisticsExpertResult, AgentFailure> {
    validate_logistics_view(&view, invocation.current_time_unix_ms)?;
    let output: LogisticsOutput = run_portfolio_model(
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
