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

/// This Expert reads attention under its own consumer identity.
pub const CONSUMER: &str = "attention.expert";

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    let (attention, dependency) = match host.attention_view(request).await? {
        SourceReadOutcome::Ready(read) => read,
        SourceReadOutcome::Unavailable(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::FocusAttention.result_artifact_name(),
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
                BlockedExpertStatus::NeedsUserAction,
                "Attention access needs your review, so there is no focus assessment.".into(),
            );
        }
    };
    host.record_dependency(request.task_id, request.task_id, dependency)?;
    let mut context = granted_context(host, request);
    let calendars = crate::shared::optional_calendar_views(
        &mut context,
        host.calendar_views(request, request.nearby_calendar_query()?)
            .await?,
    );
    // Work context enriches but never gates: an optional blocker is
    // preserved by the host while reasoning continues over admitted evidence.
    let active_work = match host.work_context_views(request).await? {
        SourceReadOutcome::Ready(views) => views,
        SourceReadOutcome::Unavailable(_) => vec![],
        SourceReadOutcome::NeedsUserAction(blockers) => {
            blockers
                .validate()
                .map_err(|_| AgentFailure::StaleContext)?;
            vec![]
        }
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
                BlockedExpertStatus::NeedsUserAction,
                "Model approval needs your review, so there is no focus assessment.".into(),
            );
        }
    };
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::FocusAttention.result_artifact_name(),
        result.summary.clone(),
        &result,
    )
}
