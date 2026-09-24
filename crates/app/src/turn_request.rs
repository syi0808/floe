//! One conversation turn, as this host states it.
//!
//! What the Person asked for and which device asked. None of it is a wire
//! shape, and none of it is a resolved route: the canonical owners admit the
//! stored credential after the turn is admitted.

use floe_conversation::ProfileSelection;
use floe_kernel::RunId;
use uuid::Uuid;

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

/// One linked resume, as this host states it.
///
/// Linkage only: the origin's text, profile and mode resolve from the
/// origin's durable admission at execution. None of it is a wire shape.
#[derive(Clone, Debug)]
pub struct ConversationResumeRequest {
    /// What the command is, in Conversation's own terms, for same-id
    /// duplicate checks.
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub device_id: String,
    pub origin_run_id: RunId,
    pub lineage: u8,
}

impl ConversationResumeRequest {
    pub fn new(
        session_id: Uuid,
        expected_revision: u64,
        device_id: String,
        origin_run_id: RunId,
        lineage: u8,
    ) -> Self {
        Self {
            session_id,
            expected_revision,
            device_id,
            origin_run_id,
            lineage,
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
