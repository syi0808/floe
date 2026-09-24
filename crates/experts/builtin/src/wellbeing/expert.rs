//! Wellbeing judgment and its schedule impact.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::AgentFailure;
use floe_agent_contract::InferencePolicyDecision;
use floe_agent_contract::{ExpertModel, ExpertModelRequirement};
use floe_context_contract::{
    CalendarContextView, WellbeingView, personal_context_evidence, validate_wellbeing_view,
};

use crate::prompts::wellbeing_expert_prompt;
use crate::shared::{
    ExpertJudgment, PersonalExpertInvocation, add_schedule_views, run_personal_model,
    validate_judgment,
};

#[derive(Clone, Debug)]
pub struct WellbeingContextViews {
    pub wellbeing: WellbeingView,
    pub calendars: Vec<CalendarContextView>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleImpact {
    KeepPlan,
    ReduceLoad,
    ProtectRecovery,
    NoConclusion,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WellbeingExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    #[serde(default)]
    pub source_handles: Vec<String>,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub schedule_impact: ScheduleImpact,
    pub rationale: String,
    pub evidence_handles: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WellbeingOutput {
    summary: String,
    schedule_impact: ScheduleImpact,
    rationale: String,
    evidence_handles: Vec<String>,
}

pub async fn run_wellbeing_expert_with_views<Model: ExpertModel>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PersonalExpertInvocation,
    views: WellbeingContextViews,
) -> Result<ExpertJudgment<WellbeingExpertResult>, AgentFailure> {
    validate_wellbeing_view(&views.wellbeing, invocation.current_time_unix_ms)?;
    let mut evidence = vec![personal_context_evidence(&views.wellbeing)?];
    let mut available = views.wellbeing.evidence_handles.clone();
    let mut source_handles = vec![views.wellbeing.source_handle.clone()];
    let mut expires_at_unix_ms = views.wellbeing.expires_at_unix_ms;
    add_schedule_views(
        &views.calendars,
        invocation.current_time_unix_ms,
        &mut evidence,
        &mut available,
        &mut source_handles,
        &mut expires_at_unix_ms,
    )?;
    let output: WellbeingOutput = match run_personal_model(
        model,
        policy,
        ExpertModelRequirement::Any,
        &invocation,
        evidence,
        wellbeing_expert_prompt(),
    )
    .await?
    {
        ExpertJudgment::Decided(output) => output,
        ExpertJudgment::Blocked(requirement) => {
            return Ok(ExpertJudgment::Blocked(requirement));
        }
    };
    validate_judgment(
        &output.summary,
        &output.rationale,
        &output.evidence_handles,
        &available,
        matches!(output.schedule_impact, ScheduleImpact::NoConclusion),
    )?;
    Ok(ExpertJudgment::Decided(WellbeingExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: views.wellbeing.source_handle,
        source_handles,
        expires_at_unix_ms,
        summary: output.summary,
        schedule_impact: output.schedule_impact,
        rationale: output.rationale,
        evidence_handles: output.evidence_handles,
    }))
}
