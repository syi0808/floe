use serde::{Deserialize, Serialize};

use crate::{
    ConnectionId, ConnectorId, GrantAuthority, GrantConsumer, GrantId, GrantOperation,
    GrantPurpose, GrantValidationError, ResourceHandle, SourceAuthority,
};

pub fn source_access_id_for_capability(capability: &str) -> Option<&'static str> {
    match capability {
        "calendar.timeline" => Some("floe.source.calendar"),
        "mail.communication" => Some("floe.source.mail"),
        "floe.tasks" => Some("floe.source.tasks"),
        "memory.confirmed" => Some("floe.source.confirmed-memory"),
        "people.identity" => Some("floe.source.contacts"),
        "relationships.confirmed_interactions" => Some("floe.source.confirmed-interactions"),
        "attention.coarse" => Some("floe.source.attention"),
        "work.context" => Some("floe.source.work-context"),
        "wellbeing.derived" => Some("floe.source.wellbeing"),
        "life.logistics" => Some("floe.source.logistics"),
        _ => None,
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SourceReadOutcome<Value> {
    Ready(Value),
    Unavailable(SourceUnavailable),
    NeedsUserAction(SourceAccessBlockers),
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
    ReviewProcessing,
    SelectResource,
}

/// The exact live grant the classifying owner observed while producing a
/// requirement, or nothing when it proved that no live grant binds the
/// reviewed source. The reviewed descriptor binds this expectation; resolution
/// re-reads current authority and never treats a changed grant as reviewed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedGrant {
    grant_id: GrantId,
    authority: GrantAuthority,
}

impl ObservedGrant {
    pub fn try_new(
        grant_id: GrantId,
        authority: GrantAuthority,
    ) -> Result<Self, GrantValidationError> {
        let observed = Self {
            grant_id,
            authority,
        };
        observed.validate()?;
        Ok(observed)
    }

    pub fn validate(&self) -> Result<(), GrantValidationError> {
        if !self.grant_id.is_valid() || !self.authority.is_valid() {
            return Err(GrantValidationError::InvalidState);
        }
        Ok(())
    }

    pub fn grant_id(&self) -> GrantId {
        self.grant_id
    }

    pub fn authority(&self) -> GrantAuthority {
        self.authority
    }
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
    requested_processing: Option<crate::ProcessingRestriction>,
    reason: SourceAccessRequirementKind,
    source_authority: Option<SourceAuthority>,
    observed_grant: Option<ObservedGrant>,
    inline_resolution: bool,
    source_resources: Vec<ResourceHandle>,
    categories: Vec<crate::GrantDataCategory>,
    policy_digest: Option<[u8; 32]>,
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
        requested_processing: Option<crate::ProcessingRestriction>,
        reason: SourceAccessRequirementKind,
        source_authority: Option<SourceAuthority>,
        observed_grant: Option<ObservedGrant>,
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
            requested_processing,
            reason,
            source_authority,
            observed_grant,
            inline_resolution,
            source_resources: Vec::new(),
            categories: Vec::new(),
            policy_digest: None,
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
        for resource in &self.resources {
            ResourceHandle::try_new(resource.as_str().to_owned())?;
        }
        super::ensure_unique(&self.resources, "resource")?;
        if let Some(crate::ProcessingRestriction::GatewayAllowed { categories }) =
            &self.requested_processing
        {
            crate::ProcessingRestriction::gateway_allowed(categories.clone())?;
        }
        if (self.reason == SourceAccessRequirementKind::ReviewProcessing)
            != self.requested_processing.is_some()
        {
            return Err(GrantValidationError::InvalidState);
        }
        if self
            .source_authority
            .is_some_and(|authority| !authority.is_valid())
        {
            return Err(GrantValidationError::InvalidState);
        }
        if let Some(observed) = &self.observed_grant {
            observed.validate()?;
        }
        if self.reason == SourceAccessRequirementKind::ReviewProcessing {
            if !matches!(
                &self.requested_processing,
                Some(crate::ProcessingRestriction::GatewayAllowed { .. })
            ) {
                return Err(GrantValidationError::InvalidState);
            }
            if self.source_resources.is_empty()
                || self.categories.is_empty()
                || self.policy_digest.is_none_or(|digest| digest == [0; 32])
                || self.source_authority.is_none()
                || self.observed_grant.is_none()
                || self.resources.is_empty()
                || self.connector_id.is_none()
                || self.connection_id.is_none()
            {
                return Err(GrantValidationError::MissingScope);
            }
            super::ensure_unique(&self.source_resources, "source resource")?;
            super::ensure_unique(&self.categories, "category")?;
        }

