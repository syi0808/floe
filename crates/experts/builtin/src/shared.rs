//! Values and model calls shared by the builtin Experts.
//!
//! Each Expert owns its own judgment and result shape; what is common is how a
//! bounded assignment reaches the model and how its answer is checked back.

use floe_agent_contract::PersonId;
use serde::{Deserialize, de::DeserializeOwned};
use tokio::time::Instant;
use uuid::Uuid;

use crate::RequirementReadOutcome;
use floe_agent_contract::prompts::PromptAssembly;
use floe_agent_contract::{
    AGENT_VERSION, AgentFailure, ExpertModel, ExpertModelAnswer, ExpertModelCall,
    ExpertModelOutcome, ExpertModelRequirement, SessionProtection,
};
use floe_agent_contract::{AgentContext, InferencePolicyDecision};
use floe_context_contract::{
    CalendarContextView, CommunicationView, ContextEvidence, ContextIssueReason, ContextSource,
    MAX_COMMUNICATION_BYTES, MAX_COMMUNICATION_ITEMS, ProcessingRequirement,
    calendar_context_evidence, communication_context_evidence, validate_calendar_context_view,
    validate_communication_view,
};

pub(crate) async fn read_declared_view<Host, View>(
    host: &Host,
    request: &crate::BuiltinExpertRequest,
    key: &str,
    query: serde_json::Value,
) -> Result<RequirementReadOutcome<View>, AgentFailure>
where
    Host: crate::BuiltinExpertHost + ?Sized,
    View: DeserializeOwned,
{
    Ok(match host.read_requirement(request, key, query).await? {
        RequirementReadOutcome::Ready(read) => {
            for dependency in read.dependencies() {
                host.record_dependency(request.task_id, request.task_id, dependency.clone())?;
            }
            RequirementReadOutcome::Ready(
                serde_json::from_value(read.payload().clone())
                    .map_err(|_| AgentFailure::CapabilityUnavailable)?,
            )
        }
        RequirementReadOutcome::Unavailable(reason) => RequirementReadOutcome::Unavailable(reason),
        RequirementReadOutcome::NeedsUserAction => RequirementReadOutcome::NeedsUserAction,
    })
}

pub(crate) fn optional_calendar_views(
    context: &mut AgentContext,
    outcome: RequirementReadOutcome<Vec<CalendarContextView>>,
) -> Vec<CalendarContextView> {
    let (views, issue) = match outcome {
        RequirementReadOutcome::Ready(views) => (views, None),
        RequirementReadOutcome::Unavailable(_) => (vec![], Some(ContextIssueReason::Unavailable)),
        // The requirement itself is preserved by the host, which publishes it
        // under the admitted origin; the context records that user action —
        // not a denial — is what the source needs.
        RequirementReadOutcome::NeedsUserAction => {
            (vec![], Some(ContextIssueReason::NeedsUserAction))
        }
    };
    floe_context_contract::record_source_issue(
        &mut context.optional_context_issues,
        ContextSource::Calendar,
        issue,
    );
    views
}

/// How many findings one communication-backed Expert may report.
pub(crate) const MAX_MAIL_EXPERT_FINDINGS: usize = 16;

/// One assignment handed to an Expert that reads a communication view.
pub struct MailExpertInvocation {
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

/// One assignment handed to an Expert that reads a work or logistics view.
pub struct PortfolioExpertInvocation {
    pub person_id: PersonId,
    pub invocation_id: Uuid,
    pub assignment: String,
    pub current_time_unix_ms: i64,
    pub context: AgentContext,
    pub max_output_bytes: usize,
    pub max_model_tokens: u64,
    pub max_model_cost_micros: u64,
    pub deadline: Instant,
    pub cancellation: floe_execution::Cancellation,
}

/// One assignment handed to an Expert that reads the Person's own views.
pub struct PersonalExpertInvocation {
    pub person_id: PersonId,
    pub invocation_id: Uuid,
    pub assignment: String,
    pub current_time_unix_ms: i64,
    pub context: AgentContext,
    pub max_output_bytes: usize,
    pub max_model_tokens: u64,
    pub max_model_cost_micros: u64,
    pub deadline: Instant,
    pub cancellation: floe_execution::Cancellation,
}

/// Whether one assignment is inside the bounds every Expert shares.
fn admissible(
    assignment: &str,
    max_output_bytes: usize,
    max_model_tokens: u64,
    deadline: Instant,
    cancellation: &floe_execution::Cancellation,
) -> Result<(), AgentFailure> {
    if !valid_text(assignment, 2048)
        || max_output_bytes == 0
        || max_model_tokens == 0
        || deadline <= Instant::now()
        || cancellation.is_cancelled()
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

/// What one Expert model call produced for its caller: a decoded judgment,
/// or the trusted requirement the host must publish before the Expert can
/// proceed. The Expert reports a blocked-domain judgment and stops; it
/// never proposes, alters, or publishes the requirement itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpertJudgment<Output> {
    Decided(Output),
    Blocked(ProcessingRequirement),
}

/// The one bounded model call an Expert makes, stating what execution class it
/// requires. The pre-check validates the context shape only; transfer
/// authority is the Access dispatch fence inside canonical Inference, never
/// this call. A blocked dispatch passes through untouched: bounds apply to
/// an answer, and a blockage carries no usage.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_expert_model<Model: ExpertModel>(
    model: &Model,
    policy: &InferencePolicyDecision,
    requirement: ExpertModelRequirement,
    person_id: PersonId,
    invocation_id: Uuid,
    assignment: &str,
    current_time_unix_ms: i64,
    context: AgentContext,
    prompt: PromptAssembly,
    max_model_tokens: u64,
    max_model_cost_micros: u64,
    max_output_bytes: usize,
    deadline: Instant,
    cancellation: &floe_execution::Cancellation,
) -> Result<ExpertModelOutcome, AgentFailure> {
    policy.authorize(
        floe_agent_contract::ModelPlacement::DeviceLocal,
        SessionProtection::Encrypted,
        &context,
        u64::try_from(current_time_unix_ms).map_err(|_| AgentFailure::InvalidInput)?,
    )?;
    let outcome = model
        .answer(ExpertModelCall {
            person_id,
            invocation_id,
            prompt,
            policy: policy.clone(),
            context,
            assignment: assignment.to_owned(),
            requirement,
            max_output_bytes: max_output_bytes.min(8192),
            max_tokens: max_model_tokens,
            max_cost_micros: max_model_cost_micros,
            deadline,
            cancellation: cancellation.clone(),
        })
        .await?;
    match &outcome {
        ExpertModelOutcome::Answered(answer)
            if answer.schema_version != AGENT_VERSION
                || answer.used_tokens > max_model_tokens
                || answer.cost_micros > max_model_cost_micros =>
        {
            Err(AgentFailure::BudgetExceeded)
        }
        _ => Ok(outcome),
    }
}

