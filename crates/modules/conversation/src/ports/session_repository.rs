use crate::{SessionReadRequest, SessionReceipt, SessionRequest};
use floe_agent_contract::BoxFuture;
use floe_kernel::AgentFailure;

pub trait SessionRepository: Send + Sync {
    fn start_session<'a>(
        &'a self,
        request: crate::StartSessionRequest,
    ) -> BoxFuture<'a, Result<crate::SessionStartAdmission, crate::SessionStartFailure>>;

    /// None means the validated store contains no resumable Session.
    fn resume_session<'a>(
        &'a self,
        request: SessionRequest,
    ) -> BoxFuture<'a, Result<Option<SessionReceipt>, AgentFailure>>;

    fn get_session<'a>(
        &'a self,
        request: SessionReadRequest,
    ) -> BoxFuture<'a, Result<SessionReceipt, AgentFailure>>;
}
