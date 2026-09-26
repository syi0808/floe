//! The Work Context Expert's own execution.

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::AgentFailure;
use floe_context_contract::SourceReadOutcome;

use floe_context_contract::WorkContextView;

use crate::shared::ExpertJudgment;
use crate::work_context::run_work_context_expert;
use crate::{
    BlockedExpertStatus, BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest,
    granted_context,
};

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    let source_view = match host
        .read_requirement(
            request,
            "floe.source.work-context",
            serde_json::json!({ "schema_version": AGENT_VERSION }),
        )
        .await?
    {
        SourceReadOutcome::Ready(view) => view,
        SourceReadOutcome::Unavailable(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::WorkContext.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::Unavailable,
                "Work context is temporarily unavailable, so there is no work assessment.".into(),
            );
        }
        SourceReadOutcome::NeedsUserAction(blockers) => {
            blockers
                .validate()
                .map_err(|_| AgentFailure::StaleContext)?;
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::WorkContext.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Work context access needs your review, so there is no work assessment.".into(),
            );
        }
    };
    for dependency in source_view.dependencies() {
        host.record_dependency(request.task_id, request.task_id, dependency.clone())?;
    }
    let view: WorkContextView = serde_json::from_value(source_view.payload().clone())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let model = host.model();
    let result = match run_work_context_expert(
        model,
        host.policy(),
        request.portfolio_invocation(granted_context(host, request)),
        view,
    )
    .await?
    {
        ExpertJudgment::Decided(result) => result,
        ExpertJudgment::Blocked(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::WorkContext.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Model approval needs your review, so there is no work assessment.".into(),
            );
        }
    };
    drop(source_view);
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::WorkContext.result_artifact_name(),
        super::RESULT_MEDIA_TYPE,
        result.summary.clone(),
        &result,
    )
}
