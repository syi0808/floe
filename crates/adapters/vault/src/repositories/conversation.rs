use std::sync::Arc;

use crate::{
    EncryptedAgentVault, VaultConversationAdmission, VaultConversationCancelAdmission,
    VaultConversationCancelRequest, VaultKeyProvider,
};
use floe_agent_contract::{
    AgentFailure, AgentMessage as ContractMessage, ArchivePointer, ArchiveReadRequest,
    ArchiveSnapshot, ArchivedMessage, BoxFuture, ExecutionJournal, JournalAck, JournalEvent,
    MessageRole, RunId,
};
use floe_conversation::{
    AdmittedTurn, CancelRunAdmission, CancelRunCommand, CancelRunReceipt, CompactionReceipt,
    CompactionRequest, ConversationInteraction, ConversationRepository, DecisionAdmission,
    ExpireInteraction, ExpireOutcome, InteractionDecision, InteractionRepository, JournalEntry,
    RecoveryReceipt, RecoveryRequest, RunReceipt, RunTerminal,
    SessionArchiveRepository, SessionReadRequest, SessionReceipt, SessionRepository,
    SessionRequest, SupersedeInteraction, TurnAdmission, TurnAdmissionRequest,
};
use uuid::Uuid;

pub struct VaultConversationRepository<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
}

impl<Keys: VaultKeyProvider + 'static> SessionRepository for VaultConversationRepository<Keys> {
    fn start_session<'a>(
        &'a self,
        request: floe_conversation::StartSessionRequest,
    ) -> BoxFuture<'a, Result<SessionReceipt, AgentFailure>> {
        Box::pin(async move { self.vault.start_conversation_session(request).await })
    }

    fn resume_session<'a>(
        &'a self,
        request: SessionRequest,
    ) -> BoxFuture<'a, Result<SessionReceipt, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            self.verify_principal(&request.principal)?;
            floe_conversation::project_session_receipt(
                self.vault.resume_conversation_session().await?,
            )
        })
    }

    fn get_session<'a>(
        &'a self,
        request: SessionReadRequest,
    ) -> BoxFuture<'a, Result<SessionReceipt, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            self.verify_principal(&request.principal)?;
            floe_conversation::project_session_receipt(
                floe_conversation::SessionStore::load(
                    self.vault.as_ref(),
                    self.vault.person_id(),
                    request.session_id,
                )
                .await?,
            )
        })
    }
}

