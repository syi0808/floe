//! The Wellbeing Expert's own execution.

use floe_agent_contract::AgentFailure;
use floe_context_contract::SourceReadOutcome;

use crate::shared::ExpertJudgment;
use crate::wellbeing::{WellbeingContextViews, run_wellbeing_expert_with_views};
use crate::{
    BlockedExpertStatus, BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest,
    granted_context,
};

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    let wellbeing = match host.wellbeing_view(request).await? {
        SourceReadOutcome::Ready(view) => view,
        SourceReadOutcome::Unavailable(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Wellbeing.result_artifact_name(),
                BlockedExpertStatus::Unavailable,
                "Wellbeing is temporarily unavailable, so there is no wellbeing assessment.".into(),
            );
        }
        SourceReadOutcome::NeedsUserAction(blockers) => {
            blockers
                .validate()
                .map_err(|_| AgentFailure::StaleContext)?;
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Wellbeing.result_artifact_name(),
                BlockedExpertStatus::NeedsUserAction,
                "Wellbeing access needs your review, so there is no wellbeing assessment.".into(),
            );
        }
    };
    let mut context = granted_context(host, request);
    let calendars = crate::shared::optional_calendar_views(
        &mut context,
        host.calendar_views(request, request.nearby_calendar_query()?)
            .await?,
    );
    let result = match run_wellbeing_expert_with_views(
        host.model(),
        host.policy(),
        request.personal_invocation(context),
        WellbeingContextViews {
            wellbeing,
            calendars,
        },
    )
    .await?
    {
        ExpertJudgment::Decided(result) => result,
        ExpertJudgment::Blocked(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Wellbeing.result_artifact_name(),
                BlockedExpertStatus::NeedsUserAction,
                "Model approval needs your review, so there is no wellbeing assessment.".into(),
            );
        }
    };
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::Wellbeing.result_artifact_name(),
        result.summary.clone(),
        &result,
    )
}
