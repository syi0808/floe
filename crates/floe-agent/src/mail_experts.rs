use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    AGENT_VERSION, AgentContext, AgentFailure, AgentMessage, CalendarContextView,
    CommunicationView, FLOE_TASK_VIEW_ID, InferencePolicyDecision, ModelRequest, ModelRunner,
    ModelStep, NativeContextItem, NativeContextView, SessionProtection, UsageLedger,
    calendar_context_evidence, commitments_expert_prompt, communication_context_evidence,
    communication_expert_prompt, generate_with_recovery, native_context_evidence,
    validate_calendar_context_view, validate_communication_view, validate_native_context_view,
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

#[derive(Clone, Debug, Default)]
pub struct CommitmentsContextViews {
    pub calendars: Vec<CalendarContextView>,
    pub tasks: Vec<NativeContextView>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitmentEvidenceSource {
    #[default]
    Mail,
    Calendar,
    FloeTask,
    ConfirmedMemory,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitmentFinding {
    pub evidence_handle: String,
    #[serde(default)]
    pub evidence_source: CommitmentEvidenceSource,
    #[serde(default)]
    pub source_handle: String,
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
    pub source_handles: Vec<String>,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub findings: Vec<CommitmentFinding>,
}

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

pub async fn run_commitments_expert_with_views<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: MailExpertInvocation,
    views: CommitmentsContextViews,
) -> Result<CommitmentsExpertResult, AgentFailure> {
    let evidence = commitment_evidence(&invocation, &views)?;
    let response = run_mail_model(
        model,
        policy,
        &invocation,
        commitments_expert_prompt(),
        evidence.context,
    )
    .await?;
    let mut output: CommitmentsModelOutput = decode_answer(&response, invocation.max_output_bytes)?;
    validate_summary(&output.summary)?;
    if output.findings.len() > MAX_MAIL_EXPERT_FINDINGS {
        return Err(AgentFailure::BudgetExceeded);
    }
    let mut finding_keys = std::collections::HashSet::new();
    for finding in &mut output.findings {
        let Some((source, source_handle)) = evidence
            .handles
            .iter()
            .find(|(handle, _, _)| handle == &finding.evidence_handle)
            .map(|(_, source, source_handle)| (*source, source_handle.clone()))
        else {
            return Err(AgentFailure::InvalidModelOutput);
        };
        finding.evidence_source = source;
        finding.source_handle = source_handle;
        if finding.statement.trim().is_empty()
            || finding.statement.len() > 512
            || finding.confidence_millis == 0
            || finding.confidence_millis > 1000
            || matches!(finding.epistemic_status, FindingEpistemicStatus::Observed)
                != (finding.confidence_millis == 1000)
            || finding
                .deadline_unix_ms
                .is_some_and(|deadline| deadline < 0)
            || !finding_keys.insert((finding.evidence_handle.clone(), finding.kind))
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    }
    Ok(CommitmentsExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handles: evidence.source_handles,
        expires_at_unix_ms: evidence.expires_at_unix_ms,
        summary: output.summary,
        findings: output.findings,
    })
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
    prompt: crate::PromptAssembly,
    mut context: AgentContext,
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

struct CommitmentEvidence {
    context: AgentContext,
    handles: Vec<(String, CommitmentEvidenceSource, String)>,
    source_handles: Vec<String>,
    expires_at_unix_ms: i64,
}

fn commitment_evidence(
    invocation: &MailExpertInvocation,
    views: &CommitmentsContextViews,
) -> Result<CommitmentEvidence, AgentFailure> {
    let now = invocation.current_time_unix_ms;
    validate_communication_view(
        &invocation.view,
        now,
        crate::MAX_COMMUNICATION_ITEMS,
        crate::MAX_COMMUNICATION_BYTES,
    )?;
    let mut context = invocation.context.clone();
    let mut handles = invocation
        .view
        .items
        .iter()
        .map(|item| {
            (
                item.evidence_handle.clone(),
                CommitmentEvidenceSource::Mail,
                invocation.view.source_handle.clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut source_handles = vec![];
    let mut expires_at_unix_ms = i64::MAX;
    if !invocation.view.items.is_empty() {
        source_handles.push(invocation.view.source_handle.clone());
        expires_at_unix_ms = invocation.view.expires_at_unix_ms;
    }

    for view in &views.calendars {
        validate_calendar_context_view(view, now)?;
        handles.extend(view.items.iter().map(|item| {
            (
                item.evidence_handle.clone(),
                CommitmentEvidenceSource::Calendar,
                view.source_handle.clone(),
            )
        }));
        if !view.items.is_empty() {
            source_handles.push(view.source_handle.clone());
            expires_at_unix_ms = expires_at_unix_ms.min(view.expires_at_unix_ms);
        }
        context.evidence.push(calendar_context_evidence(view)?);
    }
    for view in &views.tasks {
        if view.view_id != FLOE_TASK_VIEW_ID {
            return Err(AgentFailure::InvalidInput);
        }
        let now = u64::try_from(now).map_err(|_| AgentFailure::InvalidInput)?;
        validate_native_context_view(
            view,
            invocation.person_id,
            view.handle,
            now,
            crate::MAX_NATIVE_CONTEXT_ITEMS,
            crate::MAX_NATIVE_CONTEXT_BYTES,
        )?;
        handles.extend(view.items.iter().filter_map(|item| match item {
            NativeContextItem::Task {
                evidence_handle, ..
            } => Some((
                evidence_handle.to_string(),
                CommitmentEvidenceSource::FloeTask,
                view.source_handle.clone(),
            )),
            NativeContextItem::Note { .. } => None,
        }));
        if !view.items.is_empty() {
            source_handles.push(view.source_handle.clone());
            expires_at_unix_ms = expires_at_unix_ms.min(
                i64::try_from(view.expires_at_unix_ms).map_err(|_| AgentFailure::InvalidInput)?,
            );
        }
        context.evidence.push(native_context_evidence(view)?);
    }
    for memory in &context.memories {
        let source_handle = format!("memory:{}:{}", memory.target_id, memory.revision);
        handles.push((
            memory.target_id.to_string(),
            CommitmentEvidenceSource::ConfirmedMemory,
            source_handle.clone(),
        ));
        source_handles.push(source_handle);
        if let Some(valid_until) = memory.valid_until_unix_ms {
            expires_at_unix_ms = expires_at_unix_ms.min(valid_until);
        }
    }
    if handles
        .iter()
        .enumerate()
        .any(|(index, item)| handles[..index].iter().any(|other| other.0 == item.0))
    {
        return Err(AgentFailure::InvalidInput);
    }
    source_handles.sort();
    source_handles.dedup();
    if source_handles.is_empty() {
        source_handles.push(invocation.view.source_handle.clone());
        expires_at_unix_ms = invocation.view.expires_at_unix_ms;
    }
    Ok(CommitmentEvidence {
        context,
        handles,
        source_handles,
        expires_at_unix_ms,
    })
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
