//! Storage-bound implementation of the Conversation Surface port.
use super::*;
use floe_conversation::{
    AppendSurfaceInput, SubmitSurfaceWork, SurfaceAnchor, SurfaceInputReceipt, SurfaceRepository,
    SurfaceWorkReceipt, SurfaceWorkSnapshot,
};
use floe_kernel::{CommandFailure, CommandId, OwnerActor};

impl<Keys: VaultKeyProvider + 'static> SurfaceRepository for VaultConversationRepository<Keys> {
    fn cancel_pending_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: floe_conversation::CancelPendingWork,
    ) -> BoxFuture<
        'a,
        Result<floe_conversation::PendingWorkCancellationReceipt, CommandFailure<AgentFailure>>,
    > {
        self.vault.cancel_pending_work(actor, request)
    }

    fn queued_surface_sessions<'a>(
        &'a self,
        actor: &'a OwnerActor,
        after: Option<Uuid>,
        limit: usize,
    ) -> BoxFuture<'a, Result<floe_conversation::RecoveryPage<Uuid, Uuid>, AgentFailure>> {
        self.vault.queued_surface_sessions(actor, after, limit)
    }
    fn surface_work_input<'a>(
        &'a self,
        actor: &'a OwnerActor,
        work: &'a SurfaceWorkReceipt,
    ) -> BoxFuture<'a, Result<SurfaceInputReceipt, AgentFailure>> {
        self.vault.surface_work_input(actor, work)
    }
    fn submit_text_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: floe_conversation::SubmitTextWork,
    ) -> BoxFuture<'a, Result<SurfaceWorkReceipt, CommandFailure<AgentFailure>>> {
        self.vault.submit_text_work(actor, request)
    }

    fn surface_head<'a>(
        &'a self,
        actor: &'a OwnerActor,
        session_id: Uuid,
    ) -> BoxFuture<'a, Result<SurfaceAnchor, AgentFailure>> {
        self.vault.surface_head(actor, session_id)
    }
    fn append_surface_input<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: AppendSurfaceInput,
    ) -> BoxFuture<'a, Result<SurfaceInputReceipt, CommandFailure<AgentFailure>>> {
        self.vault.append_surface_input(actor, request)
    }
    fn submit_surface_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: SubmitSurfaceWork,
    ) -> BoxFuture<'a, Result<SurfaceWorkReceipt, CommandFailure<AgentFailure>>> {
        self.vault.submit_surface_work(actor, request)
    }
    fn surface_entries<'a>(
        &'a self,
        actor: &'a OwnerActor,
        through: SurfaceAnchor,
        after: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<floe_conversation::SurfaceEntryReceipt>, AgentFailure>> {
        self.vault.surface_entries(actor, through, after, limit)
    }
    fn read_surface_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
    ) -> BoxFuture<'a, Result<Option<SurfaceWorkSnapshot>, AgentFailure>> {
        self.vault.read_surface_work(actor, command_id)
    }
    fn pending_surface_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        session_id: Uuid,
    ) -> BoxFuture<'a, Result<Vec<SurfaceWorkReceipt>, AgentFailure>> {
        self.vault.pending_surface_work(actor, session_id)
    }
}

impl<Keys: VaultKeyProvider + 'static> VaultConversationRepository<Keys> {
    /// Build from the admitted prefix, never from a mutable Manager Core head.
    /// The execution's user alias is a derived projection of the retained input;
    /// original Surface identity and bytes stay in its receipt.
    pub(super) async fn manager_history_for_work(
        &self,
        work: &SurfaceWorkReceipt,
        execution_user_id: Uuid,
    ) -> Result<(Vec<ContractMessage>, bool), AgentFailure> {
        work.validate()?;
        if work.person_id != self.vault.person_id() {
            return Err(AgentFailure::PolicyDenied);
        }
        let actor = OwnerActor {
            person_id: work.person_id,
            device_id: work.device_id.clone(),
            runtime_epoch: 1,
        };
        let after = work
            .anchor
            .sequence
            .saturating_sub(floe_conversation::MAX_SURFACE_PAGE_ITEMS as u64);
        let page = self
            .vault
            .surface_entries(
                &actor,
                work.anchor,
                after,
                floe_conversation::MAX_SURFACE_PAGE_ITEMS,
            )
            .await?;
        let input = page
            .iter()
            .filter_map(|item| item.input())
            .find(|item| item.input.message_id == work.input_id)
            .ok_or(AgentFailure::ModelInputCapacityExceeded)?;
        let selected = ContractMessage {
            message_id: execution_user_id,
            role: floe_agent_contract::MessageRole::User,
            text: input.input.text.clone(),
            call_id: None,
            coverage: floe_agent_contract::DependencyCoverage::Independent,
        };
        let mut messages = Vec::new();
        for item in page {
            match item {
                floe_conversation::SurfaceEntryReceipt::Input(item) => {
                    if item.input.message_id != work.input_id {
                        messages.push(ContractMessage {
                            message_id: item.input.message_id.as_uuid(),
                            role: floe_agent_contract::MessageRole::User,
                            text: item.input.text,
                            call_id: None,
                            coverage: floe_agent_contract::DependencyCoverage::Independent,
                        });
                    }
                }
                floe_conversation::SurfaceEntryReceipt::PendingWorkCancelled(item) => {
                    let target = self
                        .vault
                        .read_surface_work(&actor, item.cancellation.work_command_id)
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    if target.receipt.work_id != item.cancellation.work_id
                        || target.receipt.anchor.session_id != work.anchor.session_id
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    // A normalized Person command, not invented assistant
                    // speech and not a mutation of the original user text.
                    messages.push(ContractMessage {
                        message_id: item.message_id().as_uuid(),
                        role: floe_agent_contract::MessageRole::User,
                        text: serde_json::json!({"person_intent": "cancel_pending_work", "input_message_id": target.receipt.input_id.as_uuid(), "work_id": target.receipt.work_id.as_uuid()}).to_string(),
                        call_id: None,
                        coverage: floe_agent_contract::DependencyCoverage::Independent,
                    });
                }
                floe_conversation::SurfaceEntryReceipt::PublishedResult(item) => {
                    let run = self
                        .vault
                        .conversation_run(item.result.run_id)
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    if floe_conversation::SurfaceResultReference::for_run(&run)? != item.result
                        || run.session_id != work.anchor.session_id
                        || run.person_id != work.person_id
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    messages.push(ContractMessage {
                        // Context reauthorizes canonical Run evidence, not the
                        // presentation entry's separate UI message identity.
                        message_id: item.result.run_id.as_uuid(),
                        role: floe_agent_contract::MessageRole::Assistant,
                        text: run.output.ok_or(AgentFailure::StorageUnavailable)?,
                        call_id: None,
                        coverage: run.coverage,
                    });
                }
            }
        }
        messages.push(selected);
        for message in &messages {
            message.validate()?;
        }
        Ok((messages, after > 0))
    }
}