/// Run the model for an Expert whose evidence is one communication view.
pub(crate) async fn run_mail_model<Model: ExpertModel>(
    model: &Model,
    policy: &InferencePolicyDecision,
    requirement: ExpertModelRequirement,
    invocation: &MailExpertInvocation,
    prompt: PromptAssembly,
    mut context: AgentContext,
) -> Result<ExpertJudgment<ExpertModelAnswer>, AgentFailure> {
    admissible(
        &invocation.assignment,
        invocation.max_output_bytes,
        invocation.max_model_tokens,
        invocation.deadline,
        &invocation.cancellation,
    )?;
    validate_communication_view(
        &invocation.view,
        invocation.current_time_unix_ms,
        MAX_COMMUNICATION_ITEMS,
        MAX_COMMUNICATION_BYTES,
    )?;
    context
        .evidence
        .push(communication_context_evidence(&invocation.view)?);
    let outcome = run_expert_model(
        model,
        policy,
        requirement,
        invocation.person_id,
        invocation.invocation_id,
        &invocation.assignment,
        invocation.current_time_unix_ms,
        context,
        prompt,
        invocation.max_model_tokens,
        invocation.max_model_cost_micros,
        invocation.max_output_bytes,
        invocation.deadline,
        &invocation.cancellation,
    )
    .await?;
    Ok(match outcome {
        ExpertModelOutcome::Answered(answer) => ExpertJudgment::Decided(answer),
        ExpertModelOutcome::Blocked(requirement) => ExpertJudgment::Blocked(requirement),
    })
}

/// Run the model for an Expert whose evidence is one portfolio view.
pub(crate) async fn run_portfolio_model<Output: DeserializeOwned, Model: ExpertModel>(
    model: &Model,
    policy: &InferencePolicyDecision,
    requirement: ExpertModelRequirement,
    invocation: &PortfolioExpertInvocation,
    evidence: ContextEvidence,
    prompt: PromptAssembly,
) -> Result<ExpertJudgment<Output>, AgentFailure> {
    admissible(
        &invocation.assignment,
        invocation.max_output_bytes,
        invocation.max_model_tokens,
        invocation.deadline,
        &invocation.cancellation,
    )?;
    let mut context = invocation.context.clone();
    context.evidence.push(evidence);
    let outcome = run_expert_model(
        model,
        policy,
        requirement,
        invocation.person_id,
        invocation.invocation_id,
        &invocation.assignment,
        invocation.current_time_unix_ms,
        context,
        prompt,
        invocation.max_model_tokens,
        invocation.max_model_cost_micros,
        invocation.max_output_bytes,
        invocation.deadline,
        &invocation.cancellation,
    )
    .await?;
    match outcome {
        ExpertModelOutcome::Answered(answer) => Ok(ExpertJudgment::Decided(decode_answer(
            &answer,
            invocation.max_output_bytes,
        )?)),
        ExpertModelOutcome::Blocked(requirement) => Ok(ExpertJudgment::Blocked(requirement)),
    }
}

