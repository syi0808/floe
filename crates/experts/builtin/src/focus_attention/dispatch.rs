//! The Focus & Attention Expert's own execution.
//!
//! Attention is coarse on-device context, so this Expert only runs on the
//! device model, and it adds schedule and active work only when granted.

use floe_agent_contract::AgentFailure;

use crate::focus_attention::{FocusContextViews, FocusExpertResult, run_focus_expert_with_views};
use crate::{
    BuiltinContextSource, BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest,
    granted_context,
};

/// This Expert reads attention under its own consumer identity.
pub const CONSUMER: &str = "attention.expert";

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    crate::require_mandatory_source(host, request)?;
    let (attention, dependency) = host.attention_view(request).await?;
    host.record_dependency(request.invocation_id, request.invocation_id, dependency)?;
    let calendars = if host.source_granted(&request.agent_id, BuiltinContextSource::Calendar) {
        host.calendar_views(request).await?
    } else {
        vec![]
    };
    let active_work = if host.source_granted(&request.agent_id, BuiltinContextSource::WorkContext) {
        host.work_context_views(request).await?
    } else {
        vec![]
    };
    let result: FocusExpertResult = run_focus_expert_with_views(
        host.model(),
        host.policy(),
        request.personal_invocation(granted_context(host, request)),
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
