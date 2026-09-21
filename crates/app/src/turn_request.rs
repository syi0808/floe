//! One conversation turn, as this host states it.
//!
//! What the Person asked for and which device asked. None of it is a wire
//! shape, and none of it is a resolved route: the canonical owners admit the
//! stored credential after the turn is admitted.

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

#[derive(Clone, Debug)]
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
    pub device_id: String,
}

impl ConversationTurnRequest {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_debug_contains_only_intent() {
        let request = ConversationTurnRequest::new(
            Uuid::new_v4(),
            3,
            "Hello".into(),
            "mac-local".into(),
            ProfileSelection::Auto,
            false,
            None,
        );
        let debug = format!("{request:?}");
        for secret in ["token", "bearer", "base_url", "connection", "HostSlot"] {
            assert!(!debug.contains(secret));
        }
        assert!(debug.contains("Hello"));
        assert!(debug.contains("mac-local"));
    }
}
