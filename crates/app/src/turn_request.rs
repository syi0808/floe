//! One conversation turn, as this host states it.
//!
//! What the Person asked for, which device asked, and which paired server this
//! run may reach. None of it is a wire shape: the binding parses the request and
//! the composition root resolves the route before either reaches a turn.

use floe_conversation::ProfileSelection;
use floe_kernel::RunId;
use uuid::Uuid;

/// The paired server one turn may use, and what it reported.
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
    /// Runtime-only inputs. Neither the asking device nor the resolved route
    /// is part of the command identity.
    pub device_id: String,
    pub remote_route: Option<RemoteTurnRoute>,
    /// Product-supplied saved server connection for the canonical root model
    /// path, if any. The canonical provider admits it against the verified
    /// caller identity and never consumes `remote_route` for model selection.
    /// When absent, the host keychain slot is consulted. Like the route, this
    /// credential is excluded from the command identity.
    pub saved_server_connection: Option<floe_inference::SavedServerConnection>,
}