        Ok(())
    }

    pub fn from_processing_dependency(
        dependency: &crate::ContextDependency,
    ) -> Result<Self, GrantValidationError> {
        dependency
            .validate()
            .map_err(|_| GrantValidationError::InvalidState)?;
        let policy =
            crate::ProcessingRestriction::gateway_allowed(dependency.categories().to_vec())?;
        let resource = dependency
            .resources()
            .first()
            .ok_or(GrantValidationError::MissingScope)?;
        let connection_id = dependency.source().connection_id();
        let view_id = crate::split_connection_view_resource(resource, &connection_id)?;
        let source_id =
            source_access_id_for_capability(view_id).ok_or(GrantValidationError::InvalidState)?;
        let mut requirement = Self {
            source_id: source_id.to_owned(),
            connector_id: Some(dependency.source().connector().clone()),
            connection_id: Some(dependency.source().connection_id()),
            operation: dependency.operation(),
            consumer: dependency.consumer().clone(),
            purpose: dependency.purpose(),
            resources: dependency.resources().to_vec(),
            requested_processing: Some(policy),
            reason: SourceAccessRequirementKind::ReviewProcessing,
            source_authority: Some(dependency.source_authority()),
            observed_grant: Some(ObservedGrant::try_new(
                dependency.grant_id(),
                dependency.grant_authority(),
            )?),
            inline_resolution: true,
            source_resources: Vec::new(),
            categories: Vec::new(),
            policy_digest: None,
        };
        requirement.source_resources = dependency.source_resources().to_vec();
        requirement.categories = dependency.categories().to_vec();
        use sha2::Digest;
        requirement.policy_digest = Some(
            sha2::Sha256::digest(
                serde_json::to_vec(&(
                    dependency.source(),
                    dependency.source_authority(),
                    dependency.resources(),
                    dependency.source_resources(),
                    dependency.grant_id(),
                    dependency.grant_authority(),
                    dependency.categories(),
                    dependency.operation(),
                    dependency.purpose(),
                    dependency.consumer(),
                    dependency.processing(),
                ))
                .map_err(|_| GrantValidationError::InvalidState)?,
            )
            .into(),
        );
        requirement.validate()?;
        Ok(requirement)
    }
    pub fn source_resources(&self) -> &[ResourceHandle] {
        &self.source_resources
    }
    pub fn categories(&self) -> &[crate::GrantDataCategory] {
        &self.categories
    }
    pub fn policy_digest(&self) -> Option<[u8; 32]> {
        self.policy_digest
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
    pub fn requested_processing(&self) -> Option<&crate::ProcessingRestriction> {
        self.requested_processing.as_ref()
    }
    pub fn reason(&self) -> SourceAccessRequirementKind {
        self.reason
    }
    pub fn source_authority(&self) -> Option<SourceAuthority> {
        self.source_authority
    }
    pub fn observed_grant(&self) -> Option<ObservedGrant> {
        self.observed_grant
    }
    pub fn inline_resolution(&self) -> bool {
        self.inline_resolution
    }
}

pub const MAX_SOURCE_ACCESS_BLOCKERS: usize = 8;

/// Several concrete review requirements, one per blocked source.
///
/// A multi-source read that is blocked on more than one source reports each
/// concrete blocker. Callers must not collapse them into a fake aggregate
/// source: every blocker keeps its own source identity, requirement detail
/// and grant/authority expectation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAccessBlockers {
    blockers: Vec<SourceAccessRequirement>,
}

impl SourceAccessBlockers {
    pub fn try_new(blockers: Vec<SourceAccessRequirement>) -> Result<Self, GrantValidationError> {
        if blockers.is_empty() {
            return Err(GrantValidationError::MissingScope);
        }
        if blockers.len() > MAX_SOURCE_ACCESS_BLOCKERS {
            return Err(GrantValidationError::TooLarge("blockers"));
        }
        for blocker in &blockers {
            blocker.validate()?;
        }
        for (index, blocker) in blockers.iter().enumerate() {
            if blockers[..index].contains(blocker) {
                return Err(GrantValidationError::Duplicate("blocker"));
            }
        }
        Ok(Self { blockers })
    }

    pub fn blockers(&self) -> &[SourceAccessRequirement] {
        &self.blockers
    }

    pub fn validate(&self) -> Result<(), GrantValidationError> {
        Self::try_new(self.blockers.clone()).map(|_| ())
    }
}
