//! The JSON the bundled calendar driver speaks.
//!
//! This is the native transport's own codec, not the app wire. The field and
//! variant names below are what the shipped Swift and Kotlin drivers already
//! send, so they are fixed here and nowhere else: one copy, held by the layer
//! that makes the call.

use serde::{Deserialize, Serialize};

/// The version the native driver stamps on a payload.
pub const NATIVE_CALENDAR_WIRE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeCalendarFailure {
    PermissionDenied,
    CalendarUnavailable,
    ProviderUnavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeEventSchedule {
    Timed {
        starts_at: String,
        ends_at: String,
        timezone: String,
    },
    AllDay {
        start_date: String,
        end_date_exclusive: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NativeCalendarRecord {
    pub can_modify: bool,
    pub calendar_id: String,
    pub external_id: String,
    pub external_revision: String,
    pub title: String,
    pub schedule: NativeEventSchedule,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NativeCalendarBatch {
    pub calendar_id: String,
    pub records: Vec<NativeCalendarRecord>,
    pub failure: Option<NativeCalendarFailure>,
}
