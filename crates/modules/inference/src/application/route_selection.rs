//! Where this host keeps the saved local-server connection.
//!
//! The canonical General Conversation path no longer performs pre-turn remote
//! route discovery. The credential store stays behind this owner-defined port;
//! the composition root injects it.

use floe_agent_contract::AgentFailure;

use crate::SavedServerConnection;

/// Where this host keeps the saved local-server connection.
pub trait SavedConnectionStore {
    fn load(&self) -> Result<Option<SavedServerConnection>, AgentFailure>;
}
