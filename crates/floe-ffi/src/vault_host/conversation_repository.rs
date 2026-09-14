use std::sync::Arc;

use floe_agent::{A2AArtifact, A2APart, A2ATask, A2ATaskState, AgentMessage};
use floe_agent_contract::{
    AgentFailure, AgentMessage as ContractMessage, Artifact as ContractArtifact,
    ArtifactPart as ContractArtifactPart, BoxFuture, DependencyCoverage, EngineStep,
    ExecutionJournal, JournalAck, JournalEvent, MessageRole, RunId, TaskReceipt, TaskState,
};
use floe_conversation::{
    AdmittedTurn, ConversationRepository, JournalEntry, RecoveryReceipt, RecoveryRequest,
    RunReceipt, RunState, RunTerminal, TurnAdmission, TurnAdmissionRequest,
};
use floe_core::{
    EncryptedAgentVault, VaultConversationAdmission, VaultConversationAdmissionRequest,
    VaultConversationRunRecord, VaultConversationRunState, VaultConversationTerminal,
    VaultKeyProvider,
};
use uuid::Uuid;

pub(super) struct VaultConversationRepository<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
}

impl<Keys> VaultConversationRepository<Keys> {
    pub(super) fn new(vault: Arc<EncryptedAgentVault<Keys>>) -> Self {
        Self { vault }
    }
}

impl<Keys: VaultKeyProvider + 'static> ConversationRepository
    for VaultConversationRepository<Keys>
{
    fn admit_turn<'a>(
        &'a self,
        request: TurnAdmissionRequest,
    ) -> BoxFuture<'a, Result<TurnAdmission, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            if request.principal != self.vault.person_id().to_string() {
                return Err(AgentFailure::CapabilityDenied);
            }
            let text = match request.user_message {
                ContractMessage {
                    role: MessageRole::User,
                    text,
                    ..
                } => text,
                _ => return Err(AgentFailure::InvalidInput),
            };
            match self
                .vault
                .admit_conversation_turn(VaultConversationAdmissionRequest {
                    run_id: request.run_id,
                    command_id: request.command_id,
                    session_id: request.session_id,
                    person_id: self.vault.person_id(),
                    expected_session_revision: request.expected_session_revision,
                    request_digest: request.request_digest,
                    text,
                })
                .await?
            {
                VaultConversationAdmission::Created { record, session } => {
                    let receipt = run_receipt(record)?;
                    let transcript = transcript(&session.messages, &receipt)?;
                    Ok(TurnAdmission::Created(AdmittedTurn {
                        receipt,
                        transcript,
                    }))
                }
                VaultConversationAdmission::Existing(record) => {
                    Ok(TurnAdmission::Existing(run_receipt(record)?))
                }
            }
        })
    }

    fn journal(&self, run_id: RunId) -> Result<Arc<dyn ExecutionJournal>, AgentFailure> {
        if !run_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Arc::new(VaultConversationJournal {
            vault: Arc::clone(&self.vault),
            run_id,
        }))
    }

    fn finish_run<'a>(
        &'a self,
        run_id: RunId,
        expected_aggregate_revision: u64,
        terminal: RunTerminal,
    ) -> BoxFuture<'a, Result<RunReceipt, AgentFailure>> {
        Box::pin(async move {
            terminal.validate()?;
            let state = vault_state(terminal.state);
            let appended_messages = terminal_messages(run_id, &terminal)?;
            self.vault
                .finish_conversation_run(
                    run_id,
                    expected_aggregate_revision,
                    VaultConversationTerminal {
                        state,
                        output: terminal.output,
                        coverage: terminal.coverage,
                        issue: terminal.issue,
                        appended_messages,
                    },
                )
                .await
                .and_then(run_receipt)
        })
    }

    fn load_run<'a>(
        &'a self,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<Option<AdmittedTurn>, AgentFailure>> {
        Box::pin(async move {
            let Some(record) = self.vault.conversation_run(run_id).await? else {
                return Ok(None);
            };
            let receipt = run_receipt(record)?;
            let session = floe_agent::SessionStore::load(
                self.vault.as_ref(),
                self.vault.person_id(),
                receipt.session_id,
            )
            .await?;
            if session.revision != receipt.session_revision {
                return Err(AgentFailure::Conflict);
            }
            let transcript = transcript(&session.messages, &receipt)?;
            Ok(Some(AdmittedTurn {
                receipt,
                transcript,
            }))
        })
    }

    fn recover_session<'a>(
        &'a self,
        request: RecoveryRequest,
    ) -> BoxFuture<'a, Result<RecoveryReceipt, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            if request.principal != self.vault.person_id().to_string() {
                return Err(AgentFailure::CapabilityDenied);
            }
            let session_revision = self
                .vault
                .recover_conversation_session(
                    request.session_id,
                    self.vault.person_id(),
                    request.expected_session_revision,
                )
                .await?;
            Ok(RecoveryReceipt {
                session_id: request.session_id,
                session_revision,
            })
        })
    }

    fn load_journal<'a>(
        &'a self,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<Vec<JournalEntry>, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .conversation_journal(run_id)
                .await?
                .into_iter()
                .map(|entry| {
                    let event = serde_json::from_str::<JournalEvent>(&entry.payload)
                        .map_err(|_| AgentFailure::StorageUnavailable)?;
                    let expected_kind = match &event {
                        JournalEvent::ModelIntent { .. }
                        | JournalEvent::ToolIntent { .. }
                        | JournalEvent::DelegationIntent { .. } => "intent",
                        JournalEvent::ModelResult { .. }
                        | JournalEvent::ToolResult { .. }
                        | JournalEvent::DelegationResult { .. } => "result",
                        JournalEvent::Output { .. } => "output",
                        JournalEvent::Checkpoint { .. } => "checkpoint",
                    };
                    if entry.kind != expected_kind {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    Ok(JournalEntry {
                        revision: entry.revision,
                        event,
                    })
                })
                .collect()
        })
    }
}

