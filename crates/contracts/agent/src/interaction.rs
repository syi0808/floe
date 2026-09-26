use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AgentFailure;

pub const USER_INTERACTION_MEDIA_TYPE: &str =
    "application/vnd.floe.user-interaction+json;version=1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserInteractionKind {
    SourceAccess,
    ProcessingRecipient,
    ExpertBinding,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_rejects_nil_and_unknown_values() {
        let reference = UserInteractionRef {
            interaction_id: Uuid::nil(),
            kind: UserInteractionKind::SourceAccess,
            status: UserInteractionStatus::Pending,
        };
        assert_eq!(reference.validate(), Err(AgentFailure::InvalidModelOutput));
        let mut value = serde_json::to_value(reference).unwrap();
        value["kind"] = "other".into();
        assert!(serde_json::from_value::<UserInteractionRef>(value).is_err());
        let mut value = serde_json::json!({
            "interaction_id": Uuid::new_v4(),
            "kind": "source_access",
            "status": "other"
        });
        assert!(serde_json::from_value::<UserInteractionRef>(value.take()).is_err());
    }

    #[test]
    fn lifecycle_statuses_round_trip() {
        for status in [
            UserInteractionStatus::Pending,
            UserInteractionStatus::Resolving,
            UserInteractionStatus::Resolved,
            UserInteractionStatus::Denied,
            UserInteractionStatus::Cancelled,
            UserInteractionStatus::Superseded,
            UserInteractionStatus::Expired,
        ] {
            for kind in [
                UserInteractionKind::SourceAccess,
                UserInteractionKind::ProcessingRecipient,
                UserInteractionKind::ExpertBinding,
            ] {
                let reference = UserInteractionRef {
                    interaction_id: Uuid::new_v4(),
                    kind,
                    status,
                };
                assert!(reference.validate().is_ok());
                let decoded: UserInteractionRef =
                    serde_json::from_str(&serde_json::to_string(&reference).unwrap()).unwrap();
                assert_eq!(decoded, reference);
            }
        }
    }

    #[test]
    fn removed_allowed_spelling_is_rejected() {
        let value = serde_json::json!({
            "interaction_id": Uuid::new_v4(),
            "kind": "source_access",
            "status": "allowed"
        });
        assert!(serde_json::from_value::<UserInteractionRef>(value).is_err());
    }
}
