use serde::{Deserialize, Serialize};

use crate::{
    ConnectionId, ConnectorId, GrantConsumer, GrantOperation, GrantPurpose, GrantValidationError,
    MAX_RESOURCE_HANDLES, ResourceHandle, SourceAuthority,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SourceReadOutcome<Value> {
    Ready(Value),
    Unavailable(SourceUnavailable),
    NeedsUserAction(SourceAccessRequirement),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceUnavailable {
    TemporarilyUnavailable,
    NoMatchingData,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceAccessRequirementKind {
    EnableObserve,
    ReviewChangedSource,
    RequestSystemPermission,
    Reconnect,
    ApproveProcessingRecipient,
    SelectResource,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAccessRequirement {
    source_id: String,
    connector_id: Option<ConnectorId>,
    connection_id: Option<ConnectionId>,
    operation: GrantOperation,
    consumer: GrantConsumer,
    purpose: GrantPurpose,
    resources: Vec<ResourceHandle>,
    processing_recipient: Option<String>,
    reason: SourceAccessRequirementKind,
    source_authority: Option<SourceAuthority>,
    inline_resolution: bool,
}

impl SourceAccessRequirement {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        source_id: impl Into<String>,
        connector_id: Option<ConnectorId>,
        connection_id: Option<ConnectionId>,
        operation: GrantOperation,
        consumer: GrantConsumer,
        purpose: GrantPurpose,
        resources: Vec<ResourceHandle>,
        processing_recipient: Option<String>,
        reason: SourceAccessRequirementKind,
        source_authority: Option<SourceAuthority>,
        inline_resolution: bool,
    ) -> Result<Self, GrantValidationError> {
        let requirement = Self {
            source_id: source_id.into(),
            connector_id,
            connection_id,
            operation,
            consumer,
            purpose,
            resources,
            processing_recipient,
            reason,
            source_authority,
            inline_resolution,
        };
        requirement.validate()?;
        Ok(requirement)
    }

    pub fn validate(&self) -> Result<(), GrantValidationError> {
        super::validate_identifier(&self.source_id, 128, "source")?;
        if let Some(connector_id) = &self.connector_id {
            ConnectorId::try_new(connector_id.as_str().to_owned())?;
        }
        if let Some(connection_id) = &self.connection_id {
            ConnectionId::try_new(connection_id.as_str().to_owned())?;
        }
        GrantConsumer::new(self.consumer.clone())?;
        if self.resources.len() > MAX_RESOURCE_HANDLES {
            return Err(GrantValidationError::ResourceCount);
        }
        for resource in &self.resources {
            ResourceHandle::try_new(resource.as_str().to_owned())?;
        }
        super::ensure_unique(&self.resources, "resource")?;
        if let Some(recipient) = &self.processing_recipient {
            super::validate_identifier(recipient, 128, "processing recipient")?;
        }
        if (self.reason == SourceAccessRequirementKind::ApproveProcessingRecipient)
            != self.processing_recipient.is_some()
        {
            return Err(GrantValidationError::InvalidState);
        }
        if self
            .source_authority
            .is_some_and(|authority| !authority.is_valid())
        {
            return Err(GrantValidationError::InvalidState);
        }
        Ok(())
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }
    pub fn connector_id(&self) -> Option<&ConnectorId> {
        self.connector_id.as_ref()
    }
    pub fn connection_id(&self) -> Option<&ConnectionId> {
        self.connection_id.as_ref()
    }
    pub fn operation(&self) -> GrantOperation {
        self.operation
    }
    pub fn consumer(&self) -> &GrantConsumer {
        &self.consumer
    }
    pub fn purpose(&self) -> GrantPurpose {
        self.purpose
    }
    pub fn resources(&self) -> &[ResourceHandle] {
        &self.resources
    }
    pub fn processing_recipient(&self) -> Option<&str> {
        self.processing_recipient.as_deref()
    }
    pub fn reason(&self) -> SourceAccessRequirementKind {
        self.reason
    }
    pub fn source_authority(&self) -> Option<SourceAuthority> {
        self.source_authority
    }
    pub fn inline_resolution(&self) -> bool {
        self.inline_resolution
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requirement(source_id: &str) -> Result<SourceAccessRequirement, GrantValidationError> {
        SourceAccessRequirement::try_new(
            source_id,
            Some(ConnectorId::try_new("floe.connector.calendar")?),
            Some(ConnectionId::try_new("calendar-connection")?),
            GrantOperation::Read,
            GrantConsumer::builtin("floe.builtin.schedule")?,
            GrantPurpose::Scheduling,
            vec![ResourceHandle::try_new("personal")?],
            None,
            SourceAccessRequirementKind::EnableObserve,
            None,
            true,
        )
    }

    #[test]
    fn requirement_validates_bounded_identity_and_scope() {
        assert!(requirement("floe.source.calendar").is_ok());
        assert!(requirement("").is_err());
        assert!(requirement(&"x".repeat(129)).is_err());
        assert!(ConnectionId::try_new("").is_err());
        assert!(GrantConsumer::builtin(&"x".repeat(129)).is_err());
        assert!(ResourceHandle::try_new(&"x".repeat(257)).is_err());
    }

    #[test]
    fn serialized_requirement_contains_no_secret_authority() {
        let json = serde_json::to_string(&requirement("floe.source.calendar").unwrap()).unwrap();
        for forbidden in ["token", "bearer", "credential", "secret", "password"] {
            assert!(!json.contains(forbidden));
        }
    }

    #[test]
    fn processing_consent_names_exact_recipient() {
        let mut requirement = requirement("floe.source.calendar").unwrap();
        requirement.reason = SourceAccessRequirementKind::ApproveProcessingRecipient;
        assert!(requirement.validate().is_err());
        requirement.processing_recipient = Some("model.example".into());
        assert!(requirement.validate().is_ok());
    }
}