/// Run the model for an Expert whose evidence is the Person's own views.
pub(crate) async fn run_personal_model<Output: DeserializeOwned, Model: ExpertModel>(
    model: &Model,
    policy: &InferencePolicyDecision,
    requirement: ExpertModelRequirement,
    invocation: &PersonalExpertInvocation,
    evidence: Vec<ContextEvidence>,
    prompt: PromptAssembly,
) -> Result<ExpertJudgment<Output>, AgentFailure> {
    admissible(
        &invocation.assignment,
        invocation.max_output_bytes,
        invocation.max_model_tokens,
        invocation.deadline,
        &invocation.cancellation,
    )?;
    let mut context = invocation.context.clone();
    context.evidence.extend(evidence);
    let outcome = run_expert_model(
        model,
        policy,
        requirement,
        invocation.person_id,
        invocation.invocation_id,
        &invocation.assignment,
        invocation.current_time_unix_ms,
        context,
        prompt,
        invocation.max_model_tokens,
        invocation.max_model_cost_micros,
        invocation.max_output_bytes,
        invocation.deadline,
        &invocation.cancellation,
    )
    .await?;
    match outcome {
        ExpertModelOutcome::Answered(answer) => Ok(ExpertJudgment::Decided(decode_answer(
            &answer,
            invocation.max_output_bytes,
        )?)),
        ExpertModelOutcome::Blocked(requirement) => Ok(ExpertJudgment::Blocked(requirement)),
    }
}

/// The judgment an Expert's one answer carries.
pub(crate) fn decode_answer<Output: for<'de> Deserialize<'de>>(
    answer: &ExpertModelAnswer,
    maximum_bytes: usize,
) -> Result<Output, AgentFailure> {
    if answer.answer.len() > maximum_bytes.min(8192) {
        return Err(AgentFailure::BudgetExceeded);
    }
    serde_json::from_str(&answer.answer).map_err(|_| AgentFailure::InvalidModelOutput)
}

/// Add the calendar views an Expert was granted to its evidence, keeping every
/// source and handle distinct and the whole view's freshness bounded.
pub(crate) fn add_schedule_views(
    calendars: &[CalendarContextView],
    now_unix_ms: i64,
    evidence: &mut Vec<ContextEvidence>,
    available: &mut Vec<String>,
    source_handles: &mut Vec<String>,
    expires_at_unix_ms: &mut i64,
) -> Result<(), AgentFailure> {
    for view in calendars {
        validate_calendar_context_view(view, now_unix_ms)?;
        ensure_unique_source(source_handles, &view.source_handle)?;
        extend_unique_handles(
            available,
            view.items.iter().map(|item| &item.evidence_handle),
        )?;
        source_handles.push(view.source_handle.clone());
        *expires_at_unix_ms = (*expires_at_unix_ms).min(view.expires_at_unix_ms);
        evidence.push(calendar_context_evidence(view)?);
    }
    Ok(())
}

pub(crate) fn ensure_unique_source(
    source_handles: &[String],
    source: &str,
) -> Result<(), AgentFailure> {
    if source_handles.iter().any(|value| value == source) {
        Err(AgentFailure::InvalidInput)
    } else {
        Ok(())
    }
}

pub(crate) fn extend_unique_handles<'a>(
    available: &mut Vec<String>,
    handles: impl Iterator<Item = &'a String>,
) -> Result<(), AgentFailure> {
    for handle in handles {
        if available.contains(handle) {
            return Err(AgentFailure::InvalidInput);
        }
        available.push(handle.clone());
    }
    Ok(())
}

pub(crate) fn valid_handle(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128
}

pub(crate) fn valid_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum
}

pub(crate) fn validate_summary(summary: &str) -> Result<(), AgentFailure> {
    if valid_text(summary, 2048) {
        Ok(())
    } else {
        Err(AgentFailure::InvalidModelOutput)
    }
}

/// A judgment must cite evidence it was actually shown, or say it reached no
/// conclusion. It cannot do both, and it cannot do neither.
pub(crate) fn validate_judgment(
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

#[cfg(test)]
mod calendar_outcome_tests {
    use super::*;
    use floe_context_contract::SourceUnavailable;

    fn context() -> AgentContext {
        AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        }
    }

    #[test]
    fn optional_calendar_absence_is_recorded_instead_of_looking_empty() {
        let mut context = context();
        let views = optional_calendar_views(
            &mut context,
            RequirementReadOutcome::Unavailable(SourceUnavailable::TemporarilyUnavailable),
        );
        assert!(views.is_empty());
        assert_eq!(context.optional_context_issues.len(), 1);
        assert_eq!(
            context.optional_context_issues[0].source,
            ContextSource::Calendar
        );
        assert_eq!(
            context.optional_context_issues[0].reason,
            ContextIssueReason::Unavailable
        );

        optional_calendar_views(&mut context, RequirementReadOutcome::NeedsUserAction);
        assert_eq!(context.optional_context_issues.len(), 1);
        assert_eq!(
            context.optional_context_issues[0].reason,
            ContextIssueReason::NeedsUserAction
        );

        optional_calendar_views(&mut context, RequirementReadOutcome::Ready(vec![]));
        assert!(context.optional_context_issues.is_empty());
    }
}
