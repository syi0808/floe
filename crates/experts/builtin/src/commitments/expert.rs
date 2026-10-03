//! Commitment findings extracted from confirmed communication evidence.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::prompts::{PromptAssembly, PromptComponentKind};
use floe_agent_contract::{AGENT_VERSION, AgentContext, AgentFailure};
use floe_context_contract::{
    CalendarContextView, ContextIssueReason, ContextMemory, ContextSource, FLOE_TASK_VIEW_ID,
    MAX_COMMUNICATION_BYTES, MAX_COMMUNICATION_ITEMS, NativeContextItem, NativeContextView,
    calendar_context_evidence, communication_context_evidence, native_context_evidence,
    record_source_issue, validate_calendar_context_view, validate_communication_view,
    validate_native_context_view,
};
use floe_experts::{
    ExpertFinalOutput, ExpertProgram, ExpertProgramRequest, ExpertProgramSpec,
    ExpertToolObservation,
};

use crate::prompts::commitments_expert_prompt;

const MAIL_REQUIREMENT: &str = "floe.source.mail";
const CALENDAR_REQUIREMENT: &str = "floe.source.calendar";
const TASK_REQUIREMENT: &str = "floe.source.tasks";
const MEMORY_REQUIREMENT: &str = "floe.source.confirmed-memory";
const FINAL_JUDGMENT_INSTRUCTION: &str = "Before the final judgment, read every declared evidence tool marked as required. Return exactly one JSON object matching this package's output contract.";
const OUTPUT_CONTRACT: &str = r#"{"type":"object","additionalProperties":false,"properties":{"summary":{"type":"string","minLength":1,"maxLength":2048},"findings":{"type":"array","maxItems":16,"items":{"type":"object","additionalProperties":false,"properties":{"evidence_handle":{"type":"string","minLength":1,"maxLength":128},"kind":{"type":"string","enum":["user_commitment","request_to_user","expected_reply","follow_up_gap"]},"statement":{"type":"string","minLength":1,"maxLength":512},"epistemic_status":{"type":"string","enum":["observed","inferred"]},"confidence_millis":{"type":"integer","minimum":1,"maximum":1000},"deadline_unix_ms":{"type":["integer","null"],"minimum":0,"maximum":9223372036854775807}},"required":["evidence_handle","kind","statement","epistemic_status","confidence_millis","deadline_unix_ms"]}}},"required":["summary","findings"]}"#;

#[derive(Clone, Copy, Debug, Default)]
pub struct CommitmentsProgram;

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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitmentsModelOutput {
    summary: String,
    findings: Vec<CommitmentFinding>,
}

#[derive(Deserialize)]
struct ConfirmedMemoryInput {
    memories: Vec<ContextMemory>,
    issue: Option<ContextIssueReason>,
}

struct CommitmentEvidence {
    handles: Vec<(String, CommitmentEvidenceSource, String)>,
    source_handles: Vec<String>,
    expires_at_unix_ms: i64,
}

impl ExpertProgram for CommitmentsProgram {
    fn specification(
        &self,
        request: &ExpertProgramRequest,
    ) -> Result<ExpertProgramSpec, AgentFailure> {
        let prompt = append_required_evidence_instruction(commitments_expert_prompt())?;
        crate::program_support::specification(request, prompt, OUTPUT_CONTRACT)
    }

    fn finalize(
        &self,
        request: &ExpertProgramRequest,
        observations: &[ExpertToolObservation],
        text: &str,
        artifacts: &[floe_agent_contract::Artifact],
    ) -> Result<ExpertFinalOutput, AgentFailure> {
        if !artifacts.is_empty() {
            return Err(AgentFailure::InvalidModelOutput);
        }
        if text.len() > request.request.execution_context.max_output_bytes.min(8192) {
            return Err(AgentFailure::BudgetExceeded);
        }

        let mail_view = match crate::program_support::read_one(observations, MAIL_REQUIREMENT)? {
            Some(view) => view,
            None if crate::program_support::was_unavailable(observations, MAIL_REQUIREMENT) => {
                return crate::program_support::unavailable(
                    request,
                    observations,
                    MAIL_REQUIREMENT,
                    crate::BuiltinExpertKind::Commitments.result_artifact_name(),
                    super::RESULT_MEDIA_TYPE,
                    "Mail is temporarily unavailable, so there are no commitment findings.",
                );
            }
            None => return Err(AgentFailure::InvalidModelOutput),
        };

        let evidence = commitment_evidence(request, observations, &mail_view)?;
        let mut output: CommitmentsModelOutput =
            serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)?;
        crate::shared::validate_summary(&output.summary)?;
        if output.findings.len() > crate::shared::MAX_MAIL_EXPERT_FINDINGS {
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

        let result = CommitmentsExpertResult {
            schema_version: AGENT_VERSION,
            invocation_id: request.request.invocation_key.as_uuid(),
            source_handles: evidence.source_handles,
            expires_at_unix_ms: evidence.expires_at_unix_ms,
            summary: output.summary,
            findings: output.findings,
        };
        crate::program_support::result(
            request,
            observations,
            crate::BuiltinExpertKind::Commitments.result_artifact_name(),
            super::RESULT_MEDIA_TYPE,
            result.summary.clone(),
            &result,
        )
    }
}

