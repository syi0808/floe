//! The Work Context Expert's own execution.

use floe_agent_contract::AgentFailure;
use floe_kernel::AGENT_VERSION;

use crate::{
    BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest, WorkContextExpertResult,
    WorkContextView, granted_context, run_work_context_expert,
};

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    crate::require_mandatory_source(host, request)?;
    let source_view = host
        .read_source_view(
            request,
            "work.context",
            serde_json::json!({ "schema_version": AGENT_VERSION }),
        )
        .await?;
    host.record_dependency(
        request.invocation_id,
        request.invocation_id,
        source_view.dependency().clone(),
    )?;
    let view: WorkContextView = serde_json::from_value(source_view.payload().clone())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let model = host
        .server_model()
        .ok_or(AgentFailure::CapabilityUnavailable)?;
    let result: WorkContextExpertResult = run_work_context_expert(
        model,
        host.policy(),
        request.portfolio_invocation(granted_context(host, request)),
        view,
    )
    .await?;
    drop(source_view);
    BuiltinExpertOutput::from_result(result.summary.clone(), &result)
}
