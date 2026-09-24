//! The Relationships Expert's own execution.
//!
//! It reads granted people context under its own consumer identity, and adds
//! confirmed interactions only when they were granted.

use floe_agent_contract::AgentFailure;
use floe_context_contract::SourceReadOutcome;

use crate::relationships::{RelationshipsContextViews, run_relationships_expert_with_views};
use crate::shared::ExpertJudgment;
use crate::{
    BlockedExpertStatus, BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest,
    granted_context,
};

/// This Expert reads people context as itself, not as the assistant.
pub const CONSUMER: &str = "contacts.expert";

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    let people = match host.people_view(request).await? {
        SourceReadOutcome::Ready(view) => view,
        SourceReadOutcome::Unavailable(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Relationships.result_artifact_name(),
                BlockedExpertStatus::Unavailable,
                "Contacts are temporarily unavailable, so there is no relationship assessment."
                    .into(),
            );
        }
        SourceReadOutcome::NeedsUserAction(blockers) => {
            blockers
                .validate()
                .map_err(|_| AgentFailure::StaleContext)?;
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Relationships.result_artifact_name(),
                BlockedExpertStatus::NeedsUserAction,
                "Contacts access needs your review, so there is no relationship assessment.".into(),
            );
        }
    };
    let confirmed_interactions = host.confirmed_interaction_views(request, &people).await?;
    let result = match run_relationships_expert_with_views(
        host.model(),
        host.policy(),
        request.personal_invocation(granted_context(host, request)),
        RelationshipsContextViews {
            people,
            confirmed_interactions,
        },
    )
    .await?
    {
        ExpertJudgment::Decided(result) => result,
        ExpertJudgment::Blocked(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Relationships.result_artifact_name(),
                BlockedExpertStatus::NeedsUserAction,
                "Model approval needs your review, so there is no relationship assessment.".into(),
            );
        }
    };
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::Relationships.result_artifact_name(),
        result.summary.clone(),
        &result,
    )
}
