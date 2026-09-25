//! The Commitments Expert's own execution.
//!
//! It states which views its judgment needs — communication, and, when granted,
//! confirmed memory, tasks and calendars — and how it composes them. Acquiring
//! each view stays behind the host port.

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::{AgentFailure, ContextSource};
use floe_context_contract::{AuthorizedRead, HeldGrant, SourceReadOutcome};

use floe_context_contract::CommunicationView;

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
    let readable = host.conversation_context_available();
    let mut context = granted_context(host, request);
    if readable {
        let snapshot = host.memory_context().await?;
        context.memories = snapshot.memories;
        floe_context_contract::record_source_issue(
            &mut context.optional_context_issues,
            ContextSource::Memory,
            snapshot.issue,
        );
    }
    let mut task_views = host.staged_task_views().to_vec();
    if readable {
        let acquired =
            floe_context_contract::acquire_optional_source(ContextSource::Tasks, host.task_view())
                .await?;
        floe_context_contract::record_source_issue(
            &mut context.optional_context_issues,
            ContextSource::Tasks,
            acquired.issue.map(|issue| issue.reason),
        );
        task_views = acquired.value.into_iter().collect();
    }
    let source_view = match host
        .read_source_view(
            request,
            "mail.communication",
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
    for binding in source_view.bindings() {
        host.record_dependency(request.task_id, request.task_id, binding.dependency.clone())?;
    }
    let view: CommunicationView = serde_json::from_value(source_view.payload().clone())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let model = host.model();
    let calendars = crate::shared::optional_calendar_views(
        &mut context,
        host.calendar_views(request, request.nearby_calendar_query()?)
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
