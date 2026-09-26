//! The Life Logistics Expert's own execution.

use crate::RequirementReadOutcome;
use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::AgentFailure;

use floe_context_contract::LogisticsView;

use crate::life_logistics::run_life_logistics_expert;
use crate::shared::ExpertJudgment;
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
            "floe.source.logistics",
            serde_json::json!({ "schema_version": AGENT_VERSION }),
        )
        .await?
    {
        RequirementReadOutcome::Ready(view) => view,
        RequirementReadOutcome::Unavailable(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::LifeLogistics.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::Unavailable,
                "Logistics are temporarily unavailable, so there is no logistics plan.".into(),
            );
        }
        RequirementReadOutcome::NeedsUserAction => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::LifeLogistics.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Logistics access needs your review, so there is no logistics plan.".into(),
            );
        }
    };
    for dependency in source_view.dependencies() {
        host.record_dependency(request.task_id, request.task_id, dependency.clone())?;
    }
    let view: LogisticsView = serde_json::from_value(source_view.payload().clone())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let model = host.model();
    let result = match run_life_logistics_expert(
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
                crate::BuiltinExpertKind::LifeLogistics.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Model approval needs your review, so there is no logistics plan.".into(),
            );
        }
    };
    drop(source_view);
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::LifeLogistics.result_artifact_name(),
        super::RESULT_MEDIA_TYPE,
        result.summary.clone(),
        &result,
    )
}
