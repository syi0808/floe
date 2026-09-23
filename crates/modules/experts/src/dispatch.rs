//! Carrying one invocation from an agent id to the Expert registered for it.
//!
//! Nothing here knows what any Expert means. The table is filled by the
//! composition root from statically registered endpoints, and the request and
//! report shapes belong to the caller's own boundary.

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::{
    AgentFailure, Artifact, ArtifactPart, BoxFuture, DependencyCoverage, EndpointInvocation,
    EndpointSettlement, ExpertReport, USER_INTERACTION_MEDIA_TYPE, UserInteractionRef,
};
use uuid::Uuid;

use crate::{
    A2AArtifact, A2AMessageRole, A2APart, A2ASendMessageRequest, A2ATask, A2ATaskState, AgentCard,
    EXPERT_RESULT_MEDIA_TYPE,
};

/// How many Experts one host may register.
const MAX_REGISTERED_EXPERTS: usize = 64;

/// The judgment registered behind one agent id.
pub type ExpertRun<Host, Request, Output> =
    for<'a> fn(&'a Host, &'a Request) -> BoxFuture<'a, Result<Output, AgentFailure>>;

struct ExpertEntry<Host, Request, Output> {
    agent_id: String,
    run: ExpertRun<Host, Request, Output>,
}

/// The registered endpoints of this host, keyed by agent id.
pub struct ExpertDispatchTable<Host, Request, Output> {
    entries: Vec<ExpertEntry<Host, Request, Output>>,
}

impl<Host, Request, Output> Default for ExpertDispatchTable<Host, Request, Output> {
    fn default() -> Self {
        Self { entries: vec![] }
    }
}

impl<Host, Request, Output> ExpertDispatchTable<Host, Request, Output> {
    /// Register one endpoint. An agent id answers to exactly one Expert.
    pub fn register(
        &mut self,
        agent_id: impl Into<String>,
        run: ExpertRun<Host, Request, Output>,
    ) -> Result<(), AgentFailure> {
        let agent_id = agent_id.into();
        if agent_id.trim() != agent_id || agent_id.is_empty() || agent_id.len() > 128 {
            return Err(AgentFailure::InvalidInput);
        }
        if self.entries.len() >= MAX_REGISTERED_EXPERTS {
            return Err(AgentFailure::BudgetExceeded);
        }
        if self.entries.iter().any(|entry| entry.agent_id == agent_id) {
            return Err(AgentFailure::Conflict);
        }
        self.entries.push(ExpertEntry { agent_id, run });
        Ok(())
    }

    pub fn is_registered(&self, agent_id: &str) -> bool {
        self.entries.iter().any(|entry| entry.agent_id == agent_id)
    }

    /// Hand the invocation to the Expert registered for this agent id.
    ///
    /// An unregistered id is denied here rather than interpreted.
    pub async fn run(
        &self,
        agent_id: &str,
        host: &Host,
        request: &Request,
    ) -> Result<Output, AgentFailure> {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.agent_id == agent_id)
            .ok_or(AgentFailure::CapabilityDenied)?;
        (entry.run)(host, request).await
    }
}

/// Admit one A2A message before any Expert sees it.
///
/// Eligibility is decided from the cards this host published, never from the
/// meaning of the request, and the Task identity must already be present.
pub fn admit_expert_message(
    request: &A2ASendMessageRequest,
    cards: &[AgentCard],
) -> Result<Uuid, AgentFailure> {
    if request.schema_version != AGENT_VERSION
        || request.message.role != A2AMessageRole::User
        || !cards.iter().any(|card| card.id == request.agent_id)
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    request
        .message
        .task_id
        .ok_or(AgentFailure::CapabilityDenied)
}

/// Assemble the completed Task that carries one Expert's result.
pub fn completed_expert_task(
    request: A2ASendMessageRequest,
    artifact_name: &str,
    summary: String,
    data: String,
    auxiliary_artifacts: Vec<Artifact>,
    settlement: Option<EndpointSettlement>,
) -> Result<A2ATask, AgentFailure> {
    let auxiliary_artifacts = auxiliary_artifacts
        .iter()
        .map(contract_artifact_to_a2a)
        .collect::<Result<Vec<_>, _>>()?;
    let mut artifacts = vec![A2AArtifact {
        artifact_id: Uuid::new_v4(),
        name: artifact_name.into(),
        parts: vec![
            A2APart::Text { text: summary },
            A2APart::Data {
                media_type: EXPERT_RESULT_MEDIA_TYPE.into(),
                data,
            },
        ],
    }];
    artifacts.extend(auxiliary_artifacts);
    if let Some(settlement) = &settlement {
        settlement.validate()?;
    }
    Ok(A2ATask {
        id: request.message.task_id.ok_or(AgentFailure::InvalidInput)?,
        context_id: request.message.context_id,
        agent_id: request.agent_id,
        state: A2ATaskState::Completed,
        history: vec![request.message],
        artifacts,
        failure: None,
        settlement,
    })
}

