use std::{future::Future, pin::Pin};

use floe_context_contract::ContextDependency;
use floe_kernel::AgentFailure;

use crate::ReleaseRecipient;

pub trait CurrentAuthority {
    fn validate_target(
        &self,
        recipient: ReleaseRecipient,
        session_id: uuid::Uuid,
        session_revision: u64,
    ) -> Result<(), AgentFailure>;

    fn validate<'a>(
        &'a self,
        dependency: &'a ContextDependency,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>>;
}
