//! Confirmed interactions: the people a Person actually exchanged something
//! with, as one bounded source view.
//!
//! A confirmed interaction is only admissible against the people view it cites:
//! an interaction with an identity the Person never granted is not evidence.

use serde::{Deserialize, Serialize};

use floe_kernel::AGENT_VERSION;
use floe_kernel::AgentFailure;

use super::personal::PeopleView;
use crate::MAX_PERSONAL_CONTEXT_BYTES;

pub const CONFIRMED_INTERACTION_VIEW_ID: &str = "relationships.confirmed_interactions";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmedInteraction {
    pub identity_handle: String,
    pub evidence_handle: String,
    pub occurred_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmedInteractionView {
    pub schema_version: u32,
    pub view_id: String,
    pub source_handle: String,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub interactions: Vec<ConfirmedInteraction>,
}

pub fn validate_confirmed_interaction_view(
    view: &ConfirmedInteractionView,
    people: &PeopleView,
    now_unix_ms: i64,
) -> Result<(), AgentFailure> {
    if view.schema_version != AGENT_VERSION
        || view.view_id != CONFIRMED_INTERACTION_VIEW_ID
        || !valid_view_handle(&view.source_handle)
        || view.observed_at_unix_ms > now_unix_ms
        || view.expires_at_unix_ms <= now_unix_ms
        || view.expires_at_unix_ms <= view.observed_at_unix_ms
        || view.expires_at_unix_ms - view.observed_at_unix_ms > 300_000
        || view.interactions.len() > 64
        || serde_json::to_vec(view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > MAX_PERSONAL_CONTEXT_BYTES
    {
        return Err(AgentFailure::InvalidInput);
    }
    for (index, interaction) in view.interactions.iter().enumerate() {
        if !valid_view_handle(&interaction.identity_handle)
            || !valid_view_handle(&interaction.evidence_handle)
            || interaction.occurred_at_unix_ms < 0
            || interaction.occurred_at_unix_ms > view.observed_at_unix_ms
            || !people
                .identities
                .iter()
                .any(|identity| identity.identity_handle == interaction.identity_handle)
            || view.interactions[..index]
                .iter()
                .any(|other| other.evidence_handle == interaction.evidence_handle)
        {
            return Err(AgentFailure::InvalidInput);
        }
    }
    Ok(())
}

fn valid_view_handle(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128
}
