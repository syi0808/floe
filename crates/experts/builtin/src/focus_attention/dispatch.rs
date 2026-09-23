//! The Focus & Attention Expert's own execution.
//!
//! Attention is coarse on-device context, so this Expert only runs on the
//! device model, and it adds schedule and active work only when granted.

use floe_agent_contract::AgentFailure;

use crate::focus_attention::{FocusContextViews, FocusExpertResult, run_focus_expert_with_views};
use crate::{BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest, granted_context};

/// This Expert reads attention under its own consumer identity.
pub const CONSUMER: &str = "attention.expert";

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    let (attention, dependency) = host.attention_view(request).await?;
    host.record_dependency(request.task_id, request.task_id, dependency)?;
    let mut context = granted_context(host, request);
    let calendars = crate::shared::optional_calendar_views(
        &mut context,
        host.calendar_views(request, request.nearby_calendar_query()?)
            .await?,
    );
    let active_work = host.work_context_views(request).await?;
    let result: FocusExpertResult = run_focus_expert_with_views(
        host.model(),
        host.policy(),
        request.personal_invocation(context),
        FocusContextViews {
            attention,
            calendars,
            active_work,
        },
    )
    .await?;
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::FocusAttention.result_artifact_name(),
        result.summary.clone(),
        &result,
    )
}
