//! The Wellbeing Expert's own execution.

use crate::RequirementReadOutcome;
use floe_agent_contract::AgentFailure;

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
    let wellbeing = match crate::shared::read_declared_view(
        host,
        request,
        "floe.source.wellbeing",
        serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION}),
    )
    .await?
    {
        RequirementReadOutcome::Ready(view) => view,
        RequirementReadOutcome::Unavailable(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Wellbeing.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::Unavailable,
                "Wellbeing is temporarily unavailable, so there is no wellbeing assessment.".into(),
            );
        }
        RequirementReadOutcome::NeedsUserAction => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Wellbeing.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Wellbeing access needs your review, so there is no wellbeing assessment.".into(),
            );
        }
    };
    let mut context = granted_context(host, request);
    let calendars = crate::shared::optional_calendar_views(
        &mut context,
        crate::shared::read_declared_view(
            host,
            request,
            "floe.source.calendar",
            serde_json::to_value(request.nearby_calendar_query()?)
                .map_err(|_| AgentFailure::InvalidInput)?,
        )
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
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Model approval needs your review, so there is no wellbeing assessment.".into(),
            );
        }
    };
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::Wellbeing.result_artifact_name(),
        super::RESULT_MEDIA_TYPE,
        result.summary.clone(),
        &result,
    )
}
