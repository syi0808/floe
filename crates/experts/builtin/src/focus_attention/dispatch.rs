//! The Focus & Attention Expert's own execution.
//!
//! Attention is coarse on-device context, so this Expert only runs on the
//! device model, and it adds schedule and active work only when granted.

use floe_agent_contract::AgentFailure;
use floe_context_contract::SourceReadOutcome;

use crate::focus_attention::{FocusContextViews, run_focus_expert_with_views};
use crate::shared::ExpertJudgment;
use crate::{
    BlockedExpertStatus, BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest,
    granted_context,
};

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    let attention = match crate::shared::read_declared_view(
        host,
        request,
        "floe.source.attention",
        serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION}),
    )
    .await?
    {
        SourceReadOutcome::Ready(read) => read,
        SourceReadOutcome::Unavailable(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::FocusAttention.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::Unavailable,
                "Attention is temporarily unavailable, so there is no focus assessment.".into(),
            );
        }
        SourceReadOutcome::NeedsUserAction(blockers) => {
            blockers
                .validate()
                .map_err(|_| AgentFailure::StaleContext)?;
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::FocusAttention.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Attention access needs your review, so there is no focus assessment.".into(),
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
    // Work context enriches but never gates: an optional blocker is
    // preserved by the host while reasoning continues over admitted evidence.
    let active_work = match crate::shared::read_declared_view(
        host,
        request,
        "floe.source.work-context",
        serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION}),
    )
    .await
    {
        Ok(SourceReadOutcome::Ready(view)) => vec![view],
        Ok(SourceReadOutcome::Unavailable(_)) | Err(AgentFailure::CapabilityUnavailable) => vec![],
        Ok(SourceReadOutcome::NeedsUserAction(blockers)) => {
            blockers
                .validate()
                .map_err(|_| AgentFailure::StaleContext)?;
            vec![]
        }
        Err(error) => return Err(error),
    };
    let result = match run_focus_expert_with_views(
        host.model(),
        host.policy(),
        request.personal_invocation(context),
        FocusContextViews {
            attention,
            calendars,
            active_work,
        },
    )
    .await?
    {
        ExpertJudgment::Decided(result) => result,
        ExpertJudgment::Blocked(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::FocusAttention.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Model approval needs your review, so there is no focus assessment.".into(),
            );
        }
    };
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::FocusAttention.result_artifact_name(),
        super::RESULT_MEDIA_TYPE,
        result.summary.clone(),
        &result,
    )
}
