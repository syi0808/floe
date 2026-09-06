use floe_domain::PersonId;
use serde::{Deserialize, Serialize};

use crate::{CoreError, FloeCore};

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
    fn default_for(person_id: PersonId) -> Self {
        Self {
            person_id,
            calendar_create: ActionAuthorityMode::Ask,
        }
    }
}

impl FloeCore {
    pub async fn action_authority(
        &self,
        person_id: PersonId,
    ) -> Result<ActionAuthority, CoreError> {
        Ok(self
            .store
            .action_authority(person_id)
            .await?
            .unwrap_or_else(|| ActionAuthority::default_for(person_id)))
    }

    pub async fn set_action_authority(
        &self,
        person_id: PersonId,
        calendar_create: ActionAuthorityMode,
    ) -> Result<ActionAuthority, CoreError> {
        let authority = ActionAuthority {
            person_id,
            calendar_create,
        };
        self.store.put_action_authority(&authority).await?;
        Ok(authority)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn authority_defaults_to_ask_and_persists_per_person() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("authority.db");
        let person = PersonId::new();
        let other = PersonId::new();
        let core = FloeCore::open(&path).await.unwrap();

        assert_eq!(
            core.action_authority(person).await.unwrap().calendar_create,
            ActionAuthorityMode::Ask
        );
        core.set_action_authority(person, ActionAuthorityMode::Allow)
            .await
            .unwrap();
        assert_eq!(
            core.action_authority(other).await.unwrap().calendar_create,
            ActionAuthorityMode::Ask
        );

        drop(core);
        let reopened = FloeCore::open(path).await.unwrap();
        assert_eq!(
            reopened
                .action_authority(person)
                .await
                .unwrap()
                .calendar_create,
            ActionAuthorityMode::Allow
        );
    }
}
