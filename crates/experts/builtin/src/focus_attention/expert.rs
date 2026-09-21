//! Focus and attention recommendations.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::AgentFailure;
use floe_agent_contract::{ExpertModel, ExpertModelRequirement};
use floe_agent_contract::InferencePolicyDecision;
use floe_context_contract::{
    AttentionView, CalendarContextView, WorkContextView, personal_context_evidence,
    validate_attention_view, validate_work_context_view, work_context_evidence,
};

use crate::prompts::focus_expert_prompt;
use crate::shared::{
    PersonalExpertInvocation, add_schedule_views, ensure_unique_source, extend_unique_handles,
    run_personal_model, validate_judgment,
};

#[derive(Clone, Debug)]
pub struct FocusContextViews {
    pub attention: AttentionView,
    pub calendars: Vec<CalendarContextView>,
    pub active_work: Vec<WorkContextView>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusRecommendation {
    ProtectFocus,
    AvailableForInterruptions,
    NoConclusion,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FocusExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    #[serde(default)]
    pub source_handles: Vec<String>,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub recommendation: FocusRecommendation,
    pub rationale: String,
    pub evidence_handles: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FocusOutput {
    summary: String,
    recommendation: FocusRecommendation,
    rationale: String,
    evidence_handles: Vec<String>,
}

pub async fn run_focus_expert_with_views<Model: ExpertModel>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PersonalExpertInvocation,
    views: FocusContextViews,
) -> Result<FocusExpertResult, AgentFailure> {
    validate_attention_view(&views.attention, invocation.current_time_unix_ms)?;
    let mut evidence = vec![personal_context_evidence(&views.attention)?];
    let mut available = views.attention.evidence_handles.clone();
    let mut source_handles = vec![views.attention.source_handle.clone()];
    let mut expires_at_unix_ms = views.attention.expires_at_unix_ms;
    add_schedule_views(
        &views.calendars,
        invocation.current_time_unix_ms,
        &mut evidence,
        &mut available,
        &mut source_handles,
        &mut expires_at_unix_ms,
    )?;
    for view in &views.active_work {
        validate_work_context_view(view, invocation.current_time_unix_ms)?;
        ensure_unique_source(&source_handles, &view.source_handle)?;
        extend_unique_handles(
            &mut available,
            view.items.iter().map(|item| &item.evidence_handle),
        )?;
        source_handles.push(view.source_handle.clone());
        expires_at_unix_ms = expires_at_unix_ms.min(view.expires_at_unix_ms);
        evidence.push(work_context_evidence(view)?);
    }
    let output: FocusOutput = run_personal_model(
        model,
        policy,
        ExpertModelRequirement::DeviceOnly,
        &invocation,
        evidence,
        focus_expert_prompt(),
    )
    .await?;
    validate_judgment(
        &output.summary,
        &output.rationale,
        &output.evidence_handles,
        &available,
        matches!(output.recommendation, FocusRecommendation::NoConclusion),
    )?;
    Ok(FocusExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: views.attention.source_handle,
        source_handles,
        expires_at_unix_ms,
        summary: output.summary,
        recommendation: output.recommendation,
        rationale: output.rationale,
        evidence_handles: output.evidence_handles,
    })
}
