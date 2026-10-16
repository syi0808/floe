use crate::{
    AppendSurfaceInput, SubmitSurfaceWork, SurfaceAnchor, SurfaceInputReceipt, SurfaceWorkReceipt,
};
use floe_agent_contract::BoxFuture;
use floe_kernel::{AgentFailure, CommandFailure, OwnerActor};
use uuid::Uuid;

/// Conversation's durable user-facing admission boundary. Implementations must
/// verify Session ownership and atomically commit each receipt/prefix transition.
/// No method reserves or weakens a Core output recorder.
pub trait SurfaceRepository: Send + Sync {
    fn list_surface_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: crate::SurfaceWorkPageRequest,
    ) -> BoxFuture<'a, Result<crate::SurfaceWorkPage, AgentFailure>>;

    /// Only pending custody can be cancelled here. A linked Run returns a
    /// determinate refusal and retains its existing cancellation owner.
    fn cancel_pending_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: crate::CancelPendingWork,
    ) -> BoxFuture<'a, Result<crate::PendingWorkCancellationReceipt, CommandFailure<AgentFailure>>>;

    fn queued_surface_sessions<'a>(
        &'a self,
        actor: &'a OwnerActor,
        after: Option<Uuid>,
        limit: usize,
    ) -> BoxFuture<'a, Result<crate::RecoveryPage<Uuid, Uuid>, AgentFailure>>;
    fn surface_work_input<'a>(
        &'a self,
        actor: &'a OwnerActor,
        work: &'a SurfaceWorkReceipt,
    ) -> BoxFuture<'a, Result<SurfaceInputReceipt, AgentFailure>>;
    fn submit_text_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: crate::SubmitTextWork,
    ) -> BoxFuture<'a, Result<SurfaceWorkReceipt, CommandFailure<AgentFailure>>>;
    fn read_surface_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: floe_kernel::CommandId,
    ) -> BoxFuture<'a, Result<Option<crate::SurfaceWorkSnapshot>, AgentFailure>>;
    /// FIFO candidates for one Session. These are observations, not claims.
    fn pending_surface_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        session_id: Uuid,
    ) -> BoxFuture<'a, Result<Vec<SurfaceWorkReceipt>, AgentFailure>>;
    fn surface_head<'a>(
        &'a self,
        actor: &'a OwnerActor,
        session_id: Uuid,
    ) -> BoxFuture<'a, Result<SurfaceAnchor, AgentFailure>>;
    fn append_surface_input<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: AppendSurfaceInput,
    ) -> BoxFuture<'a, Result<SurfaceInputReceipt, CommandFailure<AgentFailure>>>;
    fn submit_surface_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: SubmitSurfaceWork,
    ) -> BoxFuture<'a, Result<SurfaceWorkReceipt, CommandFailure<AgentFailure>>>;
    /// Forward bounded page; never reads beyond the supplied immutable anchor.
    fn surface_entries<'a>(
        &'a self,
        actor: &'a OwnerActor,
        through: SurfaceAnchor,
        after: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<crate::SurfaceEntryReceipt>, AgentFailure>>;
}
