//! Commitment findings extracted from confirmed communication evidence.

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
    pub cancellation: floe_execution::Cancellation,
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
struct CommitmentsModelOutput {
    summary: String,
    findings: Vec<CommitmentFinding>,
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
            floe_context::MAX_NATIVE_CONTEXT_ITEMS,
            floe_context::MAX_NATIVE_CONTEXT_BYTES,
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
