//! Calendar identity values shared across source, grant and view boundaries.
//!
//! The provider a calendar came from and the breadth a connection was granted
//! are quoted by Access grants, Experts registry records, Conversation session
//! scope and the app wire alike.  Day owns the mirror, the range and the record;
//! it does not own these two names.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarProvider {
    Fixture,
    EventKit,
    #[serde(rename = "google_calendar")]
    Google,
    #[serde(rename = "microsoft_calendar")]
    Microsoft,
    Android,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarScope {
    Selected,
    All,
}

/// The immutable evidence a calendar read stands on.
///
/// The stamp crosses Access, the source adapter that produced it and the
/// Context projection that consumes it, so it lives with the shared values
/// rather than with any one of them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarReadAccessStamp {
    pub schema_version: u32,
    pub person_id: floe_kernel::PersonId,
    pub device_id: String,
    pub provider: CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub native_subject_fingerprint: String,
    pub generation: String,
}
