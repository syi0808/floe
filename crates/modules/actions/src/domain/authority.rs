use floe_day::PersonId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionAuthorityMode {
    Allow,
    #[default]
    Ask,
    Deny,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionAuthority {
    pub person_id: PersonId,
    pub calendar_create: ActionAuthorityMode,
}

impl ActionAuthority {
    pub fn default_for(person_id: PersonId) -> Self {
        Self {
            person_id,
            calendar_create: ActionAuthorityMode::Ask,
        }
    }
}
