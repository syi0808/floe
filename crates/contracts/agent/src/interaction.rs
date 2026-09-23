use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AgentFailure;

pub const USER_INTERACTION_MEDIA_TYPE: &str =
    "application/vnd.floe.user-interaction+json;version=1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserInteractionKind {
    SourceAccess,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserInteractionStatus {
    Pending,
    Allowed,
    Denied,
    Superseded,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UserInteractionRef {
    pub interaction_id: Uuid,
    pub kind: UserInteractionKind,
    pub status: UserInteractionStatus,
}

impl UserInteractionRef {
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
}
