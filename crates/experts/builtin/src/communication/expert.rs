//! Communication assessment over confirmed interactions.

use floe_kernel::PersonId;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use crate::prompts::{commitments_expert_prompt, communication_expert_prompt};
use floe_context::{CalendarContextView, CommunicationView, calendar_context_evidence, communication_context_evidence, validate_calendar_context_view, validate_communication_view};
use floe_agent_contract::{AgentFailure, SessionProtection};
use floe_context::{AgentContext, FLOE_TASK_VIEW_ID, InferencePolicyDecision, NativeContextItem, NativeContextView, native_context_evidence, validate_native_context_view};
use floe_kernel::AGENT_VERSION;
use floe_conversation::{AgentMessage, ModelRequest, ModelRunner, ModelStep};
use floe_conversation::{UsageLedger, generate_with_recovery};

pub enum CommunicationChannel {
    Email,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommunicationResultKind {
    #[default]
    NoReply,
    ReplyRecommended,
    DraftForReview,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommunicationAssessment {
    pub evidence_handle: String,
    pub needs_reply: bool,
    pub rationale: String,
    pub channel: CommunicationChannel,
    pub tone: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    #[serde(default)]
    pub result: CommunicationResultKind,
    #[serde(default)]
    pub requires_review: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommunicationExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub assessments: Vec<CommunicationAssessment>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationModelOutput {
    summary: String,
    assessments: Vec<CommunicationAssessment>,
}

pub async fn run_communication_expert<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: MailExpertInvocation,
) -> Result<CommunicationExpertResult, AgentFailure> {
    let response = run_mail_model(
        model,
        policy,
        &invocation,
        communication_expert_prompt(),
        invocation.context.clone(),
    )
    .await?;
    let mut output: CommunicationModelOutput =
        decode_answer(&response, invocation.max_output_bytes)?;
    validate_summary(&output.summary)?;
    if output.assessments.len() > MAX_MAIL_EXPERT_FINDINGS {
        return Err(AgentFailure::BudgetExceeded);
    }
    let mut assessment_handles = std::collections::HashSet::new();
    for assessment in &mut output.assessments {
        if !evidence_exists(&invocation.view, &assessment.evidence_handle)
            || assessment.rationale.trim().is_empty()
            || assessment.rationale.len() > 512
            || assessment.tone.trim().is_empty()
            || assessment.tone.len() > 64
            || assessment.draft.as_ref().is_some_and(|draft| {
                draft.trim().is_empty() || draft.len() > 4096 || !assessment.needs_reply
            })
            || !assessment_handles.insert(assessment.evidence_handle.clone())
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        assessment.result = match (assessment.needs_reply, assessment.draft.is_some()) {
            (false, _) => CommunicationResultKind::NoReply,
            (true, false) => CommunicationResultKind::ReplyRecommended,
            (true, true) => CommunicationResultKind::DraftForReview,
        };
        assessment.requires_review = assessment.draft.is_some();
    }
    Ok(CommunicationExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: invocation.view.source_handle,
        expires_at_unix_ms: invocation.view.expires_at_unix_ms,
        summary: output.summary,
        assessments: output.assessments,
    })
}

async fn run_mail_model<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: &MailExpertInvocation,
    prompt: floe_knowledge::prompts::PromptAssembly,
    mut context: AgentContext,
) -> Result<floe_conversation::ModelResponse, AgentFailure> {
    if invocation.assignment.trim().is_empty()
        || invocation.assignment.len() > 2048
        || invocation.max_output_bytes == 0
        || invocation.max_model_tokens == 0
        || invocation.deadline <= Instant::now()
        || invocation.cancellation.is_cancelled()
    {
        return Err(AgentFailure::InvalidInput);
    }
    validate_communication_view(
        &invocation.view,
        invocation.current_time_unix_ms,
        crate::MAX_COMMUNICATION_ITEMS,
        crate::MAX_COMMUNICATION_BYTES,
    )?;
    context
        .evidence
        .push(communication_context_evidence(&invocation.view)?);
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
            context: context.clone(),
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
    Ok(response)
}

fn decode_answer<Output: for<'de> Deserialize<'de>>(
    response: &floe_conversation::ModelResponse,
    maximum_bytes: usize,
) -> Result<Output, AgentFailure> {
    let [ModelStep::Answer { text }] = response.output.as_slice() else {
        return Err(AgentFailure::InvalidModelOutput);
    };
    if text.len() > maximum_bytes.min(8192) {
        return Err(AgentFailure::BudgetExceeded);
    }
    serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)
}

fn evidence_exists(view: &CommunicationView, handle: &str) -> bool {
    view.items.iter().any(|item| item.evidence_handle == handle)
}

fn validate_summary(summary: &str) -> Result<(), AgentFailure> {
    if summary.trim().is_empty() || summary.len() > 2048 {
        Err(AgentFailure::InvalidModelOutput)
    } else {
        Ok(())
    }
}