struct VaultConversationJournal<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    run_id: RunId,
}

impl<Keys: VaultKeyProvider + 'static> VaultConversationJournal<Keys> {
    fn record<'a>(
        &'a self,
        phase: &'static str,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async move {
            let payload =
                serde_json::to_string(&event).map_err(|_| AgentFailure::StorageUnavailable)?;
            let revision = self
                .vault
                .append_conversation_journal(self.run_id, phase, &payload)
                .await?;
            Ok(JournalAck::Accepted { revision })
        })
    }
}

impl<Keys: VaultKeyProvider + 'static> ExecutionJournal for VaultConversationJournal<Keys> {
    fn record_intent<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        self.record("intent", event)
    }

    fn record_result<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        self.record("result", event)
    }

    fn record_output<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        self.record("output", event)
    }

    fn checkpoint<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        self.record("checkpoint", event)
    }
}

fn run_receipt(record: VaultConversationRunRecord) -> Result<RunReceipt, AgentFailure> {
    let receipt = RunReceipt {
        run_id: record.run_id,
        command_id: record.command_id,
        session_id: record.session_id,
        principal: record.person_id.to_string(),
        request_digest: record.request_digest,
        state: match record.state {
            VaultConversationRunState::Working => RunState::Working,
            VaultConversationRunState::Completed => RunState::Completed,
            VaultConversationRunState::Failed => RunState::Failed,
            VaultConversationRunState::Cancelled => RunState::Cancelled,
            VaultConversationRunState::TimedOut => RunState::TimedOut,
            VaultConversationRunState::Interrupted => RunState::Interrupted,
        },
        output: record.output,
        coverage: record.coverage,
        issue: record.issue,
        session_revision: record.session_revision,
        aggregate_revision: record.aggregate_revision,
        executor_generation: record.executor_generation,
    };
    receipt.validate()?;
    Ok(receipt)
}

fn vault_state(state: RunState) -> VaultConversationRunState {
    match state {
        RunState::Working => VaultConversationRunState::Working,
        RunState::Completed => VaultConversationRunState::Completed,
        RunState::Failed => VaultConversationRunState::Failed,
        RunState::Cancelled => VaultConversationRunState::Cancelled,
        RunState::TimedOut => VaultConversationRunState::TimedOut,
        RunState::Interrupted => VaultConversationRunState::Interrupted,
    }
}

