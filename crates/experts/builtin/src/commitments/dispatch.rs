//! The Commitments Expert's own execution.
//!
//! It states which views its judgment needs — communication, and, when granted,
//! confirmed memory, tasks and calendars — and how it composes them. Acquiring
//! each view stays behind the host port.

use floe_agent_contract::{AgentFailure, ContextSource};
use floe_kernel::AGENT_VERSION;

use floe_context::CommunicationView;

use crate::commitments::{
    CommitmentsContextViews, CommitmentsExpertResult, run_commitments_expert_with_views,
};
use crate::{
    BuiltinContextSource, BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest,
    granted_context,
};

/// How much communication this Expert reads in one invocation.
const COMMUNICATION_LIMIT: usize = 25;

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    crate::require_mandatory_source(host, request)?;
    let readable = host.conversation_context_available();
    let memory_granted = readable
        && host.source_granted(&request.agent_id, BuiltinContextSource::ConfirmedMemory);
    let tasks_granted = readable
        && host.source_granted(&request.agent_id, BuiltinContextSource::Tasks);
    let mut context = granted_context(host, request);
    if memory_granted {
        let snapshot = host.memory_context().await?;
        context.memories = snapshot.memories;
        floe_context::record_source_issue(
            &mut context.optional_context_issues,
            ContextSource::Memory,
            snapshot.issue,
        );
    }
    let mut task_views = host.staged_task_views().to_vec();
    if tasks_granted {
        let acquired =
            floe_context::acquire_optional_source(ContextSource::Tasks, host.task_view()).await?;
        floe_context::record_source_issue(
            &mut context.optional_context_issues,
            ContextSource::Tasks,
            acquired.issue.map(|issue| issue.reason),
        );
        task_views = acquired.value.into_iter().collect();
    }
    let source_view = host
        .read_source_view(
            request,
            "mail.communication",
            serde_json::json!({
                "schema_version": AGENT_VERSION,
                "query": "",
                "cursor": 0,
                "limit": COMMUNICATION_LIMIT,
            }),
        )
        .await?;
    host.record_dependency(
        request.invocation_id,
        request.invocation_id,
        source_view.dependency().clone(),
    )?;
    let view: CommunicationView = serde_json::from_value(source_view.payload().clone())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let model = host
        .server_model()
        .ok_or(AgentFailure::CapabilityUnavailable)?;
    let calendars = if host.source_granted(&request.agent_id, BuiltinContextSource::Calendar) {
        host.calendar_views(request).await?
    } else {
        vec![]
    };
    let result: CommitmentsExpertResult = run_commitments_expert_with_views(
        model,
        host.policy(),
        request.mail_invocation(context, view),
        CommitmentsContextViews {
            calendars,
            tasks: if host.source_granted(&request.agent_id, BuiltinContextSource::Tasks) {
                task_views
            } else {
                vec![]
            },
        },
    )
    .await?;
    drop(source_view);
    BuiltinExpertOutput::from_result(result.summary.clone(), &result)
}
