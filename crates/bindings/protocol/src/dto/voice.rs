use serde::{Deserialize, Serialize};

use super::{CommandIdDto, SessionRefDto};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceStartStateDto {
    Preparing,
    Bound,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMediaStateDto {
    Preparing,
    Muted,
    Capturing,
    Stopped,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceRemoteStateDto {
    NotStarted,
    Dispatching,
    Connected,
    CloseRequested,
    Closed,
    Unknown,
}

/// Read-only reflection of durable Voice state; absent media fields mean that
/// the Start command has not bound a Voice call.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceStartStatusDto {
    pub command_id: CommandIdDto,
    pub surface_session_id: SessionRefDto,
    pub state: VoiceStartStateDto,
    pub media: Option<VoiceMediaStateDto>,
    pub remote: Option<VoiceRemoteStateDto>,
    pub stop_requested: Option<bool>,
    pub local_lease_released: Option<bool>,
}