fn transcript(
    messages: &[AgentMessage],
    receipt: &RunReceipt,
) -> Result<Vec<ContractMessage>, AgentFailure> {
    messages
        .iter()
        .map(|message| {
            let turn_id = message.turn_id();
            let message_id = match message {
                AgentMessage::User { .. } if turn_id == receipt.run_id.as_uuid() => {
                    receipt.command_id.as_uuid()
                }
                _ => turn_id,
            };
            let (role, text, call_id) = match message {
                AgentMessage::Compaction { summary, .. } => {
                    (MessageRole::Assistant, summary.clone(), None)
                }
                AgentMessage::Preamble { text, .. } => (MessageRole::Preamble, text.clone(), None),
                AgentMessage::User { text, .. } => (MessageRole::User, text.clone(), None),
                AgentMessage::Assistant { text, .. } => {
                    (MessageRole::Assistant, text.clone(), None)
                }
                AgentMessage::Capability {
                    call_id, result, ..
                } => (
                    MessageRole::Tool,
                    result
                        .as_ref()
                        .cloned()
                        .unwrap_or_else(|failure| format!("unavailable: {failure:?}")),
                    Some(*call_id),
                ),
                AgentMessage::Delegation { task, .. } => (
                    MessageRole::Delegation,
                    task.artifacts
                        .iter()
                        .flat_map(|artifact| &artifact.parts)
                        .find_map(|part| match part {
                            A2APart::Text { text } => Some(text.clone()),
                            A2APart::Data { .. } => None,
                        })
                        .unwrap_or_else(|| format!("{}: {:?}", task.agent_id, task.state)),
                    None,
                ),
            };
            let message = ContractMessage {
                message_id,
                role,
                text,
                call_id,
                coverage: if matches!(message, AgentMessage::User { .. }) {
                    DependencyCoverage::Independent
                } else {
                    DependencyCoverage::Unknown
                },
            };
            message.validate()?;
            Ok(message)
        })
        .collect()
}

fn terminal_messages(
    run_id: RunId,
    terminal: &RunTerminal,
) -> Result<Vec<AgentMessage>, AgentFailure> {
    let mut messages = Vec::new();
    for step in &terminal.steps {
        match step {
            EngineStep::Answer { text, .. } => messages.push(AgentMessage::Assistant {
                turn_id: run_id.as_uuid(),
                text: text.clone(),
            }),
            EngineStep::Delegation(receipt) => messages.push(AgentMessage::Delegation {
                turn_id: run_id.as_uuid(),
                task: legacy_task(run_id, receipt)?,
            }),
            EngineStep::Tool(_) => {}
        }
    }
    Ok(messages)
}

fn legacy_task(run_id: RunId, receipt: &TaskReceipt) -> Result<A2ATask, AgentFailure> {
    receipt
        .snapshot
        .validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
    let mut artifacts = receipt
        .snapshot
        .artifacts
        .iter()
        .map(legacy_artifact)
        .collect::<Result<Vec<_>, _>>()?;
    if receipt.snapshot.state == TaskState::Completed {
        let result = receipt
            .snapshot
            .result
            .as_ref()
            .ok_or(AgentFailure::StorageUnavailable)?;
        let summary = serde_json::from_str::<floe_agent::ExpertResult>(result)
            .ok()
            .and_then(|result| result.summary)
            .unwrap_or_else(|| "Expert task completed".into());
        artifacts.push(A2AArtifact {
            artifact_id: Uuid::new_v5(&receipt.task_id.as_uuid(), b"expert-result"),
            name: "Expert result".into(),
            parts: vec![
                A2APart::Text { text: summary },
                A2APart::Data {
                    media_type: floe_agent::EXPERT_RESULT_MEDIA_TYPE.into(),
                    data: result.clone(),
                },
            ],
        });
    }
    Ok(A2ATask {
        id: receipt.task_id.as_uuid(),
        context_id: run_id.as_uuid(),
        agent_id: receipt.snapshot.agent_id.clone(),
        state: match receipt.snapshot.state {
            TaskState::Submitted => A2ATaskState::Submitted,
            TaskState::Working => A2ATaskState::Working,
            TaskState::Completed => A2ATaskState::Completed,
            TaskState::Rejected => A2ATaskState::Rejected,
            TaskState::Cancelled => A2ATaskState::Cancelled,
            TaskState::Failed | TaskState::TimedOut | TaskState::Interrupted => {
                A2ATaskState::Failed
            }
        },
        history: vec![],
        artifacts,
        failure: receipt.snapshot.issue,
    })
}

fn legacy_artifact(artifact: &ContractArtifact) -> Result<A2AArtifact, AgentFailure> {
    artifact.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
    Ok(A2AArtifact {
        artifact_id: artifact.artifact_id,
        name: artifact.name.clone(),
        parts: artifact
            .parts
            .iter()
            .map(|part| match part {
                ContractArtifactPart::Text { text } => A2APart::Text { text: text.clone() },
                ContractArtifactPart::Data { media_type, data } => A2APart::Data {
                    media_type: media_type.clone(),
                    data: data.clone(),
                },
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests;
