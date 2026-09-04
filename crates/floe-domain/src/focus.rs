use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::PersonId;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FocusPreferenceInput {
    pub start_minute: u16,
    pub end_minute: u16,
    pub duration_minutes: u16,
}

impl Default for FocusPreferenceInput {
    fn default() -> Self {
        Self {
            start_minute: 540,
            end_minute: 1080,
            duration_minutes: 60,
        }
    }
}

impl FocusPreferenceInput {
    pub fn is_valid(&self) -> bool {
        self.start_minute < self.end_minute
            && self.end_minute <= 1440
            && (15..=240).contains(&self.duration_minutes)
            && self.duration_minutes <= self.end_minute - self.start_minute
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusPreferenceSource {
    UserEntered,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FocusPreference {
    pub person_id: PersonId,
    pub revision: u64,
    pub source: FocusPreferenceSource,
    pub updated_at: DateTime<Utc>,
    pub value: Option<FocusPreferenceInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FocusSlot {
    pub id: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FocusEvidence {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FocusProposal {
    pub id: String,
    pub person_id: PersonId,
    pub generated_at: DateTime<Utc>,
    pub timezone_offset_seconds: i32,
    pub slot: FocusSlot,
    pub reason: String,
    pub evidence: Vec<FocusEvidence>,
    pub model: String,
    pub calendar_warning: bool,
}
