//! Wellbeing judgment and its schedule impact.

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

pub struct WellbeingContextViews {
    pub wellbeing: WellbeingView,
    pub calendars: Vec<CalendarContextView>,
}

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

pub async fn run_wellbeing_expert_with_views<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PersonalExpertInvocation,
    views: WellbeingContextViews,
) -> Result<WellbeingExpertResult, AgentFailure> {
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
    let output: WellbeingOutput = run_personal_model(
        model,
        policy,
        &invocation,
        evidence,
        wellbeing_expert_prompt(),
    )
    .await?;
    validate_judgment(
        &output.summary,
        &output.rationale,
        &output.evidence_handles,
        &available,
        matches!(output.schedule_impact, ScheduleImpact::NoConclusion),
    )?;
    Ok(WellbeingExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: views.wellbeing.source_handle,
        source_handles,
        expires_at_unix_ms,
        summary: output.summary,
        schedule_impact: output.schedule_impact,
        rationale: output.rationale,
        evidence_handles: output.evidence_handles,
    })
}
