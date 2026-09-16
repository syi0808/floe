//! The Relationships Expert's own execution.
//!
//! It reads granted people context under its own consumer identity, and adds
//! confirmed interactions only when they were granted.

use floe_agent_contract::AgentFailure;

use crate::relationships::{
    RelationshipsContextViews, RelationshipsExpertResult, run_relationships_expert_with_views,
};
use crate::{
    BuiltinContextSource, BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest,
    granted_context,
};

/// This Expert reads people context as itself, not as the assistant.
pub const CONSUMER: &str = "contacts.expert";

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    crate::require_mandatory_source(host, request)?;
    let people = host.people_view(request).await?;
    let confirmed_interactions = if host.source_granted(
        &request.agent_id,
        BuiltinContextSource::ConfirmedInteractions,
    ) {
        host.confirmed_interaction_views(request, &people).await?
    } else {
        vec![]
    };
    let result: RelationshipsExpertResult = run_relationships_expert_with_views(
        host.model(),
        host.policy(),
        request.personal_invocation(granted_context(host, request)),
        RelationshipsContextViews {
            people,
            confirmed_interactions,
        },
    )
    .await?;
    BuiltinExpertOutput::from_result(result.summary.clone(), &result)
}
