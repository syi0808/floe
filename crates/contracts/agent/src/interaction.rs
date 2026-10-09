use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AgentFailure;

pub const USER_INTERACTION_MEDIA_TYPE: &str =
    "application/vnd.floe.user-interaction+json;version=1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserInteractionKind {
    SourceAccess,
    ExpertBinding,
    OperationApproval,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserInteractionStatus {
    Pending,
    Resolving,
    Resolved,
    Denied,
    Cancelled,
    Superseded,
    Expired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UserInteractionRef {
    pub interaction_id: Uuid,
    pub kind: UserInteractionKind,
    pub status: UserInteractionStatus,
}

impl UserInteractionRef {
    /// Shape validation only: a well-formed reference is never authorization.
    /// Only Conversation's trusted lookup of the durable interaction row
    /// decides what the reference means and whether it is actionable.
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.interaction_id.is_nil() {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
}
