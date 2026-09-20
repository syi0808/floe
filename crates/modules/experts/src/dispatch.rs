//! Carrying one invocation from an agent id to the Expert registered for it.
//!
//! Nothing here knows what any Expert means. The table is filled by the
//! composition root from statically registered endpoints, and the request and
//! report shapes belong to the caller's own boundary.

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::{
    AgentFailure, BoxFuture, DependencyCoverage, EndpointInvocation, ExpertReport,
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
) -> Result<A2ATask, AgentFailure> {
    Ok(A2ATask {
        id: request.message.task_id.ok_or(AgentFailure::InvalidInput)?,
        context_id: request.message.context_id,
        agent_id: request.agent_id,
        state: A2ATaskState::Completed,
        history: vec![request.message],
        artifacts: vec![A2AArtifact {
            artifact_id: Uuid::new_v4(),
            name: artifact_name.into(),
            parts: vec![
                A2APart::Text { text: summary },
                A2APart::Data {
                    media_type: EXPERT_RESULT_MEDIA_TYPE.into(),
                    data,
                },
            ],
        }],
        failure: None,
    })
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
    let result = task
        .data_part(EXPERT_RESULT_MEDIA_TYPE)
        .or_else(|| task.result_text().ok())
        .map(str::to_owned)
        .ok_or(AgentFailure::InvalidModelOutput)?;
    Ok(ExpertReport {
        task_id: invocation.request.task_id,
        principal: invocation.request.principal,
        agent_id: invocation.request.selected_agent_id,
        definition_revision: invocation.request.selected_definition_revision,
        result,
        artifacts: vec![],
        coverage,
        settlement: None,
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

/// Delegate one admitted A2A message to the Task path as a child run.
///
/// The child gets its own bounded scope under the caller's cancellation and
/// deadline; the Expert behind the agent id never chooses its own budget.
pub async fn delegate_expert_task<Repository: crate::TaskRepository>(
    coordinator: &crate::TaskCoordinator<Repository>,
    request: &A2ASendMessageRequest,
    selected_definition_revision: u64,
    execution_context: floe_agent_contract::DelegationExecutionContext,
) -> Result<floe_agent_contract::TaskReceipt, AgentFailure> {
    use floe_agent_contract::{DelegationPort, DelegationRequest, InvocationKey, TaskId};
    use floe_agent_contract::{RunId, TraceContext};
    use floe_execution::{
        ExecutionScope,
        budget::{BudgetConfig, BudgetLedger},
    };

    let task_uuid = request.message.task_id.ok_or(AgentFailure::InvalidInput)?;
    let task_id = TaskId::from_uuid(task_uuid).ok_or(AgentFailure::InvalidInput)?;
    let run_id = RunId::from_uuid(request.parent_turn_id).ok_or(AgentFailure::InvalidInput)?;
    let ledger = BudgetLedger::new(BudgetConfig::new(50_000, 100_000), Default::default());
    let root_scope = ExecutionScope::root(
        request.cancellation.clone(),
        request.deadline,
        ledger.work_lease(),
        TraceContext::new(request.message.message_id).with_run_id(run_id),
    );
    let scope = root_scope.child_scope(request.deadline, 40_960, 50_000, Some(task_id));
    coordinator
        .delegate(
            DelegationRequest {
                task_id,
                parent_run_id: Some(request.parent_turn_id),
                principal: request.person_id.to_string(),
                invocation_key: InvocationKey::from_uuid(task_uuid)
                    .ok_or(AgentFailure::InvalidInput)?,
                selected_agent_id: request.agent_id.clone(),
                selected_definition_revision,
                message: request.message.text()?.to_owned(),
                context_refs: vec![],
                execution_context,
            },
            &scope,
        )
        .await
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
        vec![A2AArtifact {
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
        }]
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
    })
}
