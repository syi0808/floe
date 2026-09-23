//! The Life Logistics Expert's own execution.

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::AgentFailure;
use floe_context_contract::{AuthorizedRead, HeldGrant};

use floe_context_contract::LogisticsView;

use crate::life_logistics::{LifeLogisticsExpertResult, run_life_logistics_expert};
use crate::{BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest, granted_context};

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    let source_view = host
        .read_source_view(
            request,
            "life.logistics",
            serde_json::json!({ "schema_version": AGENT_VERSION }),
        )
        .await?;
    host.record_dependency(
        request.task_id,
        request.task_id,
        source_view.dependency().clone(),
    )?;
    let view: LogisticsView = serde_json::from_value(source_view.payload().clone())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let model = host.model();
    let result: LifeLogisticsExpertResult = run_life_logistics_expert(
        model,
        host.policy(),
        request.portfolio_invocation(granted_context(host, request)),
        view,
    )
    .await?;
    drop(source_view);
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::LifeLogistics.result_artifact_name(),
        result.summary.clone(),
        &result,
    )
}
