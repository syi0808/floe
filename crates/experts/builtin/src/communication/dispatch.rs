//! The Communication Expert's own execution.
//!
//! It needs one view — granted communication — and the paired server model.

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::AgentFailure;
use floe_context_contract::SourceReadOutcome;

use floe_context_contract::CommunicationView;

use crate::communication::run_communication_expert;
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
    let model = host.model();
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
                crate::BuiltinExpertKind::Communication.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::Unavailable,
                "Mail is temporarily unavailable, so there is no communication assessment.".into(),
            );
        }
        SourceReadOutcome::NeedsUserAction(blockers) => {
            blockers
                .validate()
                .map_err(|_| AgentFailure::StaleContext)?;
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Communication.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Mail access needs your review, so there is no communication assessment.".into(),
            );
        }
    };
    for dependency in source_view.dependencies() {
        host.record_dependency(request.task_id, request.task_id, dependency.clone())?;
    }
    let view: CommunicationView = serde_json::from_value(source_view.payload().clone())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let result = match run_communication_expert(
        model,
        host.policy(),
        request.mail_invocation(granted_context(host, request), view),
    )
    .await?
    {
        ExpertJudgment::Decided(result) => result,
        ExpertJudgment::Blocked(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Communication.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Model approval needs your review, so there is no communication assessment.".into(),
            );
        }
    };
    drop(source_view);
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::Communication.result_artifact_name(),
        super::RESULT_MEDIA_TYPE,
        result.summary.clone(),
        &result,
    )
}