fn contract_artifact_to_a2a(artifact: &Artifact) -> Result<A2AArtifact, AgentFailure> {
    artifact.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
    if artifact.coverage != DependencyCoverage::Independent {
        return Err(AgentFailure::InvalidModelOutput);
    }
    if artifact.parts.iter().any(|part| {
        matches!(part,
            ArtifactPart::Data { media_type, .. } if media_type == EXPERT_RESULT_MEDIA_TYPE
        )
    }) {
        return Err(AgentFailure::InvalidModelOutput);
    }
    let parts = artifact
        .parts
        .iter()
        .map(|part| match part {
            ArtifactPart::Text { text } => A2APart::Text { text: text.clone() },
            ArtifactPart::Data { media_type, data } => A2APart::Data {
                media_type: media_type.clone(),
                data: data.clone(),
            },
        })
        .collect();
    Ok(A2AArtifact {
        artifact_id: artifact.artifact_id,
        name: artifact.name.clone(),
        parts,
    })
}

fn auxiliary_artifact_from_a2a(artifact: &A2AArtifact) -> Result<Artifact, AgentFailure> {
    let parts = artifact
        .parts
        .iter()
        .map(|part| match part {
            A2APart::Text { text } => ArtifactPart::Text { text: text.clone() },
            A2APart::Data { media_type, data } => ArtifactPart::Data {
                media_type: media_type.clone(),
                data: data.clone(),
            },
        })
        .collect::<Vec<_>>();
    let artifact = Artifact {
        artifact_id: artifact.artifact_id,
        name: artifact.name.clone(),
        parts,
        coverage: DependencyCoverage::Independent,
    };
    artifact.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
    for part in &artifact.parts {
        if let ArtifactPart::Data { media_type, data } = part {
            if media_type == EXPERT_RESULT_MEDIA_TYPE {
                return Err(AgentFailure::InvalidModelOutput);
            }
            if media_type == USER_INTERACTION_MEDIA_TYPE {
                let reference: UserInteractionRef =
                    serde_json::from_str(data).map_err(|_| AgentFailure::InvalidModelOutput)?;
                reference.validate()?;
            }
        }
    }
    Ok(artifact)
}

