use std::sync::Arc;

use crate::{
    EncryptedAgentVault, VaultConversationCancelAdmission, VaultConversationCancelRequest,
    VaultKeyProvider, VaultManagerConversationAdmission,
};
use floe_agent_contract::{
    AgentFailure, AgentMessage as ContractMessage, ArchivePointer, ArchiveReadRequest,
    ArchiveSnapshot, BoxFuture, ExecutionJournal, JournalAck, JournalEvent, MessageRole, RunId,
};
use floe_conversation::{
    AdmittedTurn, CancelRunAdmission, CancelRunCommand, CancelRunReceipt, CompactionReceipt,
    CompactionRequest, ConversationInteraction, ConversationRepository, DecisionAdmission,
    ExpireInteraction, ExpireOutcome, InteractionDecision, InteractionRepository, JournalEntry,
    RunReceipt, RunTerminal, SessionArchiveRepository, SessionReadRequest, SessionReceipt,
    SessionRepository, SessionRequest, SupersedeInteraction, TurnAdmission, TurnAdmissionRequest,
};
use uuid::Uuid;

pub struct VaultConversationRepository<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    manager_identity: floe_conversation_contract::AgentIdentity,
}

impl<Keys: VaultKeyProvider + 'static> SessionRepository for VaultConversationRepository<Keys> {
    fn start_session<'a>(
        &'a self,
        request: floe_conversation::StartSessionRequest,
    ) -> BoxFuture<
        'a,
        Result<floe_conversation::SessionStartAdmission, floe_conversation::SessionStartFailure>,
    > {
        Box::pin(async move { self.vault.start_conversation_session(request).await })
    }

    fn resume_session<'a>(
        &'a self,
        request: SessionRequest,
    ) -> BoxFuture<'a, Result<Option<SessionReceipt>, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            self.verify_principal(&request.principal)?;
            self.vault
                .resume_manager_conversation_session(&self.manager_identity)
                .await?
                .map(floe_conversation::project_session_receipt)
                .transpose()
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
                .compact_manager_session(
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
            self.vault.read_manager_session_archive(request).await
        })
    }
}

impl<Keys> VaultConversationRepository<Keys> {
    pub fn new(
        vault: Arc<EncryptedAgentVault<Keys>>,
        manager_identity: floe_conversation_contract::AgentIdentity,
    ) -> Self {
        Self {
            vault,
            manager_identity,
        }
    }
}

impl<Keys: VaultKeyProvider> VaultConversationRepository<Keys> {
    fn verify_principal(&self, principal: &str) -> Result<(), AgentFailure> {
        if principal != self.vault.person_id().to_string() {
            return Err(AgentFailure::CapabilityDenied);
        }
        Ok(())
    }

