//! Storage-bound implementation of the Conversation Surface port.
use super::*;
use floe_conversation::{
    AppendSurfaceInput, SubmitSurfaceWork, SurfaceAnchor, SurfaceInputReceipt, SurfaceRepository,
    SurfaceWorkReceipt, SurfaceWorkSnapshot,
};
use floe_kernel::{CommandFailure, CommandId, OwnerActor};

impl<Keys: VaultKeyProvider + 'static> SurfaceRepository for VaultConversationRepository<Keys> {
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
    fn surface_inputs<'a>(
        &'a self,
        actor: &'a OwnerActor,
        through: SurfaceAnchor,
        after: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<SurfaceInputReceipt>, AgentFailure>> {
        self.vault.surface_inputs(actor, through, after, limit)
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
            .surface_inputs(
                &actor,
                work.anchor,
                after,
                floe_conversation::MAX_SURFACE_PAGE_ITEMS,
            )
            .await?;
        let input = page
            .iter()
            .find(|item| item.input.message_id == work.input_id)
            .ok_or(AgentFailure::ModelInputCapacityExceeded)?;
        let selected = ContractMessage {
            message_id: execution_user_id,
            role: floe_agent_contract::MessageRole::User,
            text: input.input.text.clone(),
            call_id: None,
            coverage: floe_agent_contract::DependencyCoverage::Independent,
        };
        let mut messages = page
            .into_iter()
            .filter(|item| item.input.message_id != work.input_id)
            .map(|item| ContractMessage {
                message_id: item.input.message_id.as_uuid(),
                role: floe_agent_contract::MessageRole::User,
                text: item.input.text,
                call_id: None,
                coverage: floe_agent_contract::DependencyCoverage::Independent,
            })
            .collect::<Vec<_>>();
        messages.push(selected);
        for message in &messages {
            message.validate()?;
        }
        Ok((messages, after > 0))
    }
}
