//! The Communication Expert's own execution.
//!
//! It needs one view — granted communication — and the paired server model.

use floe_agent_contract::AgentFailure;
use floe_kernel::AGENT_VERSION;

use floe_context::CommunicationView;

use crate::communication::{CommunicationExpertResult, run_communication_expert};
use crate::{BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest, granted_context};

/// How much communication this Expert reads in one invocation.
const COMMUNICATION_LIMIT: usize = 25;

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    crate::require_mandatory_source(host, request)?;
    let model = host
        .server_model()
        .ok_or(AgentFailure::CapabilityUnavailable)?;
    let source_view = host
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
        .await?;
    host.record_dependency(
        request.invocation_id,
        request.invocation_id,
        source_view.dependency().clone(),
    )?;
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
