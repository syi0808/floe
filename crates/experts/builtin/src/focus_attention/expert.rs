//! Focus and attention recommendations.

use floe_kernel::PersonId;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::time::Instant;
use uuid::Uuid;

use crate::prompts::{focus_expert_prompt, relationships_expert_prompt, wellbeing_expert_prompt};
use floe_context::{AttentionView, CalendarContextView, PeopleView, WellbeingView, WorkContextView, calendar_context_evidence, personal_context_evidence, validate_attention_view, validate_calendar_context_view, validate_people_view, validate_wellbeing_view, validate_work_context_view, work_context_evidence};
use floe_agent_contract::{AgentFailure, DataClass, SessionProtection};
use floe_context::{AgentContext, ContextEvidence, InferencePolicyDecision};
use floe_kernel::AGENT_VERSION;
use floe_conversation::{AgentMessage, ModelRequest, ModelRunner, ModelStep};
use floe_conversation::{UsageLedger, generate_with_recovery};
use floe_knowledge::prompts::{PromptAssembly};

pub struct FocusContextViews {
    pub attention: AttentionView,
    pub calendars: Vec<CalendarContextView>,
    pub active_work: Vec<WorkContextView>,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
struct FocusOutput {
    summary: String,
    recommendation: FocusRecommendation,
    rationale: String,
    evidence_handles: Vec<String>,
}

pub async fn run_focus_expert_with_views<Model: ModelRunner>(
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
    let output: FocusOutput =
        run_personal_model(model, policy, &invocation, evidence, focus_expert_prompt()).await?;
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
