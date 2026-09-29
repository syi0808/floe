//! Personal View identifiers and the contextual Feasibility grant source.
//!
//! Standing personal source resources and subjects belong to Connections.

use floe_context_contract::{ConnectionId, ConnectorId, ExecutionOwnerId, GrantSourceBinding};
use floe_kernel::{AgentFailure, PersonId};

pub const ATTENTION_CONNECTOR: &str = "attention.macos";
pub const ATTENTION_RESOURCE: &str = "attention.coarse";
pub const PEOPLE_RESOURCE: &str = "people.identity";
pub const FEASIBILITY_CONNECTOR: &str = "feasibility.apple";
pub const FEASIBILITY_CONNECTION: &str = "feasibility.apple.local";
pub const FEASIBILITY_RESOURCE: &str = "schedule.feasibility";
pub const WELLBEING_CONNECTOR: &str = "health.apple";
pub const WELLBEING_RESOURCE: &str = "wellbeing.derived";

/// The device that answers for the Apple personal sources.
pub fn apple_execution_owner(device_id: &str) -> String {
    format!("apple:{device_id}")
}

/// Whether a source is one this device serves for the Person themselves.
///
/// A device-local source is read here and re-admitted here; anything else is
/// served by the Person's paired server and re-admitted against it.
pub fn is_device_local_source(connector: &str) -> bool {
    connector == ATTENTION_CONNECTOR
        || connector.starts_with("calendar.")
        || matches!(
            connector,
            "contacts.apple" | "contacts.android" | "health.apple"
        )
}

fn source_binding(
    person_id: PersonId,
    connection: &str,
    connector: &str,
    execution_owner: String,
) -> Result<GrantSourceBinding, AgentFailure> {
    GrantSourceBinding::try_new(
        person_id,
        ConnectionId::try_new(connection).map_err(|_| AgentFailure::InvalidInput)?,
        ConnectorId::try_new(connector).map_err(|_| AgentFailure::InvalidInput)?,
        ExecutionOwnerId::try_new(execution_owner).map_err(|_| AgentFailure::InvalidInput)?,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

/// The source binding a feasibility grant is bound to.
pub fn feasibility_source(
    person_id: PersonId,
    device_id: &str,
) -> Result<GrantSourceBinding, AgentFailure> {
    source_binding(
        person_id,
        FEASIBILITY_CONNECTION,
        FEASIBILITY_CONNECTOR,
        apple_execution_owner(device_id),
    )
}