    async fn attach_run_references(&self, receipt: RunReceipt) -> Result<RunReceipt, AgentFailure> {
        let current = self
            .vault
            .accounted_conversation_receipt(receipt.run_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if current.command_id != receipt.command_id
            || current.principal != receipt.principal
            || current.device_id != receipt.device_id
            || current.session_id != receipt.session_id
            || current.request_digest != receipt.request_digest
            || current.expert_environment != receipt.expert_environment
            || current.executor_generation != receipt.executor_generation
            || current.aggregate_revision < receipt.aggregate_revision
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(current)
    }

    async fn manager_history_for_session(
        &self,
        session_id: Uuid,
        required_user_id: Option<Uuid>,
    ) -> Result<Vec<ContractMessage>, AgentFailure> {
        let page = self
            .vault
            .read_manager_session_history_page(
                session_id,
                None,
                required_user_id,
                floe_agent_contract::MAX_AGENT_MESSAGES,
                floe_conversation::MAX_SESSION_BYTES,
            )
            .await?;
        let mut seen = std::collections::HashSet::new();
        if page.messages.iter().any(|item| !seen.insert(item.alias_id)) {
            return Err(AgentFailure::StorageUnavailable);
        }
        page.messages
            .into_iter()
            .map(|item| {
                floe_conversation::contract_message(&item.message, item.alias_id, item.coverage)
            })
            .collect()
    }
}

impl<Keys: VaultKeyProvider + 'static> ConversationRepository
    for VaultConversationRepository<Keys>
{
    fn read_session_history_page<'a>(
        &'a self,
        session_id: Uuid,
        before_message_id: Option<Uuid>,
        limit: usize,
        byte_limit: usize,
    ) -> BoxFuture<'a, Result<floe_conversation::SessionHistoryPage, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .read_manager_session_history_page(
                    session_id,
                    before_message_id,
                    None,
                    limit,
                    byte_limit,
                )
                .await
        })
    }

    fn read_session_user_message<'a>(
        &'a self,
        session_id: Uuid,
        message_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<floe_conversation::SessionHistoryMessage>, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .read_manager_session_user_message(session_id, message_id)
                .await
        })
    }

    fn recovery_runs<'a>(
        &'a self,
        actor: &'a floe_kernel::OwnerActor,
        after: Option<RunId>,
        limit: usize,
    ) -> BoxFuture<'a, Result<floe_conversation::RecoveryPage<RunId, RunId>, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .conversation_recovery_runs(actor, after, limit)
                .await
        })
    }
    fn settle_pending_terminal<'a>(
        &'a self,
        actor: &'a floe_kernel::OwnerActor,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<RunReceipt, AgentFailure>> {
        Box::pin(async move {
            let record = self
                .vault
                .settle_pending_manager_conversation_terminal(actor, run_id)
                .await?;
            self.attach_run_references(floe_conversation::project_run_receipt(record)?)
                .await
        })
    }
    fn reconcile_delegation<'a>(
        &'a self,
        run_id: RunId,
        receipt: floe_agent_contract::TaskReceipt,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.vault
                .reconcile_manager_conversation_delegation(run_id, receipt)
                .await
                .map(|_| ())
        })
    }

    fn finish_blocked_run<'a>(
        &'a self,
        commit: floe_conversation::BlockedRunCommit,
    ) -> BoxFuture<'a, Result<RunReceipt, AgentFailure>> {
        Box::pin(async move {
            let record = self
                .vault
                .settle_manager_blocked_conversation_run(commit)
                .await?;
            self.attach_run_references(floe_conversation::project_run_receipt(record)?)
                .await
        })
    }

    fn admit_operation_approval<'a>(
        &'a self,
        admission: floe_calendar_operations::OperationAdmission,
        publication: floe_conversation::OperationApprovalPublication,
    ) -> BoxFuture<
        'a,
        Result<
            floe_conversation::OperationApprovalAdmission,
            floe_kernel::CommandFailure<AgentFailure>,
        >,
    > {
        Box::pin(async move {
            self.vault
                .admit_calendar_operation_and_conversation_interaction(admission, publication)
                .await
        })
    }

    fn admit_turn<'a>(
        &'a self,
        request: TurnAdmissionRequest,
    ) -> BoxFuture<'a, Result<TurnAdmission, floe_kernel::CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            if request.principal != self.vault.person_id().to_string() {
                return Err(floe_kernel::CommandFailure::NotAdmitted(
                    AgentFailure::CapabilityDenied,
                ));
            }
            let session_id = request.session_id;
            let required_user_id = match &request.input {
                floe_conversation::TurnInput::NewMessage(message) => Some(message.message_id),
                floe_conversation::TurnInput::ExistingMessage { message_id } => Some(*message_id),
            };
            match self
                .vault
                .admit_manager_conversation_turn(request, self.manager_identity.clone())
                .await?
            {
                VaultManagerConversationAdmission::Created(record) => {
                    let receipt = floe_conversation::project_run_receipt(record)
                        .map_err(floe_kernel::CommandFailure::Admitted)?;
                    let transcript = self
                        .manager_history_for_session(session_id, required_user_id)
                        .await
                        .map_err(floe_kernel::CommandFailure::Admitted)?;
                    Ok(TurnAdmission::Created(AdmittedTurn {
                        receipt,
                        transcript,
                    }))
                }
                VaultManagerConversationAdmission::Existing(record) => {
                    let receipt = floe_conversation::project_run_receipt(record)
                        .map_err(floe_kernel::CommandFailure::Admitted)?;
                    Ok(TurnAdmission::Existing(
                        self.attach_run_references(receipt)
                            .await
                            .map_err(floe_kernel::CommandFailure::Admitted)?,
                    ))
                }
                VaultManagerConversationAdmission::Resumed(record) => {
                    let receipt = floe_conversation::project_run_receipt(record)
                        .map_err(floe_kernel::CommandFailure::Admitted)?;
                    Ok(TurnAdmission::Resumed(
                        self.attach_run_references(receipt)
                            .await
                            .map_err(floe_kernel::CommandFailure::Admitted)?,
                    ))
                }
                VaultManagerConversationAdmission::Superseded => Err(
                    floe_kernel::CommandFailure::NotApplied(AgentFailure::Conflict),
                ),
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

    fn command_occupant<'a>(
        &'a self,
        command_id: floe_kernel::CommandId,
    ) -> BoxFuture<'a, Result<Option<floe_conversation::ConversationCommandKind>, AgentFailure>>
    {
        Box::pin(async move {
            if !command_id.is_valid() {
                return Err(AgentFailure::InvalidInput);
            }
            self.vault.conversation_command_occupant(command_id).await
        })
    }

    fn admit_cancel<'a>(
        &'a self,
        request: CancelRunCommand,
    ) -> BoxFuture<'a, Result<CancelRunAdmission, floe_kernel::CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            if request.principal != self.vault.person_id().to_string() {
                return Err(floe_kernel::CommandFailure::NotAdmitted(
                    AgentFailure::CapabilityDenied,
                ));
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
                .settle_manager_conversation_run(run_id, expected_aggregate_revision, terminal)
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
            let transcript = self
                .manager_history_for_session(receipt.session_id, Some(receipt.user_message_id))
                .await?;
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

    fn list_session_interactions<'a>(
        &'a self,
        person_id: floe_kernel::PersonId,
        session_id: Uuid,
        device_id: &'a str,
    ) -> BoxFuture<'a, Result<Vec<ConversationInteraction>, AgentFailure>> {
        Box::pin(async move {
            if person_id != self.vault.person_id() {
                return Ok(Vec::new());
            }
            self.vault
                .session_conversation_interactions(session_id, device_id)
                .await
        })
    }

    fn record_decision<'a>(
        &'a self,
        decision: InteractionDecision,
    ) -> BoxFuture<'a, Result<DecisionAdmission, floe_kernel::CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            if decision.principal != self.vault.person_id().to_string() {
                return Err(floe_kernel::CommandFailure::NotAdmitted(
                    AgentFailure::CapabilityDenied,
                ));
            }
            self.vault
                .record_conversation_interaction_decision(decision)
                .await
        })
    }

    fn admit_refresh<'a>(
        &'a self,
        request: floe_conversation::InteractionRefresh,
    ) -> BoxFuture<'a, Result<ConversationInteraction, floe_kernel::CommandFailure<AgentFailure>>>
    {
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
        actor: &'a floe_kernel::OwnerActor,
        after: Option<floe_conversation::InteractionRecoveryCursor>,
        limit: usize,
    ) -> BoxFuture<
        'a,
        Result<
            floe_conversation::RecoveryPage<
                ConversationInteraction,
                floe_conversation::InteractionRecoveryCursor,
            >,
            AgentFailure,
        >,
    > {
        Box::pin(async move {
            self.vault
                .resolving_conversation_interactions(actor, after, limit)
                .await
        })
    }
    fn pending_resume_requests<'a>(
        &'a self,
        actor: &'a floe_kernel::OwnerActor,
        after: Option<RunId>,
        limit: usize,
    ) -> BoxFuture<
        'a,
        Result<
            floe_conversation::RecoveryPage<floe_conversation::ResumeRequired, RunId>,
            AgentFailure,
        >,
    > {
        Box::pin(async move {
            self.vault
                .pending_conversation_resume_requests(actor, after, limit)
                .await
        })
    }
    fn pending_resume_request<'a>(
        &'a self,
        actor: &'a floe_kernel::OwnerActor,
        origin: RunId,
    ) -> BoxFuture<'a, Result<Option<floe_conversation::ResumeRequired>, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .pending_conversation_resume_request(actor, origin)
                .await
        })
    }
    fn reconcile_resume_request<'a>(
        &'a self,
        actor: &'a floe_kernel::OwnerActor,
        origin: RunId,
    ) -> BoxFuture<'a, Result<Option<floe_conversation::ResumeRequired>, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .reconcile_conversation_resume_request(actor, origin)
                .await
        })
    }
    fn claim_resume<'a>(
        &'a self,
        request: floe_conversation::ResumeChildAdmission,
    ) -> BoxFuture<'a, Result<TurnAdmission, AgentFailure>> {
        Box::pin(async move {
            let prepared_request = request.child.clone();
            match self
                .vault
                .claim_manager_conversation_resume(request, self.manager_identity.clone())
                .await?
            {
                VaultManagerConversationAdmission::Created(record) => {
                    let receipt = self
                        .attach_run_references(floe_conversation::project_run_receipt(record)?)
                        .await?;
                    let required_user_id = match &prepared_request.input {
                        floe_conversation::TurnInput::NewMessage(message) => {
                            Some(message.message_id)
                        }
                        floe_conversation::TurnInput::ExistingMessage { message_id } => {
                            Some(*message_id)
                        }
                    };
                    let transcript = self
                        .manager_history_for_session(prepared_request.session_id, required_user_id)
                        .await?;
                    Ok(TurnAdmission::Created(AdmittedTurn {
                        receipt,
                        transcript,
                    }))
                }
                VaultManagerConversationAdmission::Existing(record) => Ok(TurnAdmission::Existing(
                    self.attach_run_references(floe_conversation::project_run_receipt(record)?)
                        .await?,
                )),
                VaultManagerConversationAdmission::Resumed(record) => Ok(TurnAdmission::Resumed(
                    self.attach_run_references(floe_conversation::project_run_receipt(record)?)
                        .await?,
                )),
                VaultManagerConversationAdmission::Superseded => Err(AgentFailure::Conflict),
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
        | JournalEvent::ToolReviewRequired { .. }
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
            // StorageBusy is returned only after a transaction start/body
            // failure whose rollback was confirmed. Retry this exact local
            // journal append; commit failures are OutcomeUnknown and are not
            // retried here.
            let mut attempt = 0u32;
            let revision = loop {
                match self
                    .vault
                    .record_manager_conversation_journal(self.run_id, phase, event.clone())
                    .await
                {
                    Err(AgentFailure::StorageBusy) if attempt < 3 => {
                        tokio::time::sleep(std::time::Duration::from_millis(25 << attempt)).await;
                        attempt += 1;
                    }
                    result => break result?,
                }
            };
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