fn append_required_evidence_instruction(
    mut prompt: PromptAssembly,
) -> Result<PromptAssembly, AgentFailure> {
    let role = prompt
        .components
        .iter_mut()
        .find(|component| component.kind == PromptComponentKind::Role)
        .ok_or(AgentFailure::InvalidInput)?;
    role.content.push_str("\n\n");
    role.content.push_str(FINAL_JUDGMENT_INSTRUCTION);
    prompt.validate()?;
    Ok(prompt)
}

fn commitment_evidence(
    request: &ExpertProgramRequest,
    observations: &[ExpertToolObservation],
    mail_view: &floe_context_contract::CommunicationView,
) -> Result<CommitmentEvidence, AgentFailure> {
    let now = request.now_unix_ms;
    validate_communication_view(
        mail_view,
        now,
        MAX_COMMUNICATION_ITEMS,
        MAX_COMMUNICATION_BYTES,
    )?;
    let _ = communication_context_evidence(mail_view)?;

    let mut context: AgentContext = request.context.clone();
    let mut handles = mail_view
        .items
        .iter()
        .map(|item| {
            (
                item.evidence_handle.clone(),
                CommitmentEvidenceSource::Mail,
                mail_view.source_handle.clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut source_handles = vec![];
    let mut expires_at_unix_ms = i64::MAX;
    if !mail_view.items.is_empty() {
        source_handles.push(mail_view.source_handle.clone());
        expires_at_unix_ms = mail_view.expires_at_unix_ms;
    }

    match crate::program_support::read_one::<Vec<CalendarContextView>>(
        observations,
        CALENDAR_REQUIREMENT,
    )? {
        Some(calendars) => {
            for view in &calendars {
                validate_calendar_context_view(view, now)?;
                let _ = calendar_context_evidence(view)?;
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
            }
            record_source_issue(
                &mut context.optional_context_issues,
                ContextSource::Calendar,
                None,
            );
        }
        None if crate::program_support::was_unavailable(observations, CALENDAR_REQUIREMENT) => {
            record_source_issue(
                &mut context.optional_context_issues,
                ContextSource::Calendar,
                Some(ContextIssueReason::Unavailable),
            );
        }
        None => {}
    }

    match crate::program_support::read_one::<NativeContextView>(observations, TASK_REQUIREMENT)? {
        Some(view) => {
            if view.view_id != FLOE_TASK_VIEW_ID {
                return Err(AgentFailure::InvalidInput);
            }
            let now = u64::try_from(now).map_err(|_| AgentFailure::InvalidInput)?;
            validate_native_context_view(
                &view,
                request.actor.person_id,
                view.handle,
                now,
                floe_context_contract::MAX_NATIVE_CONTEXT_ITEMS,
                floe_context_contract::MAX_NATIVE_CONTEXT_BYTES,
            )?;
            let _ = native_context_evidence(&view)?;
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
                    i64::try_from(view.expires_at_unix_ms)
                        .map_err(|_| AgentFailure::InvalidInput)?,
                );
            }
        }
        None if crate::program_support::was_unavailable(observations, TASK_REQUIREMENT) => {
            record_source_issue(
                &mut context.optional_context_issues,
                ContextSource::Tasks,
                Some(ContextIssueReason::Unavailable),
            );
        }
        None => {}
    }

    match crate::program_support::read_one::<ConfirmedMemoryInput>(
        observations,
        MEMORY_REQUIREMENT,
    )? {
        Some(snapshot) => {
            context.memories = snapshot.memories;
            record_source_issue(
                &mut context.optional_context_issues,
                ContextSource::Memory,
                snapshot.issue,
            );
        }
        None if crate::program_support::was_unavailable(observations, MEMORY_REQUIREMENT)
            && context.memories.is_empty() =>
        {
            record_source_issue(
                &mut context.optional_context_issues,
                ContextSource::Memory,
                Some(ContextIssueReason::Unavailable),
            );
        }
        None => {}
    }
    context.validate()?;

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
        source_handles.push(mail_view.source_handle.clone());
        expires_at_unix_ms = mail_view.expires_at_unix_ms;
    }
    Ok(CommitmentEvidence {
        handles,
        source_handles,
        expires_at_unix_ms,
    })
}
