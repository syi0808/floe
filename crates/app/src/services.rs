//! Temporary S2 owner adapters; Conversation values live in its owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceError {
    InvalidInput,
    NotFound,
    Conflict,
    AccessDenied,
    Unavailable,
    Internal,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CalendarActionsResult {
    pub actions: Vec<floe_actions::CalendarAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub writes_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<floe_actions::ActionAuthority>,
}
