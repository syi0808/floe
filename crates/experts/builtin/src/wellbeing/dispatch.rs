//! The Wellbeing Expert's own execution.

use floe_agent_contract::AgentFailure;

use crate::wellbeing::{
    WellbeingContextViews, WellbeingExpertResult, run_wellbeing_expert_with_views,
};
use crate::{BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest, granted_context};

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    let wellbeing = host.wellbeing_view(request).await?;
    let mut context = granted_context(host, request);
    let calendars = crate::shared::optional_calendar_views(
        &mut context,
        host.calendar_views(request, request.nearby_calendar_query()?)
            .await?,
    );
    let result: WellbeingExpertResult = run_wellbeing_expert_with_views(
        host.model(),
        host.policy(),
        request.personal_invocation(context),
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
