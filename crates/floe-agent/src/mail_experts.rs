use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    AGENT_VERSION, AgentContext, AgentFailure, AgentMessage, CommunicationView,
    InferencePolicyDecision, ModelRequest, ModelRunner, ModelStep, SessionProtection, UsageLedger,
    commitments_expert_prompt, communication_context_evidence, communication_expert_prompt,
    generate_with_recovery, validate_communication_view,
};

const MAX_MAIL_EXPERT_FINDINGS: usize = 16;

pub struct MailExpertInvocation {
    pub usage: UsageLedger,
    pub person_id: PersonId,
    pub invocation_id: Uuid,
    pub assignment: String,
    pub current_time_unix_ms: i64,
    pub context: AgentContext,
    pub view: CommunicationView,
    pub max_output_bytes: usize,
    pub max_model_tokens: u64,
    pub max_model_cost_micros: u64,
    pub deadline: Instant,
    pub cancellation: crate::Cancellation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitmentKind {
    UserCommitment,
    RequestToUser,
    ExpectedReply,
    FollowUpGap,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingEpistemicStatus {
    Observed,
    Inferred,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitmentFinding {
    pub evidence_handle: String,
    pub kind: CommitmentKind,
    pub statement: String,
    pub epistemic_status: FindingEpistemicStatus,
    pub confidence_millis: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitmentsExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub findings: Vec<CommitmentFinding>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommunicationChannel {
    Email,
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
struct CommitmentsModelOutput {
    summary: String,
    findings: Vec<CommitmentFinding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationModelOutput {
    summary: String,
    assessments: Vec<CommunicationAssessment>,
}

pub async fn run_commitments_expert<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: MailExpertInvocation,
) -> Result<CommitmentsExpertResult, AgentFailure> {
    let response = run_mail_model(model, policy, &invocation, commitments_expert_prompt()).await?;
    let output: CommitmentsModelOutput = decode_answer(&response, invocation.max_output_bytes)?;
    validate_summary(&output.summary)?;
    if output.findings.len() > MAX_MAIL_EXPERT_FINDINGS {
        return Err(AgentFailure::BudgetExceeded);
    }
    for (index, finding) in output.findings.iter().enumerate() {
        if !evidence_exists(&invocation.view, &finding.evidence_handle)
            || finding.statement.trim().is_empty()
            || finding.statement.len() > 512
            || finding.confidence_millis == 0
            || finding.confidence_millis > 1000
            || matches!(finding.epistemic_status, FindingEpistemicStatus::Observed)
                != (finding.confidence_millis == 1000)
            || finding
                .deadline_unix_ms
                .is_some_and(|deadline| deadline < 0)
            || output.findings[..index].iter().any(|other| {
                other.evidence_handle == finding.evidence_handle && other.kind == finding.kind
            })
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    }
    Ok(CommitmentsExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: invocation.view.source_handle,
        expires_at_unix_ms: invocation.view.expires_at_unix_ms,
        summary: output.summary,
        findings: output.findings,
    })
}

pub async fn run_communication_expert<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: MailExpertInvocation,
) -> Result<CommunicationExpertResult, AgentFailure> {
    let response =
        run_mail_model(model, policy, &invocation, communication_expert_prompt()).await?;
    let output: CommunicationModelOutput = decode_answer(&response, invocation.max_output_bytes)?;
    validate_summary(&output.summary)?;
    if output.assessments.len() > MAX_MAIL_EXPERT_FINDINGS {
        return Err(AgentFailure::BudgetExceeded);
    }
    for (index, assessment) in output.assessments.iter().enumerate() {
        if !evidence_exists(&invocation.view, &assessment.evidence_handle)
            || assessment.rationale.trim().is_empty()
            || assessment.rationale.len() > 512
            || assessment.tone.trim().is_empty()
            || assessment.tone.len() > 64
            || assessment.draft.as_ref().is_some_and(|draft| {
                draft.trim().is_empty() || draft.len() > 4096 || !assessment.needs_reply
            })
            || output.assessments[..index]
                .iter()
                .any(|other| other.evidence_handle == assessment.evidence_handle)
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
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
    prompt: crate::PromptAssembly,
) -> Result<crate::ModelResponse, AgentFailure> {
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
    let mut context = invocation.context.clone();
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
    response: &crate::ModelResponse,
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
