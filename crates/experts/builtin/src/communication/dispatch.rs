//! The Communication Expert's own execution.
//!
//! It needs one view — granted communication — and the paired server model.

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::AgentFailure;
use floe_context_contract::{AuthorizedRead, HeldGrant, SourceReadOutcome};

use floe_context_contract::CommunicationView;

use crate::communication::{CommunicationExpertResult, run_communication_expert};
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
                crate::BuiltinExpertKind::Communication.result_artifact_name(),
                BlockedExpertStatus::Unavailable,
                "Mail is temporarily unavailable, so there is no communication assessment.".into(),
            );
        }
        SourceReadOutcome::NeedsUserAction(blockers) => {
            blockers.validate().map_err(|_| AgentFailure::StaleContext)?;
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Communication.result_artifact_name(),
                BlockedExpertStatus::NeedsUserAction,
                "Mail access needs your review, so there is no communication assessment.".into(),
            );
        }
    };
    for binding in source_view.bindings() {
        host.record_dependency(request.task_id, request.task_id, binding.dependency.clone())?;
    }
    let view: CommunicationView = serde_json::from_value(source_view.payload().clone())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let result: CommunicationExpertResult = run_communication_expert(
        model,
        host.policy(),
        request.mail_invocation(granted_context(host, request), view),
    )
    .await?;
    drop(source_view);
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::Communication.result_artifact_name(),
        result.summary.clone(),
        &result,
    )
}
