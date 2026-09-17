//! What the Person's own device sources are, as a grant names them.
//!
//! A grant binds a connector, a connection and the device that answers for it.
//! Those three names are the same whether a grant is being reviewed or a read is
//! being admitted under one, so they are stated once, here, by the owner that
//! decides what a grant may say.

use floe_context_contract::{
    ConnectionId, ConnectorId, ExecutionOwnerId, GrantSourceBinding, SourceAuthority,
};
use floe_kernel::{AgentFailure, PersonId};

pub const ATTENTION_CONNECTOR: &str = "attention.macos";
pub const ATTENTION_CONNECTION: &str = "attention.macos.local";
pub const ATTENTION_RESOURCE: &str = "attention.coarse";
pub const PEOPLE_RESOURCE: &str = "people.identity";
pub const FEASIBILITY_CONNECTOR: &str = "feasibility.apple";
pub const FEASIBILITY_CONNECTION: &str = "feasibility.apple.local";
pub const FEASIBILITY_RESOURCE: &str = "schedule.feasibility";
pub const WELLBEING_CONNECTOR: &str = "health.apple";
pub const WELLBEING_CONNECTION: &str = "health.apple.local";
pub const WELLBEING_RESOURCE: &str = "wellbeing.derived";

/// The device that answers for attention on this Person's behalf.
pub fn attention_execution_owner(device_id: &str) -> String {
    format!("macos:{device_id}")
}

/// The device that answers for the Apple personal sources.
pub fn apple_execution_owner(device_id: &str) -> String {
    format!("apple:{device_id}")
}

/// The connection a contacts source is bound to.
pub fn contacts_connection(connector: &str) -> String {
    format!("{connector}.local")
}

/// The device that answers for a contacts source.
pub fn contacts_execution_owner(connector: &str, device_id: &str) -> String {
    let platform = connector.strip_prefix("contacts.").unwrap_or("unknown");
    format!("{platform}:{device_id}")
}

pub fn source_binding(
    person_id: PersonId,
    connection: &str,
    connector: &str,
    execution_owner: String,
    authority: SourceAuthority,
) -> Result<GrantSourceBinding, AgentFailure> {
    GrantSourceBinding::try_new(
        person_id,
        ConnectionId::try_new(connection).map_err(|_| AgentFailure::InvalidInput)?,
        ConnectorId::try_new(connector).map_err(|_| AgentFailure::InvalidInput)?,
        ExecutionOwnerId::try_new(execution_owner).map_err(|_| AgentFailure::InvalidInput)?,
        authority,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

/// The source binding a feasibility grant is bound to.
pub fn feasibility_source(
    person_id: PersonId,
    device_id: &str,
    authority: SourceAuthority,
) -> Result<GrantSourceBinding, AgentFailure> {
    source_binding(
        person_id,
        FEASIBILITY_CONNECTION,
        FEASIBILITY_CONNECTOR,
        apple_execution_owner(device_id),
        authority,
    )
}

/// The source binding a wellbeing grant is bound to.
pub fn wellbeing_source(
    person_id: PersonId,
    device_id: &str,
    authority: SourceAuthority,
) -> Result<GrantSourceBinding, AgentFailure> {
    source_binding(
        person_id,
        WELLBEING_CONNECTION,
        WELLBEING_CONNECTOR,
        apple_execution_owner(device_id),
        authority,
    )
}

/// The source binding an attention grant is bound to.
pub fn attention_source(
    person_id: PersonId,
    device_id: &str,
    authority: SourceAuthority,
) -> Result<GrantSourceBinding, AgentFailure> {
    source_binding(
        person_id,
        ATTENTION_CONNECTION,
        ATTENTION_CONNECTOR,
        attention_execution_owner(device_id),
        authority,
    )
}

/// The source binding a contacts grant is bound to.
pub fn contacts_source(
    person_id: PersonId,
    device_id: &str,
    connector: &str,
    authority: SourceAuthority,
) -> Result<GrantSourceBinding, AgentFailure> {
    if !connector.starts_with("contacts.") {
        return Err(AgentFailure::InvalidInput);
    }
    source_binding(
        person_id,
        &contacts_connection(connector),
        connector,
        contacts_execution_owner(connector, device_id),
        authority,
    )
}
