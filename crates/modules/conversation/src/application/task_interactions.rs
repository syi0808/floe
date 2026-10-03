use floe_agent_contract::{
    AgentFailure, Artifact, ArtifactPart, DependencyCoverage, USER_INTERACTION_MEDIA_TYPE,
    UserInteractionKind, UserInteractionRef, UserInteractionStatus,
};
use floe_kernel::RunId;
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
pub async fn publish_expert_binding_blockers<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    principal: &str,
    session_id: Uuid,
    origin_run_id: RunId,
    task_id: Uuid,
    admission: &floe_experts::ExpertAdmissionIdentity,
    selection: &floe_experts::ExpertExecutionSelection,
    now_unix_ms: i64,
) -> Result<Vec<UserInteractionRef>, AgentFailure>
where
    Runs: crate::ConversationRepository + ?Sized,
    Interactions: crate::InteractionRepository + ?Sized,
{
    selection.validate()?;
    if selection
        .requirements
        .iter()
        .filter(|requirement| requirement.selected.len() < usize::from(requirement.minimum_sources))
        .count()
        > crate::MAX_ACTIVE_INTERACTIONS_PER_RUN
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    let mut refs = Vec::new();
    for requirement in &selection.requirements {
        if requirement.selected.len() >= usize::from(requirement.minimum_sources) {
            continue;
        }
        let admission = crate::publish_interaction(
            runs,
            interactions,
            crate::PublishInteractionRequest {
                principal: principal.to_owned(),
                session_id,
                origin_run_id,
                origin: crate::InteractionOrigin::Task {
                    task_id,
                    capability_call_id: None,
                },
                kind: UserInteractionKind::ExpertBinding,
                requirement: crate::InteractionRequirement {
                    kind: crate::InteractionRequirementKind::ConfigureExpertBinding,
                    source_id: "floe.expert.binding".into(),
                    connection_id: None,
                    consumer: admission.package.id.clone(),
                    purpose: "configuration".into(),
                    inline: false,
                },
                target: crate::ReviewedTarget::ExpertBinding(crate::ExpertBindingTarget {
                    registry_instance_id: admission.registry_instance_id,
                    assignment_id: admission.assignment_id,
                    package: admission.package.clone(),
                    definition_revision: admission.definition_revision,
                    requirement_key: requirement.key.clone(),
                    capability: requirement.capability.clone(),
                    contract_version: requirement.contract_version,
                    minimum_sources: requirement.minimum_sources,
                    maximum_sources: requirement.maximum_sources,
                    expected_binding_revision: selection.binding_revision,
                    admitted_selection_digest: selection.digest,
                }),
            },
            now_unix_ms,
        )
        .await?;
        let record = match admission {
            crate::PublishAdmission::Created(record)
            | crate::PublishAdmission::Existing(record) => record,
        };
        refs.push(UserInteractionRef {
            interaction_id: record.id,
            kind: UserInteractionKind::ExpertBinding,
            status: interaction_status(&record.state),
        });
    }
    Ok(refs)
}

pub fn interaction_ref_artifacts(
    refs: &[UserInteractionRef],
) -> Result<Vec<Artifact>, AgentFailure> {
    if refs.is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    let mut artifacts = Vec::with_capacity(refs.len());
    for reference in refs {
        reference.validate()?;
        artifacts.push(Artifact {
            artifact_id: Uuid::new_v4(),
            name: "user_interaction".into(),
            parts: vec![ArtifactPart::Data {
                media_type: USER_INTERACTION_MEDIA_TYPE.into(),
                data: serde_json::to_string(reference)
                    .map_err(|_| AgentFailure::InvalidModelOutput)?,
            }],
            coverage: DependencyCoverage::Independent,
        });
    }
    Ok(artifacts)
}

pub(super) fn interaction_status(state: &crate::InteractionState) -> UserInteractionStatus {
    match state {
        crate::InteractionState::Pending => UserInteractionStatus::Pending,
        crate::InteractionState::Resolving { .. } => UserInteractionStatus::Resolving,
        crate::InteractionState::Resolved { .. } => UserInteractionStatus::Resolved,
        crate::InteractionState::Denied { .. } => UserInteractionStatus::Denied,
        crate::InteractionState::Cancelled { .. } => UserInteractionStatus::Cancelled,
        crate::InteractionState::Superseded { .. } => UserInteractionStatus::Superseded,
        crate::InteractionState::Expired => UserInteractionStatus::Expired,
    }
}