impl<Keys: VaultKeyProvider + 'static> SessionArchiveRepository
    for VaultConversationRepository<Keys>
{
    fn compact_session<'a>(
        &'a self,
        request: CompactionRequest,
    ) -> BoxFuture<'a, Result<CompactionReceipt, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            if request.principal != self.vault.person_id().to_string() {
                return Err(AgentFailure::CapabilityDenied);
            }
            let result = self
                .vault
                .compact_session(
                    request.session_id,
                    request.expected_session_revision,
                    request.through_turn_id,
                    request.summary.clone(),
                )
                .await?;
            let pointer = archive_pointer(&result.recovery);
            let receipt = CompactionReceipt {
                session_id: result.session.id,
                session_revision: result.session.revision,
                pointer: pointer.clone(),
                summary: ContractMessage {
                    message_id: pointer.through_turn_id,
                    role: MessageRole::Assistant,
                    text: request.summary,
                    call_id: None,
                    coverage: result.summary_coverage,
                },
            };
            receipt.validate()?;
            Ok(receipt)
        })
    }

    fn read_archive<'a>(
        &'a self,
        request: &'a ArchiveReadRequest,
    ) -> BoxFuture<'a, Result<ArchiveSnapshot, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            if request.person_id != self.vault.person_id() {
                return Err(AgentFailure::CapabilityDenied);
            }
            let recovery = session_recovery_pointer(&request.pointer);
            let source = self.vault.recover_session_with_coverage(&recovery).await?;
            if source.session.id != request.session_id
                || source.session.person_id != request.person_id
                || source.recovery != recovery
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            let archived = source
                .session
                .messages
                .get(..recovery.archived_message_count)
                .ok_or(AgentFailure::StorageUnavailable)?;
            let messages = archived
                .iter()
                .enumerate()
                .map(|(index, message)| {
                    let turn_id = message.turn_id();
                    let coverage = source
                        .coverage_by_turn
                        .get(&turn_id)
                        .cloned()
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    let message_id = Uuid::new_v5(
                        &recovery.archive_id,
                        &u64::try_from(index)
                            .map_err(|_| AgentFailure::StorageUnavailable)?
                            .to_be_bytes(),
                    );
                    Ok(ArchivedMessage {
                        turn_id,
                        message: floe_conversation::contract_message(
                            message, message_id, coverage,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, AgentFailure>>()?;
            Ok(ArchiveSnapshot {
                person_id: request.person_id,
                session_id: request.session_id,
                pointer: request.pointer.clone(),
                messages,
            })
        })
    }
}

impl<Keys> VaultConversationRepository<Keys> {
    pub fn new(vault: Arc<EncryptedAgentVault<Keys>>) -> Self {
        Self { vault }
    }
}

impl<Keys: VaultKeyProvider> VaultConversationRepository<Keys> {
    fn verify_principal(&self, principal: &str) -> Result<(), AgentFailure> {
        if principal != self.vault.person_id().to_string() {
            return Err(AgentFailure::CapabilityDenied);
        }
        Ok(())
    }

    async fn attach_run_references(
        &self,
        mut receipt: RunReceipt,
    ) -> Result<RunReceipt, AgentFailure> {
        let entries = self
            .vault
            .conversation_journal(receipt.run_id)
            .await?
            .into_iter()
            .map(|entry| {
                let event = serde_json::from_str::<JournalEvent>(&entry.payload)
                    .map_err(|_| AgentFailure::StorageUnavailable)?;
                if entry.kind != journal_event_kind(&event) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                Ok(JournalEntry {
                    revision: entry.revision,
                    event,
                })
            })
            .collect::<Result<Vec<_>, AgentFailure>>()?;
        let accounting = floe_conversation::project_run_accounting(&receipt, &entries)?;
        receipt.attempt_refs = accounting.attempt_refs;
        receipt.task_refs = accounting.task_refs;
        receipt.unresolved_attempts = accounting.unresolved_attempts;
        receipt.validate()?;
        Ok(receipt)
    }
}

impl<Keys: VaultKeyProvider + 'static> ConversationRepository
    for VaultConversationRepository<Keys>
{
    fn finish_blocked_run<'a>(
        &'a self,
        commit: floe_conversation::BlockedRunCommit,
    ) -> BoxFuture<'a, Result<RunReceipt, AgentFailure>> {
        Box::pin(async move {
            let record = self.vault.finish_blocked_conversation_run(commit).await?;
            self.attach_run_references(floe_conversation::project_run_receipt(record)?)
                .await
        })
    }

    fn admit_turn<'a>(
        &'a self,
        request: TurnAdmissionRequest,
    ) -> BoxFuture<'a, Result<TurnAdmission, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            if request.principal != self.vault.person_id().to_string() {
                return Err(AgentFailure::CapabilityDenied);
            }
            match self.vault.admit_conversation_turn(request).await? {
                VaultConversationAdmission::Created { record, session } => {
                    let receipt = floe_conversation::project_run_receipt(record)?;
                    let transcript = floe_conversation::project_transcript(&session.messages)?;
                    Ok(TurnAdmission::Created(AdmittedTurn {
                        receipt,
                        transcript,
                    }))
                }
                VaultConversationAdmission::Existing(record) => Ok(TurnAdmission::Existing(
                    self.attach_run_references(floe_conversation::project_run_receipt(record)?)
                        .await?,
                )),
                VaultConversationAdmission::Resumed(record) => Ok(TurnAdmission::Resumed(
                    self.attach_run_references(floe_conversation::project_run_receipt(record)?)
                        .await?,
                )),
            }
        })
    }

    fn find_command<'a>(
        &'a self,
        query: floe_conversation::CommandQuery,
    ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>> {
        Box::pin(async move {
            query.validate()?;
            self.verify_principal(&query.principal)?;
            let Some(record) = self
                .vault
                .conversation_run_by_command(query.command_id)
                .await?
            else {
                return Ok(None);
            };
            Ok(Some(
                self.attach_run_references(floe_conversation::project_run_receipt(record)?)
                    .await?,
            ))
        })
    }

    fn admit_cancel<'a>(
        &'a self,
        request: CancelRunCommand,
    ) -> BoxFuture<'a, Result<CancelRunAdmission, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            if request.principal != self.vault.person_id().to_string() {
                return Err(AgentFailure::CapabilityDenied);
            }
            let admission = self
                .vault
                .admit_conversation_cancel(VaultConversationCancelRequest {
                    command_id: request.command_id,
                    run_id: request.run_id,
                    person_id: self.vault.person_id(),
                })
                .await?;
            let convert = |receipt: crate::VaultConversationCancelReceipt| CancelRunReceipt {
                command_id: receipt.command_id,
                run_id: receipt.run_id,
                principal: receipt.person_id.to_string(),
            };
            Ok(match admission {
                VaultConversationCancelAdmission::Created(receipt) => {
                    CancelRunAdmission::Created(convert(receipt))
                }
                VaultConversationCancelAdmission::Existing(receipt) => {
                    CancelRunAdmission::Existing(convert(receipt))
                }
            })
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
            let record = self
                .vault
                .finish_conversation_run(run_id, expected_aggregate_revision, terminal)
                .await?;
            self.attach_run_references(floe_conversation::project_run_receipt(record)?)
                .await
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
            let receipt = self
                .attach_run_references(floe_conversation::project_run_receipt(record)?)
                .await?;
            let session = floe_conversation::SessionStore::load(
                self.vault.as_ref(),
                self.vault.person_id(),
                receipt.session_id,
            )
            .await?;
            if session.revision != receipt.session_revision {
                return Err(AgentFailure::Conflict);
            }
            let transcript = floe_conversation::project_transcript(&session.messages)?;
            Ok(Some(AdmittedTurn {
                receipt,
                transcript,
            }))
        })
    }

    fn load_receipt<'a>(
        &'a self,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>> {
        Box::pin(async move {
            let Some(record) = self.vault.conversation_run(run_id).await? else {
                return Ok(None);
            };
            Ok(Some(
                self.attach_run_references(floe_conversation::project_run_receipt(record)?)
                    .await?,
            ))
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
                    request.command_id,
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
                    let expected_kind = journal_event_kind(&event);
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

impl<Keys: VaultKeyProvider + 'static> InteractionRepository for VaultConversationRepository<Keys> {
    fn get_interaction<'a>(
        &'a self,
        person_id: floe_kernel::PersonId,
        interaction_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<ConversationInteraction>, AgentFailure>> {
        Box::pin(async move {
            if interaction_id.is_nil() {
                return Err(AgentFailure::InvalidInput);
            }
            if person_id != self.vault.person_id() {
                return Ok(None);
            }
            self.vault.conversation_interaction(interaction_id).await
        })
    }

    fn list_run_interactions<'a>(
        &'a self,
        person_id: floe_kernel::PersonId,
        origin_run_id: RunId,
    ) -> BoxFuture<'a, Result<Vec<ConversationInteraction>, AgentFailure>> {
        Box::pin(async move {
            if !origin_run_id.is_valid() {
                return Err(AgentFailure::InvalidInput);
            }
            if person_id != self.vault.person_id() {
                return Ok(vec![]);
            }
            self.vault
                .run_conversation_interactions(origin_run_id)
                .await
        })
    }

    fn record_decision<'a>(
        &'a self,
        decision: InteractionDecision,
    ) -> BoxFuture<'a, Result<DecisionAdmission, AgentFailure>> {
        Box::pin(async move {
            decision.validate()?;
            if decision.principal != self.vault.person_id().to_string() {
                return Err(AgentFailure::CapabilityDenied);
            }
            self.vault
                .record_conversation_interaction_decision(decision)
                .await
        })
    }

    fn admit_refresh<'a>(
        &'a self,
        request: floe_conversation::InteractionRefresh,
    ) -> BoxFuture<'a, Result<ConversationInteraction, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .admit_conversation_interaction_refresh(request)
                .await
        })
    }

    fn resolve_and_request_resume<'a>(
        &'a self,
        commit: floe_conversation::InteractionResolutionCommit,
    ) -> BoxFuture<'a, Result<floe_conversation::InteractionResolutionReceipt, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .resolve_conversation_interaction_and_request_resume(commit)
                .await
        })
    }
    fn resolving_interactions<'a>(
        &'a self,
        person_id: floe_kernel::PersonId,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<ConversationInteraction>, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .resolving_conversation_interactions(person_id, limit)
                .await
        })
    }
    fn pending_resume_requests<'a>(
        &'a self,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<floe_conversation::ResumeRequired>, AgentFailure>> {
        Box::pin(async move { self.vault.pending_conversation_resume_requests(limit).await })
    }
    fn claim_resume<'a>(
        &'a self,
        request: floe_conversation::ResumeChildAdmission,
    ) -> BoxFuture<'a, Result<TurnAdmission, AgentFailure>> {
        Box::pin(async move {
            match self.vault.claim_conversation_resume(request).await? {
                VaultConversationAdmission::Created { record, session } => {
                    let receipt = self
                        .attach_run_references(floe_conversation::project_run_receipt(record)?)
                        .await?;
                    let transcript = floe_conversation::project_transcript(&session.messages)?;
                    Ok(TurnAdmission::Created(AdmittedTurn {
                        receipt,
                        transcript,
                    }))
                }
                VaultConversationAdmission::Existing(record) => Ok(TurnAdmission::Existing(
                    self.attach_run_references(floe_conversation::project_run_receipt(record)?)
                        .await?,
                )),
                VaultConversationAdmission::Resumed(record) => Ok(TurnAdmission::Resumed(
                    self.attach_run_references(floe_conversation::project_run_receipt(record)?)
                        .await?,
                )),
            }
        })
    }

    fn mark_superseded<'a>(
        &'a self,
        supersede: SupersedeInteraction,
    ) -> BoxFuture<'a, Result<ConversationInteraction, AgentFailure>> {
        Box::pin(async move {
            supersede.validate()?;
            if supersede.person_id != self.vault.person_id() {
                return Err(AgentFailure::CapabilityDenied);
            }
            self.vault
                .supersede_conversation_interaction(supersede)
                .await
        })
    }

    fn mark_expired<'a>(
        &'a self,
        expire: ExpireInteraction,
    ) -> BoxFuture<'a, Result<ExpireOutcome, AgentFailure>> {
        Box::pin(async move {
            expire.validate()?;
            if expire.person_id != self.vault.person_id() {
                return Err(AgentFailure::CapabilityDenied);
            }
            self.vault.expire_conversation_interaction(expire).await
        })
    }
}

