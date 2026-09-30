use floe_agent_contract::AgentFailure;
use floe_connections::{ConnectionResource, ResourceMode, SourceConnection};
use floe_context_contract::{ATTENTION_VIEW_ID, PEOPLE_VIEW_ID, ResourceHandle, WELLBEING_VIEW_ID};

#[derive(Clone, Copy)]
pub(crate) struct PersonalSourceSpec {
    pub connector: &'static str,
    pub connection: &'static str,
    pub view: &'static str,
    pub mode: ResourceMode,
    owner_platform: &'static str,
    singleton_resource: Option<&'static str>,
}

impl PersonalSourceSpec {
    pub fn for_connector(connector: &str) -> Result<Self, AgentFailure> {
        match connector {
            "contacts.apple" => Ok(Self {
                connector: "contacts.apple",
                connection: "contacts.apple.local",
                view: PEOPLE_VIEW_ID,
                mode: ResourceMode::Selected,
                owner_platform: "apple",
                singleton_resource: None,
            }),
            "contacts.android" => Ok(Self {
                connector: "contacts.android",
                connection: "contacts.android.local",
                view: PEOPLE_VIEW_ID,
                mode: ResourceMode::Selected,
                owner_platform: "android",
                singleton_resource: None,
            }),
            "attention.macos" => Ok(Self {
                connector: "attention.macos",
                connection: "attention.macos.local",
                view: ATTENTION_VIEW_ID,
                mode: ResourceMode::AllAvailable,
                owner_platform: "macos",
                singleton_resource: Some(ATTENTION_VIEW_ID),
            }),
            "health.apple" => Ok(Self {
                connector: "health.apple",
                connection: "health.apple.local",
                view: WELLBEING_VIEW_ID,
                mode: ResourceMode::AllAvailable,
                owner_platform: "apple",
                singleton_resource: Some(WELLBEING_VIEW_ID),
            }),
            _ => Err(AgentFailure::InvalidInput),
        }
    }

    pub fn execution_owner(self, device_id: &str) -> Result<String, AgentFailure> {
        if device_id.is_empty()
            || device_id.len() > 128
            || device_id.chars().any(char::is_whitespace)
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(format!("{}:{device_id}", self.owner_platform))
    }

    pub fn resources(
        self,
        selected_handles: Vec<String>,
    ) -> Result<Vec<ConnectionResource>, AgentFailure> {
        let mut handles = match self.singleton_resource {
            Some(handle) if selected_handles.is_empty() => vec![handle.to_owned()],
            Some(_) => return Err(AgentFailure::InvalidInput),
            None if !selected_handles.is_empty() && selected_handles.len() <= 64 => {
                selected_handles
            }
            None => return Err(AgentFailure::InvalidInput),
        };
        handles.sort();
        if handles.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(AgentFailure::InvalidInput);
        }
        handles
            .into_iter()
            .map(|handle| {
                ConnectionResource::new(
                    ResourceHandle::try_new(&handle).map_err(|_| AgentFailure::InvalidInput)?,
                    handle,
                )
                .map_err(|_| AgentFailure::InvalidInput)
            })
            .collect()
    }

    pub fn validate_connection(
        self,
        source: &SourceConnection,
        device_id: &str,
    ) -> Result<(), AgentFailure> {
        if source.connector_id().as_str() != self.connector
            || source.connection_id().as_str() != self.connection
            || source.execution_owner_id().as_str() != self.execution_owner(device_id)?
            || source.resource_mode() != self.mode
            || !source.is_serving()
            || source.native_subject_fingerprint().is_none()
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        if let Some(handle) = self.singleton_resource {
            if source.resources().len() != 1 || source.resources()[0].handle().as_str() != handle {
                return Err(AgentFailure::AccessReviewRequired);
            }
        } else if source.resources().is_empty() || source.resources().len() > 64 {
            return Err(AgentFailure::AccessReviewRequired);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_personal_resources_have_canonical_modes_and_bounds() {
        for connector in ["attention.macos", "health.apple"] {
            let spec = PersonalSourceSpec::for_connector(connector).unwrap();
            assert_eq!(spec.mode, ResourceMode::AllAvailable);
            assert_eq!(
                spec.resources(vec![]).unwrap()[0].handle().as_str(),
                spec.view
            );
            assert!(spec.resources(vec![spec.view.into()]).is_err());
        }
        let contacts = PersonalSourceSpec::for_connector("contacts.apple").unwrap();
        assert_eq!(contacts.mode, ResourceMode::Selected);
        assert_eq!(
            contacts
                .resources(vec!["b".into(), "a".into()])
                .unwrap()
                .iter()
                .map(|resource| resource.handle().as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert!(contacts.resources(vec![]).is_err());
        assert!(contacts.resources(vec!["a".into(), "a".into()]).is_err());
        assert!(contacts.resources(vec!["a".repeat(257)]).is_err());
        assert!(contacts.resources(vec!["a".into(); 65]).is_err());
        assert!(PersonalSourceSpec::for_connector("unknown.connector").is_err());
    }
}
