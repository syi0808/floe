use floe_domain::PersonId;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    AGENT_VERSION, AgentContext, AgentFailure, AgentMessage, AttentionView,
    InferencePolicyDecision, ModelRequest, ModelRunner, ModelStep, PeopleView, PromptAssembly,
    SessionProtection, UsageLedger, WellbeingView, focus_expert_prompt, generate_with_recovery,
    personal_context_evidence, relationships_expert_prompt, validate_attention_view,
    validate_people_view, validate_wellbeing_view, wellbeing_expert_prompt,
};

pub struct PersonalExpertInvocation {
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
    pub cancellation: crate::Cancellation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipFollowUp {
    pub identity_handle: String,
    pub reason: String,
    pub evidence_handles: Vec<String>,
    pub confidence_millis: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipsExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub follow_ups: Vec<RelationshipFollowUp>,
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
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub recommendation: FocusRecommendation,
    pub rationale: String,
    pub evidence_handles: Vec<String>,
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
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub schedule_impact: ScheduleImpact,
    pub rationale: String,
    pub evidence_handles: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RelationshipsOutput {
    summary: String,
    follow_ups: Vec<RelationshipFollowUp>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FocusOutput {
    summary: String,
    recommendation: FocusRecommendation,
    rationale: String,
    evidence_handles: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WellbeingOutput {
    summary: String,
    schedule_impact: ScheduleImpact,
    rationale: String,
    evidence_handles: Vec<String>,
}

pub async fn run_relationships_expert<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PersonalExpertInvocation,
    view: PeopleView,
) -> Result<RelationshipsExpertResult, AgentFailure> {
    validate_people_view(&view, invocation.current_time_unix_ms)?;
    let output: RelationshipsOutput = run_personal_model(
        model,
        policy,
        &invocation,
        personal_context_evidence(&view)?,
        relationships_expert_prompt(),
    )
    .await?;
    validate_summary(&output.summary)?;
    if output.follow_ups.len() > 16 {
        return Err(AgentFailure::BudgetExceeded);
    }
    for (index, follow_up) in output.follow_ups.iter().enumerate() {
        let Some(identity) = view
            .identities
            .iter()
            .find(|identity| identity.identity_handle == follow_up.identity_handle)
        else {
            return Err(AgentFailure::InvalidModelOutput);
        };
        if follow_up.reason.trim().is_empty()
            || follow_up.reason.len() > 512
            || follow_up.confidence_millis == 0
            || follow_up.confidence_millis > 1000
            || follow_up.evidence_handles.is_empty()
            || follow_up.evidence_handles.len() > 16
            || follow_up
                .evidence_handles
                .iter()
                .any(|handle| !identity.evidence_handles.contains(handle))
            || output.follow_ups[..index]
                .iter()
                .any(|other| other.identity_handle == follow_up.identity_handle)
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    }
    Ok(RelationshipsExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: view.source_handle,
        expires_at_unix_ms: view.expires_at_unix_ms,
        summary: output.summary,
        follow_ups: output.follow_ups,
    })
}

pub async fn run_focus_expert<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PersonalExpertInvocation,
    view: AttentionView,
) -> Result<FocusExpertResult, AgentFailure> {
    validate_attention_view(&view, invocation.current_time_unix_ms)?;
    let output: FocusOutput = run_personal_model(
        model,
        policy,
        &invocation,
        personal_context_evidence(&view)?,
        focus_expert_prompt(),
    )
    .await?;
    validate_judgment(
        &output.summary,
        &output.rationale,
        &output.evidence_handles,
        &view.evidence_handles,
        matches!(output.recommendation, FocusRecommendation::NoConclusion),
    )?;
    Ok(FocusExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: view.source_handle,
        expires_at_unix_ms: view.expires_at_unix_ms,
        summary: output.summary,
        recommendation: output.recommendation,
        rationale: output.rationale,
        evidence_handles: output.evidence_handles,
    })
}

pub async fn run_wellbeing_expert<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PersonalExpertInvocation,
    view: WellbeingView,
) -> Result<WellbeingExpertResult, AgentFailure> {
    validate_wellbeing_view(&view, invocation.current_time_unix_ms)?;
    let output: WellbeingOutput = run_personal_model(
        model,
        policy,
        &invocation,
        personal_context_evidence(&view)?,
        wellbeing_expert_prompt(),
    )
    .await?;
    validate_judgment(
        &output.summary,
        &output.rationale,
        &output.evidence_handles,
        &view.evidence_handles,
        matches!(output.schedule_impact, ScheduleImpact::NoConclusion),
    )?;
    Ok(WellbeingExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: view.source_handle,
        expires_at_unix_ms: view.expires_at_unix_ms,
        summary: output.summary,
        schedule_impact: output.schedule_impact,
        rationale: output.rationale,
        evidence_handles: output.evidence_handles,
    })
}

async fn run_personal_model<Output: DeserializeOwned, Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: &PersonalExpertInvocation,
    evidence: crate::ContextEvidence,
    prompt: PromptAssembly,
) -> Result<Output, AgentFailure> {
    if invocation.assignment.trim().is_empty()
        || invocation.assignment.len() > 2048
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

fn validate_summary(summary: &str) -> Result<(), AgentFailure> {
    if summary.trim().is_empty() || summary.len() > 2048 {
        Err(AgentFailure::InvalidModelOutput)
    } else {
        Ok(())
    }
}

fn validate_judgment(
    summary: &str,
    rationale: &str,
    evidence_handles: &[String],
    available: &[String],
    no_conclusion: bool,
) -> Result<(), AgentFailure> {
    validate_summary(summary)?;
    if rationale.trim().is_empty()
        || rationale.len() > 512
        || evidence_handles.len() > 16
        || no_conclusion != evidence_handles.is_empty()
        || evidence_handles
            .iter()
            .any(|handle| !available.contains(handle))
    {
        Err(AgentFailure::InvalidModelOutput)
    } else {
        Ok(())
    }
}
