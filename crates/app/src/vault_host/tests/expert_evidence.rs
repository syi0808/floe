use floe_actions::ExpertCalendarProposal;
use floe_agent_contract::{
    Artifact, ArtifactPart, DependencyCoverage, InvocationKey, TaskId, TaskSnapshot, TaskState,
};
use floe_context_contract::ContextDependency;
use floe_conversation::AgentMessage;
use floe_experts::{
    A2AArtifact, A2AMessage, A2AMessageRole, A2APart, A2ATask, A2ATaskState, ExpertSettlement,
    ExpertTaskCompletion, RegistrySnapshot,
};
use floe_vault::{EncryptedAgentVault, VaultKeyProvider, VaultTaskRecord};
use uuid::Uuid;

pub(in crate::vault_host) fn delegation_message(
    turn_id: Uuid,
    snapshot: &TaskSnapshot,
) -> AgentMessage {
    let context_id = Uuid::new_v4();
    AgentMessage::Delegation {
        turn_id,
        task: A2ATask {
            id: snapshot.task_id.as_uuid(),
            context_id,
            agent_id: snapshot.agent_id.clone(),
            state: A2ATaskState::Completed,
            history: vec![A2AMessage {
                message_id: Uuid::new_v4(),
                context_id,
                task_id: Some(snapshot.task_id.as_uuid()),
                role: A2AMessageRole::User,
                parts: vec![A2APart::Text {
                    text: "Prepare a bounded scheduling proposal.".into(),
                }],
            }],
            artifacts: snapshot
                .artifacts
                .iter()
                .map(|artifact| A2AArtifact {
                    artifact_id: artifact.artifact_id,
                    name: artifact.name.clone(),
                    parts: artifact
                        .parts
                        .iter()
                        .map(|part| match part {
                            ArtifactPart::Text { text } => A2APart::Text { text: text.clone() },
                            ArtifactPart::Data { media_type, data } => A2APart::Data {
                                media_type: media_type.clone(),
                                data: data.clone(),
                            },
                        })
                        .collect(),
                })
                .collect(),
            result: snapshot.result.clone(),
            failure: None,
            settlement: None,
        },
    }
}

pub(in crate::vault_host) async fn record_proposal_task<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    completed_registry: RegistrySnapshot,
    evidence: &ExpertCalendarProposal,
    dependency: ContextDependency,
) -> TaskSnapshot {
    let coverage = DependencyCoverage::dependent(dependency.clone()).unwrap();
    let artifact = evidence.artifact(coverage.clone()).unwrap();
    record_proposal_task_with_artifacts(
        vault,
        completed_registry,
        evidence,
        dependency,
        vec![artifact],
    )
    .await
}

pub(in crate::vault_host) async fn record_proposal_task_with_artifacts<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    completed_registry: RegistrySnapshot,
    evidence: &ExpertCalendarProposal,
    dependency: ContextDependency,
    artifacts: Vec<Artifact>,
) -> TaskSnapshot {
    let task_id = TaskId::from_uuid(evidence.task_id).unwrap();
    let coverage = DependencyCoverage::dependent(dependency.clone()).unwrap();
    let result = "A scheduling proposal is available for review.".to_owned();
    let terminal = TaskSnapshot {
        task_id,
        parent_run_id: None,
        principal: evidence.person_id.to_string(),
        agent_id: evidence.package.id.clone(),
        definition_revision: 1,
        state: TaskState::Completed,
        result: Some(result.clone()),
        artifacts,
        coverage,
        issue: None,
    };
    let activation = vault.activate_task_executor().await.unwrap();
    let submitted = TaskSnapshot {
        state: TaskState::Submitted,
        result: None,
        artifacts: vec![],
        coverage: DependencyCoverage::Unknown,
        ..terminal.clone()
    };
    vault
        .admit_task(VaultTaskRecord {
            snapshot: submitted.clone(),
            admission: floe_experts::ExpertAdmissionIdentity {
                registry_instance_id: evidence.instance_id,
                assignment_id: evidence.assignment_id,
                installation_id: completed_registry
                    .assignments
                    .iter()
                    .find(|assignment| assignment.id == evidence.assignment_id)
                    .unwrap()
                    .installation_id,
                package: evidence.package.clone(),
                definition_revision: 1,
            },
            invocation_key: InvocationKey::from_uuid(evidence.invocation_id).unwrap(),
            request_digest: [1; 32],
            aggregate_revision: 1,
            executor_generation: activation.executor_generation,
        })
        .await
        .unwrap();
    vault
        .compare_and_swap_task(
            task_id,
            1,
            activation.executor_generation,
            TaskSnapshot {
                state: TaskState::Working,
                ..submitted
            },
        )
        .await
        .unwrap();
    let next_private_state = completed_registry
        .assignments
        .iter()
        .find(|assignment| assignment.id == evidence.assignment_id)
        .unwrap()
        .private_state
        .clone();
    let settlement = ExpertSettlement::new(
        evidence.package.id.clone(),
        floe_experts::ExpertAdmissionIdentity {
            registry_instance_id: evidence.instance_id,
            assignment_id: evidence.assignment_id,
            installation_id: completed_registry
                .assignments
                .iter()
                .find(|assignment| assignment.id == evidence.assignment_id)
                .unwrap()
                .installation_id,
            package: evidence.package.clone(),
            definition_revision: 1,
        },
        next_private_state.revision - 1,
        next_private_state,
        evidence.invocation_id,
        vec![dependency],
        result,
    );
    vault
        .settle_expert_task_checked(
            ExpertTaskCompletion {
                settlement,
                task_id,
                expected_task_revision: 2,
                executor_generation: activation.executor_generation,
                task_snapshot: terminal.clone(),
            },
            || Ok(()),
        )
        .await
        .unwrap();
    terminal
}
