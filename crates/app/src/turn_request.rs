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

/// Where the canonical owners read the saved server connection for one turn.
///
/// Production always uses [`TurnSavedConnection::HostSlot`]: the host keychain
/// slot, re-read on every check. Tests inject
/// [`TurnSavedConnection::Fixed`] instead — including a fixed absence — so no
/// test depends on ambient keychain state and no test convention can change
/// production credential lookup.
#[derive(Clone)]
pub enum TurnSavedConnection {
    HostSlot,
    Fixed(Option<floe_inference::SavedServerConnection>),
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
    /// Where the canonical model and source owners read the saved server
    /// connection: the host keychain slot in production, a fixed injected
    /// store in tests. Like any credential, it is excluded from the command
    /// identity.
    pub saved_server_connection: TurnSavedConnection,
}

impl ConversationTurnRequest {
    /// The saved server credential this turn may use: the injected fixed
    /// connection when one was supplied, else the host keychain slot. A local
    /// read only: no network, no route resolution, no model or source
    /// discovery.
    pub fn stored_server_connection(
        &self,
    ) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
        match self.saved_server_connection.clone() {
            TurnSavedConnection::Fixed(stored) => Ok(stored),
            TurnSavedConnection::HostSlot => {
                floe_provider_adapters::control::load_saved_connection()
            }
        }
    }
}