fn journal_event_kind(event: &JournalEvent) -> &'static str {
    match event {
        JournalEvent::ModelIntent { .. }
        | JournalEvent::ToolIntent { .. }
        | JournalEvent::DelegationIntent { .. } => "intent",
        JournalEvent::ModelResult { .. }
        | JournalEvent::ToolResult { .. }
        | JournalEvent::DelegationResult { .. } => "result",
        JournalEvent::Output { .. } => "output",
        JournalEvent::FinalizationStarted { .. }
        | JournalEvent::Checkpoint { .. }
        | JournalEvent::ValidatedBatch { .. }
        | JournalEvent::BatchProgress { .. } => "checkpoint",
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

fn archive_pointer(recovery: &floe_conversation::SessionRecoveryPointer) -> ArchivePointer {
    ArchivePointer {
        archive_id: recovery.archive_id,
        source_revision: recovery.source_revision,
        through_turn_id: recovery.through_turn_id,
        archived_message_count: recovery.archived_message_count,
    }
}

fn session_recovery_pointer(pointer: &ArchivePointer) -> floe_conversation::SessionRecoveryPointer {
    floe_conversation::SessionRecoveryPointer {
        archive_id: pointer.archive_id,
        source_revision: pointer.source_revision,
        through_turn_id: pointer.through_turn_id,
        archived_message_count: pointer.archived_message_count,
    }
}
