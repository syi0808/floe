//! Storage-bound implementation of the Conversation Surface port.
use super::*;
use floe_conversation::{
    AppendSurfaceInput, SubmitSurfaceWork, SurfaceAnchor, SurfaceInputReceipt, SurfaceRepository,
    SurfaceWorkReceipt, SurfaceWorkSnapshot,
};
use floe_kernel::{CommandFailure, CommandId, OwnerActor};

impl<Keys: VaultKeyProvider + 'static> SurfaceRepository for VaultConversationRepository<Keys> {
    fn list_surface_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: floe_conversation::SurfaceWorkPageRequest,
    ) -> BoxFuture<'a, Result<floe_conversation::SurfaceWorkPage, AgentFailure>> {
        self.vault.list_surface_work(actor, request)
    }

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
        self.vault
            .surface_model_history_for_work(work, execution_user_id)
            .await
    }
}
