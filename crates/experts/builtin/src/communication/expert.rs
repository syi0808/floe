//! Communication assessment over confirmed interactions.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::AgentFailure;
use floe_agent_contract::AGENT_VERSION;
use floe_context_contract::{CommunicationView};
use floe_agent_contract::{InferencePolicyDecision};
use floe_conversation::ModelRunner;

use crate::prompts::{communication_expert_prompt};
use crate::shared::{MAX_MAIL_EXPERT_FINDINGS, MailExpertInvocation, decode_answer, run_mail_model, validate_summary};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
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

fn evidence_exists(view: &CommunicationView, handle: &str) -> bool {
    view.items.iter().any(|item| item.evidence_handle == handle)
}
