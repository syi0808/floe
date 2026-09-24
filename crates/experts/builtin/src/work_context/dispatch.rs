//! The Work Context Expert's own execution.

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::AgentFailure;
use floe_context_contract::{AuthorizedRead, HeldGrant};

use floe_context_contract::WorkContextView;

use crate::work_context::{WorkContextExpertResult, run_work_context_expert};
use crate::{BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest, granted_context};

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    let source_view = host
        .read_source_view(
            request,
            "work.context",
            serde_json::json!({ "schema_version": AGENT_VERSION }),
        )
        .await?;
    for binding in source_view.bindings() {
        host.record_dependency(request.task_id, request.task_id, binding.dependency.clone())?;
    }
    let view: WorkContextView = serde_json::from_value(source_view.payload().clone())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let model = host.model();
    let result: WorkContextExpertResult = run_work_context_expert(
        model,
        host.policy(),
        request.portfolio_invocation(granted_context(host, request)),
        view,
    )
    .await?;
    drop(source_view);
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::WorkContext.result_artifact_name(),
        result.summary.clone(),
        &result,
    )
}
