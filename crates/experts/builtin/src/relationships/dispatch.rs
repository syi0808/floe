//! The Relationships Expert's own execution.
//!
//! It reads granted people context under its own consumer identity, and adds
//! confirmed interactions only when they were granted.

use crate::RequirementReadOutcome;
use floe_agent_contract::AgentFailure;

use crate::relationships::{RelationshipsContextViews, run_relationships_expert_with_views};
use crate::shared::ExpertJudgment;
use crate::{
    BlockedExpertStatus, BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest,
    granted_context,
};

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    let people = match crate::shared::read_declared_view(
        host,
        request,
        "floe.source.contacts",
        serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION}),
    )
    .await?
    {
        RequirementReadOutcome::Ready(view) => view,
        RequirementReadOutcome::Unavailable(_) => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Relationships.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::Unavailable,
                "Contacts are temporarily unavailable, so there is no relationship assessment."
                    .into(),
            );
        }
        RequirementReadOutcome::NeedsUserAction => {
            return BuiltinExpertOutput::from_blocked(
                crate::BuiltinExpertKind::Relationships.result_artifact_name(),
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Contacts access needs your review, so there is no relationship assessment.".into(),
            );
        }
    };
    let confirmed_interactions = match crate::shared::read_declared_view(
        host,
        request,
        "floe.source.confirmed-interactions",
        serde_json::to_value(&people).map_err(|_| AgentFailure::InvalidInput)?,
    )
    .await?
    {
        RequirementReadOutcome::Ready(views) => views,
        RequirementReadOutcome::Unavailable(_) | RequirementReadOutcome::NeedsUserAction => vec![],
    };
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
                super::RESULT_MEDIA_TYPE,
                BlockedExpertStatus::NeedsUserAction,
                "Model approval needs your review, so there is no relationship assessment.".into(),
            );
        }
    };
    BuiltinExpertOutput::from_result(
        crate::BuiltinExpertKind::Relationships.result_artifact_name(),
        super::RESULT_MEDIA_TYPE,
        result.summary.clone(),
        &result,
    )
}
