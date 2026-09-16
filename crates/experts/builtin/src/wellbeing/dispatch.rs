//! The Wellbeing Expert's own execution.

use floe_agent_contract::AgentFailure;

use crate::wellbeing::{
    WellbeingContextViews, WellbeingExpertResult, run_wellbeing_expert_with_views,
};
use crate::{
    BuiltinContextSource, BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest,
    granted_context,
};

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    crate::require_mandatory_source(host, request)?;
    let wellbeing = host.wellbeing_view(request).await?;
    let calendars = if host.source_granted(&request.agent_id, BuiltinContextSource::Calendar) {
        host.calendar_views(request).await?
    } else {
        vec![]
    };
    let result: WellbeingExpertResult = run_wellbeing_expert_with_views(
        host.model(),
        host.policy(),
        request.personal_invocation(granted_context(host, request)),
        WellbeingContextViews {
            wellbeing,
            calendars,
        },
    )
    .await?;
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::Wellbeing.result_artifact_name(),
        result.summary.clone(),
        &result,
    )
}
