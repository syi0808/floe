//! The Commitments Expert's own execution.
//!
//! It states which views its judgment needs — communication, and, when granted,
//! confirmed memory, tasks and calendars — and how it composes them. Acquiring
//! each view stays behind the host port.

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::{AgentFailure, ContextSource};
use floe_context_contract::SourceReadOutcome;

use floe_context_contract::CommunicationView;
use floe_context_contract::{ContextIssueReason, ContextMemory, NativeContextView};

#[derive(serde::Deserialize)]
struct ConfirmedMemoryInput {
    memories: Vec<ContextMemory>,
    issue: Option<ContextIssueReason>,
}

use crate::commitments::{CommitmentsContextViews, run_commitments_expert_with_views};
use crate::shared::ExpertJudgment;
use crate::{
    BlockedExpertStatus, BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest,
    granted_context,
};

/// How much communication this Expert reads in one invocation.
const COMMUNICATION_LIMIT: usize = 25;

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    let mut context = granted_context(host, request);
    let mut task_views = request.staged_task_views.clone();
    if request.context_inputs_available {
        let simple_query = serde_json::json!({"schema_version": AGENT_VERSION});
        match crate::shared::read_declared_view::<_, ConfirmedMemoryInput>(
            host,
            request,
            "floe.source.confirmed-memory",
            simple_query.clone(),
        )
        .await
        {
            Ok(SourceReadOutcome::Ready(snapshot)) => {
                context.memories = snapshot.memories;
                floe_context_contract::record_source_issue(
                    &mut context.optional_context_issues,
                    ContextSource::Memory,
                    snapshot.issue,
                );
            }
            Ok(SourceReadOutcome::Unavailable(_)) | Err(AgentFailure::CapabilityUnavailable) => {
                floe_context_contract::record_source_issue(
                    &mut context.optional_context_issues,
                    ContextSource::Memory,
                    Some(ContextIssueReason::Unavailable),
                );
            }
            Ok(SourceReadOutcome::NeedsUserAction(_)) => {
                floe_context_contract::record_source_issue(
                    &mut context.optional_context_issues,
                    ContextSource::Memory,
                    Some(ContextIssueReason::NeedsUserAction),
                );
            }
            Err(error) => return Err(error),
        }
        match crate::shared::read_declared_view::<_, NativeContextView>(
            host,
            request,
            "floe.source.tasks",
            simple_query,
        )
        .await
        {
            Ok(SourceReadOutcome::Ready(view)) => task_views = vec![view],
            Ok(SourceReadOutcome::Unavailable(_)) | Err(AgentFailure::CapabilityUnavailable) => {
                floe_context_contract::record_source_issue(
                    &mut context.optional_context_issues,
                    ContextSource::Tasks,
                    Some(ContextIssueReason::Unavailable),
                );
            }
            Ok(SourceReadOutcome::NeedsUserAction(_)) => {
                floe_context_contract::record_source_issue(
                    &mut context.optional_context_issues,
                    ContextSource::Tasks,
                    Some(ContextIssueReason::NeedsUserAction),
                );
            }
            Err(error) => return Err(error),
        }
    }
    let source_view = match host
        .read_requirement(
            request,
            "floe.source.mail",
            serde_json::json!({
                "schema_version": AGENT_VERSION,
                "query": "",
                "cursor": 0,
                "limit": COMMUNICATION_LIMIT,
            }),
        )
        .await?
    {
        SourceReadOutcome::Ready(view) => view,
        SourceReadOutcome::Unavailable(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Commitments.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::Unavailable,
                "Mail is temporarily unavailable, so there are no commitment findings.".into(),
            );
        }
        SourceReadOutcome::NeedsUserAction(blockers) => {
            blockers
                .validate()
                .map_err(|_| AgentFailure::StaleContext)?;
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Commitments.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Mail access needs your review, so there are no commitment findings.".into(),
            );
        }
    };
    for dependency in source_view.dependencies() {
        host.record_dependency(request.task_id, request.task_id, dependency.clone())?;
    }
    let view: CommunicationView = serde_json::from_value(source_view.payload().clone())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let model = host.model();
    let calendars = crate::shared::optional_calendar_views(
        &mut context,
        crate::shared::read_declared_view(
            host,
            request,
            "floe.source.calendar",
            serde_json::to_value(request.nearby_calendar_query()?)
                .map_err(|_| AgentFailure::InvalidInput)?,
        )
        .await?,
    );
    let result = match run_commitments_expert_with_views(
        model,
        host.policy(),
        request.mail_invocation(context, view),
        CommitmentsContextViews {
            calendars,
            tasks: task_views,
        },
    )
    .await?
    {
        ExpertJudgment::Decided(result) => result,
        ExpertJudgment::Blocked(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Commitments.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Model approval needs your review, so there are no commitment findings.".into(),
            );
        }
    };
    drop(source_view);
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::Commitments.result_artifact_name(),
        super::RESULT_MEDIA_TYPE,
        result.summary.clone(),
        &result,
    )
}
