//! Personal View identifiers.
//!
//! Standing personal source resources and subjects belong to Connections.

pub const ATTENTION_CONNECTOR: &str = "attention.macos";
pub const ATTENTION_RESOURCE: &str = "attention.coarse";
pub const PEOPLE_RESOURCE: &str = "people.identity";
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
