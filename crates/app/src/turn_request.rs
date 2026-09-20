//! One conversation turn, as this host states it.
//!
//! What the Person asked for, which device asked, and which saved server
//! credential this run may use. None of it is a wire shape, and none of it is
//! a resolved route: the canonical owners admit the stored credential after
//! the turn is admitted.

use floe_agent_contract::AgentFailure;
use floe_conversation::ProfileSelection;
use floe_kernel::RunId;
use uuid::Uuid;

/// The paired server one outer remote authority/pairing operation names, and
/// what it reported.
///
/// Conversation turns no longer carry this: the canonical owners admit the
/// stored credential after admission. The outer compatibility operations keep
/// it until Stage 3.
///
/// The model route and the source catalog are two different admissions that one
/// resolution happens to observe together; they are kept apart here so that
/// neither is mistaken for the other.
#[derive(Clone)]
pub struct RemoteTurnRoute {
    pub route: floe_inference::RemoteRoute,
    pub calendar_connections: Vec<floe_connections::CalendarConnectionRef>,
}

impl RemoteTurnRoute {
    pub fn pairing(&self) -> Option<&floe_inference::RoutePairing> {
        self.route.pairing.as_ref()
    }

    /// The recipient this route resolves to, for the consent question.
    pub fn recipient(&self) -> floe_inference::RouteRecipient {
        floe_inference::RouteRecipient {
            external: self.route.external,
            allowed: self.route.allow_external,
        }
    }
}

#[derive(Clone)]
pub struct ConversationTurnRequest {
    /// What the command is, in Conversation's own terms. The worker compares
    /// these to tell a repeated command from a different one under the same id;
    /// the continuation reference itself resolves at prepare time from the same
    /// durable lineage, so same-id duplicates agree on it.
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub text: String,
    pub profile: ProfileSelection,
    pub continuation: bool,
    pub retry_of: Option<RunId>,
    /// Runtime-only inputs. Neither the asking device nor the stored
    /// credential is part of the command identity.
    pub device_id: String,
    /// Product-supplied saved server connection, if any. The canonical model
    /// and source owners admit it against the verified caller identity after
    /// admission. When absent, the host keychain slot is consulted. Like any
    /// credential, it is excluded from the command identity.
    pub saved_server_connection: Option<floe_inference::SavedServerConnection>,
}

impl ConversationTurnRequest {
    /// The saved server credential this turn may use: the product-injected
    /// connection when one was supplied, else the host keychain slot. A local
    /// read only: no network, no route resolution, no model or source
    /// discovery.
    pub fn stored_server_connection(
        &self,
    ) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
        match self.saved_server_connection.clone() {
            Some(stored) => Ok(Some(stored)),
            None => floe_provider_adapters::control::load_saved_connection(),
        }
    }
}
