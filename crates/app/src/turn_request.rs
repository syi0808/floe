//! One conversation turn, as this host states it.
//!
//! What the Person asked for and which device asked. None of it is a wire
//! shape, and none of it is a resolved route: the canonical owners admit the
//! stored credential after the turn is admitted.

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
/// Production always uses [`SavedConnectionSource::HostSlot`]: the host
/// keychain slot, re-read on every check. App unit tests inject
/// `Fixed` instead — including a fixed absence — so no test depends on
/// ambient keychain state and no test convention can change production
/// credential lookup.
///
/// The choice of credential source is not part of the public turn contract:
/// external callers construct turns through
/// [`ConversationTurnRequest::new`], which always binds the host slot, and
/// cannot name this type.
#[derive(Clone)]
pub(crate) enum SavedConnectionSource {
    HostSlot,
    #[cfg(test)]
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
    /// identity. Private so production callers cannot select a credential
    /// source; see [`ConversationTurnRequest::new`].
    saved_server_connection: SavedConnectionSource,
}

impl ConversationTurnRequest {
    /// The one production turn shape: intent only, always bound to the host
    /// saved-connection slot. The canonical owners admit the stored
    /// credential after admission and re-read it on every authority check.
    pub fn new(
        session_id: Uuid,
        expected_revision: u64,
        text: String,
        device_id: String,
        profile: ProfileSelection,
        continuation: bool,
        retry_of: Option<RunId>,
    ) -> Self {
        Self {
            session_id,
            expected_revision,
            text,
            profile,
            continuation,
            retry_of,
            device_id,
            saved_server_connection: SavedConnectionSource::HostSlot,
        }
    }

    /// Bind a fixed saved connection for hermetic App unit tests, including
    /// a fixed absence. Compile-time test-only: this helper and the `Fixed`
    /// source do not exist in production builds.
    #[cfg(test)]
    pub(crate) fn with_fixed_saved_connection_for_test(
        self,
        saved: Option<floe_inference::SavedServerConnection>,
    ) -> Self {
        Self {
            saved_server_connection: SavedConnectionSource::Fixed(saved),
            ..self
        }
    }

    /// The credential source for internal composition only. Never re-exposed
    /// through a public signature.
    pub(crate) fn saved_connection_source(&self) -> SavedConnectionSource {
        self.saved_server_connection.clone()
    }

    /// The saved server credential this turn may use: the injected fixed
    /// connection when one was supplied, else the host keychain slot. A local
    /// read only: no network, no route resolution, no model or source
    /// discovery. Crate-internal: external callers express turn intent
    /// through [`ConversationTurnRequest::new`] and must not read the saved
    /// credential.
    pub(crate) fn stored_server_connection(
        &self,
    ) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
        match self.saved_server_connection.clone() {
            #[cfg(test)]
            SavedConnectionSource::Fixed(stored) => Ok(stored),
            SavedConnectionSource::HostSlot => {
                floe_provider_adapters::control::load_saved_connection()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_constructor_binds_the_host_slot() {
        let request = ConversationTurnRequest::new(
            Uuid::new_v4(),
            3,
            "Hello".into(),
            "mac-local".into(),
            ProfileSelection::Auto,
            false,
            None,
        );
        assert!(matches!(
            request.saved_connection_source(),
            SavedConnectionSource::HostSlot
        ));
    }

    #[test]
    fn fixed_injection_returns_the_fixed_store_without_the_keychain() {
        let person = floe_kernel::PersonId::new().to_string();
        let saved = floe_inference::SavedServerConnection {
            base_url: "http://127.0.0.1:9".into(),
            token: "t".repeat(32),
            client_id: "test-client".into(),
            person_id: person.clone(),
            device_id: "mac-local".into(),
            allow_external: false,
            external_recipients: vec![],
        };
        let present = ConversationTurnRequest::new(
            Uuid::new_v4(),
            0,
            "Hello".into(),
            "mac-local".into(),
            ProfileSelection::Auto,
            false,
            None,
        )
        .with_fixed_saved_connection_for_test(Some(saved.clone()));
        let stored = present.stored_server_connection().unwrap().unwrap();
        assert_eq!(stored.base_url, saved.base_url);
        assert_eq!(stored.person_id, person);

        let absent = ConversationTurnRequest::new(
            Uuid::new_v4(),
            0,
            "Hello".into(),
            "mac-local".into(),
            ProfileSelection::Auto,
            false,
            None,
        )
        .with_fixed_saved_connection_for_test(None);
        assert!(absent.stored_server_connection().unwrap().is_none());
    }
}