/// Turn a settled Task into the report the delegating Run receives.
///
/// The Task must be the one that was requested, for the agent that was
/// selected, and must actually have completed; anything else is invalid model
/// output rather than an empty success.
pub fn expert_report(
    invocation: EndpointInvocation,
    task: &A2ATask,
    parent_context_id: Uuid,
    coverage: DependencyCoverage,
) -> Result<ExpertReport, AgentFailure> {
    if task.id != invocation.request.task_id.as_uuid()
        || task.context_id != parent_context_id
        || task.agent_id != invocation.request.selected_agent_id
        || task.state != A2ATaskState::Completed
        || task.failure.is_some()
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    let mut artifact_ids = std::collections::HashSet::new();
    if task
        .artifacts
        .iter()
        .any(|artifact| !artifact_ids.insert(artifact.artifact_id))
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    let primary = task
        .artifacts
        .iter()
        .filter(|artifact| {
            artifact.parts.iter().any(|part| {
                matches!(part,
                    A2APart::Data { media_type, .. } if media_type == EXPERT_RESULT_MEDIA_TYPE
                )
            })
        })
        .collect::<Vec<_>>();
    let [primary] = primary.as_slice() else {
        return Err(AgentFailure::InvalidModelOutput);
    };
    let result = primary
        .parts
        .iter()
        .filter_map(|part| match part {
            A2APart::Data { media_type, data } if media_type == EXPERT_RESULT_MEDIA_TYPE => {
                Some(data)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [result] = result.as_slice() else {
        return Err(AgentFailure::InvalidModelOutput);
    };
    let artifacts = task
        .artifacts
        .iter()
        .filter(|artifact| artifact.artifact_id != primary.artifact_id)
        .map(auxiliary_artifact_from_a2a)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ExpertReport {
        task_id: invocation.request.task_id,
        principal: invocation.request.principal,
        agent_id: invocation.request.selected_agent_id,
        definition_revision: invocation.request.selected_definition_revision,
        result: (*result).clone(),
        artifacts,
        coverage,
        settlement: task.settlement.clone(),
    })
}

/// Who records how a delegated result depends on the sources behind it.
pub trait TaskCoverageRecorder: Send + Sync {
    fn record_independent(&self, turn_id: Uuid, result_id: Uuid) -> Result<(), AgentFailure>;

    fn record(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
        dependency: floe_agent_contract::ContextDependency,
    ) -> Result<(), AgentFailure>;
}

/// Record the coverage a delegated Task reported, so the caller's own result
/// carries the same provenance.
pub fn record_task_coverage(
    recorder: Option<&dyn TaskCoverageRecorder>,
    turn_id: Uuid,
    result_id: Uuid,
    receipt: &floe_agent_contract::TaskReceipt,
) -> Result<(), AgentFailure> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    match &receipt.snapshot.coverage {
        DependencyCoverage::Independent => recorder.record_independent(turn_id, result_id),
        DependencyCoverage::Dependent { dependencies } => {
            for dependency in dependencies {
                recorder.record(turn_id, result_id, dependency.clone())?;
            }
            Ok(())
        }
        DependencyCoverage::Unknown => Ok(()),
    }
}

/// Project a settled Task receipt onto the A2A Task the caller polls.
///
/// Only a completed Task carries an artifact, and a completed Task without a
/// usable result is a storage failure rather than an empty success.
pub fn task_receipt_to_a2a(
    request: A2ASendMessageRequest,
    artifact_name: &str,
    receipt: floe_agent_contract::TaskReceipt,
) -> Result<A2ATask, AgentFailure> {
    use floe_agent_contract::TaskState;

    let state = match receipt.snapshot.state {
        TaskState::Submitted => A2ATaskState::Submitted,
        TaskState::Working => A2ATaskState::Working,
        TaskState::Completed => A2ATaskState::Completed,
        TaskState::Rejected => A2ATaskState::Rejected,
        TaskState::Cancelled => A2ATaskState::Cancelled,
        TaskState::Failed | TaskState::TimedOut | TaskState::Interrupted => A2ATaskState::Failed,
    };
    let artifacts = if receipt.snapshot.state == TaskState::Completed {
        let result = receipt
            .snapshot
            .result
            .as_deref()
            .ok_or(AgentFailure::StorageUnavailable)?;
        let report: crate::ExpertResult =
            serde_json::from_str(result).map_err(|_| AgentFailure::StorageUnavailable)?;
        let mut artifacts = vec![A2AArtifact {
            artifact_id: Uuid::new_v4(),
            name: artifact_name.into(),
            parts: vec![
                A2APart::Text {
                    text: report
                        .summary
                        .clone()
                        .ok_or(AgentFailure::InvalidModelOutput)?,
                },
                A2APart::Data {
                    media_type: EXPERT_RESULT_MEDIA_TYPE.into(),
                    data: result.into(),
                },
            ],
        }];
        artifacts.extend(
            receipt
                .snapshot
                .artifacts
                .iter()
                .map(contract_artifact_to_a2a)
                .collect::<Result<Vec<_>, _>>()?,
        );
        artifacts
    } else {
        vec![]
    };
    Ok(A2ATask {
        id: receipt.task_id.as_uuid(),
        context_id: request.message.context_id,
        agent_id: request.agent_id,
        state,
        history: vec![request.message],
        artifacts,
        failure: receipt.snapshot.issue,
        settlement: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_agent_contract::{
        AgentContext, DataClass, DelegationExecutionContext, DelegationRequest, InvocationKey,
        PackageKind, PackageRef, TaskId, TaskReceipt, TaskSnapshot, TaskState,
        UserInteractionKind, UserInteractionStatus,
    };
    use floe_execution::Cancellation;
    use tokio::time::Instant;

    fn request() -> A2ASendMessageRequest {
        let task_id = Uuid::new_v4();
        A2ASendMessageRequest {
            usage: Default::default(),
            schema_version: AGENT_VERSION,
            person_id: floe_agent_contract::PersonId::new(),
            session_id: Uuid::new_v4(),
            parent_turn_id: Uuid::new_v4(),
            agent_id: "floe.builtin.schedule".into(),
            message: crate::A2AMessage {
                message_id: Uuid::new_v4(),
                context_id: Uuid::new_v4(),
                task_id: Some(task_id),
                role: A2AMessageRole::User,
                parts: vec![A2APart::Text {
                    text: "Schedule?".into(),
                }],
            },
            max_output_bytes: 4096,
            deadline: Instant::now() + std::time::Duration::from_secs(10),
            cancellation: Cancellation::default(),
        }
    }

    fn interaction_artifact() -> Artifact {
        Artifact {
            artifact_id: Uuid::new_v4(),
            name: "user_interaction".into(),
            parts: vec![ArtifactPart::Data {
                media_type: USER_INTERACTION_MEDIA_TYPE.into(),
                data: serde_json::to_string(&UserInteractionRef {
                    interaction_id: Uuid::new_v4(),
                    kind: UserInteractionKind::SourceAccess,
                    status: UserInteractionStatus::Pending,
                })
                .unwrap(),
            }],
            coverage: DependencyCoverage::Independent,
        }
    }

    fn invocation(request: &A2ASendMessageRequest) -> EndpointInvocation {
        EndpointInvocation {
            request: DelegationRequest {
                task_id: TaskId::from_uuid(request.message.task_id.unwrap()).unwrap(),
                parent_run_id: Some(request.parent_turn_id),
                principal: request.person_id.to_string(),
                invocation_key: InvocationKey::new(),
                selected_agent_id: request.agent_id.clone(),
                selected_definition_revision: 1,
                message: "Schedule?".into(),
                context_refs: vec![],
                execution_context: DelegationExecutionContext {
                    session_id: request.session_id,
                    device_id: "test-device".into(),
                    agent_context: AgentContext {
                        projection_version: 1,
                        persona: None,
                        memories: vec![],
                        optional_context_issues: vec![],
                        evidence: vec![],
                    },
                    max_output_bytes: 4096,
                },
            },
            request_digest: [1; 32],
        }
    }

    #[test]
    fn completed_task_preserves_interaction_and_optional_settlement() {
        let request = request();
        let invocation = invocation(&request);
        let context_id = request.message.context_id;
        let interaction = interaction_artifact();
        let settlement = EndpointSettlement::try_new("schedule", "payload").unwrap();
        let task = completed_expert_task(
            request,
            "Schedule",
            "Blocked".into(),
            "{}".into(),
            vec![interaction.clone()],
            Some(settlement.clone()),
        )
        .unwrap();
        let report = expert_report(
            invocation,
            &task,
            context_id,
            DependencyCoverage::Independent,
        )
        .unwrap();
        assert_eq!(report.artifacts, vec![interaction]);
        assert_eq!(report.settlement, Some(settlement));
        assert!(
            serde_json::to_value(&report)
                .unwrap()
                .get("settlement")
                .is_none()
        );
    }

    #[test]
    fn completed_task_requires_one_primary_and_valid_interaction() {
        let request = request();
        let invocation = invocation(&request);
        let context_id = request.message.context_id;
        let mut task = completed_expert_task(
            request,
            "Schedule",
            "Blocked".into(),
            "{}".into(),
            vec![interaction_artifact()],
            None,
        )
        .unwrap();
        let primary = task.artifacts.remove(0);
        assert!(
            expert_report(
                invocation.clone(),
                &task,
                context_id,
                DependencyCoverage::Independent
            )
            .is_err()
        );
        task.artifacts.insert(0, primary);
        let mut duplicate = task.artifacts[0].clone();
        duplicate.artifact_id = Uuid::new_v4();
        task.artifacts.push(duplicate);
        assert!(
            expert_report(
                invocation.clone(),
                &task,
                context_id,
                DependencyCoverage::Independent
            )
            .is_err()
        );
        task.artifacts.pop();
        let mut malformed = interaction_artifact();
        malformed.parts = vec![ArtifactPart::Data {
            media_type: USER_INTERACTION_MEDIA_TYPE.into(),
            data: "{}".into(),
        }];
        task.artifacts
            .push(contract_artifact_to_a2a(&malformed).unwrap());
        assert!(
            expert_report(
                invocation,
                &task,
                context_id,
                DependencyCoverage::Independent
            )
            .is_err()
        );
    }

    #[test]
    fn completed_receipt_preserves_auxiliary_artifact() {
        let request = request();
        let artifact = interaction_artifact();
        let result = crate::ExpertResult {
            schema_version: AGENT_VERSION,
            invocation_id: request.message.task_id.unwrap(),
            instance_id: Uuid::new_v4(),
            person_id: request.person_id,
            assignment_id: Uuid::new_v4(),
            package: PackageRef {
                kind: PackageKind::Expert,
                id: request.agent_id.clone(),
                version: "1".into(),
            },
            view_handle: Uuid::new_v4(),
            source_handle: "calendar".into(),
            data_class: DataClass::Personal,
            expires_at_unix_ms: 1,
            insights: vec![],
            action_proposals: vec![],
            summary: Some("Blocked".into()),
            model_calls: 1,
            state_revision: 1,
            view_calls: 0,
        };
        let task_id = TaskId::from_uuid(request.message.task_id.unwrap()).unwrap();
        let receipt = TaskReceipt {
            task_id,
            snapshot: TaskSnapshot {
                task_id,
                parent_run_id: Some(request.parent_turn_id),
                principal: request.person_id.to_string(),
                agent_id: request.agent_id.clone(),
                definition_revision: 1,
                state: TaskState::Completed,
                result: Some(serde_json::to_string(&result).unwrap()),
                artifacts: vec![artifact.clone()],
                coverage: DependencyCoverage::Independent,
                issue: None,
            },
            replay: None,
        };
        let task = task_receipt_to_a2a(request, "Schedule", receipt).unwrap();
        assert_eq!(task.artifacts.len(), 2);
        assert_eq!(auxiliary_artifact_from_a2a(&task.artifacts[1]).unwrap(), artifact);
    }
}
