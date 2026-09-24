//! Exact-recipient processing requirements for model dispatch consent.
//!
//! When an otherwise admissible external dispatch lacks contextual recipient
//! consent, Access derives one of these from the actual dispatch request: the
//! exact recipient, the reviewed route/profile identity, the authorized
//! consumer/purpose, the reviewed input data classes, the reviewed source
//! scope entries (empty for independent input), the original projection
//! identity (audit only), and the opaque intent lineage the consent binds.
//!
//! The requirement carries bounded non-secret facts only: no endpoint, bearer
//! token, provider credential, prompt, model output, or source payload. An
//! LLM-produced string never becomes one: Inference derives it from the
//! selected candidate and the Access decision.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    ConnectionId, ConnectorId, ConsumerPolicyAuthority, ContextDependency, DataClass,
    GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
    GrantValidationError, ResourceHandle, SourceAuthority,
};

pub const MAX_PROCESSING_SOURCE_SCOPES: usize = 64;
pub const MAX_PROCESSING_DATA_CLASSES: usize = 32;
pub const MAX_PROCESSING_REQUIREMENT_BYTES: usize = 64 * 1024;
pub const MAX_RECIPIENT_BYTES: usize = 256;
pub const MAX_PROFILE_ID_BYTES: usize = 128;
pub const MAX_PROCESSING_PURPOSE_BYTES: usize = 128;
pub const MAX_PROCESSING_CONSUMER_BYTES: usize = 128;

/// Opaque intent lineage a recipient consent binds: the Session and the origin
/// Run whose explicit intent the review authorizes.
///
/// Supplied by admitted App execution, never by Flutter. A linked resume
/// carries its origin's lineage; an unrelated turn carries its own and never
/// matches. Access never interprets these as Conversation rows; it only
/// compares them for equality.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipientLineage {
    session_id: Uuid,
    origin_run_id: Uuid,
}

impl RecipientLineage {
    pub fn try_new(session_id: Uuid, origin_run_id: Uuid) -> Result<Self, GrantValidationError> {
        let lineage = Self {
            session_id,
            origin_run_id,
        };
        lineage.validate()?;
        Ok(lineage)
    }

    pub fn validate(&self) -> Result<(), GrantValidationError> {
        if self.session_id.is_nil() || self.origin_run_id.is_nil() {
            return Err(GrantValidationError::InvalidState);
        }
        Ok(())
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub fn origin_run_id(&self) -> Uuid {
        self.origin_run_id
    }
}

/// One reviewed source scope entry: the exact source identity, resources,
/// categories, operation, purpose, consumer, and grant/source/policy
/// authorities the consent binds.
///
/// Observation-specific values (observation id, fingerprints, invocation ids,
/// timestamps) are deliberately absent: a fresh resume re-observes, and
/// consent compares review-relevant scope only. New account, expanded
/// resources, or changed authority never matches an old review.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessingSourceScope {
    connection_id: ConnectionId,
    connector_id: ConnectorId,
    resources: Vec<ResourceHandle>,
    categories: Vec<GrantDataCategory>,
    operation: GrantOperation,
    purpose: GrantPurpose,
    consumer: GrantConsumer,
    grant_id: GrantId,
    grant_authority: GrantAuthority,
    source_authority: SourceAuthority,
    policy_authority: ConsumerPolicyAuthority,
}

impl ProcessingSourceScope {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        connection_id: ConnectionId,
        connector_id: ConnectorId,
        mut resources: Vec<ResourceHandle>,
        mut categories: Vec<GrantDataCategory>,
        operation: GrantOperation,
        purpose: GrantPurpose,
        consumer: GrantConsumer,
        grant_id: GrantId,
        grant_authority: GrantAuthority,
        source_authority: SourceAuthority,
        policy_authority: ConsumerPolicyAuthority,
    ) -> Result<Self, GrantValidationError> {
        resources.sort();
        categories.sort();
        let scope = Self {
            connection_id,
            connector_id,
            resources,
            categories,
            operation,
            purpose,
            consumer,
            grant_id,
            grant_authority,
            source_authority,
            policy_authority,
        };
        scope.validate()?;
        Ok(scope)
    }

    /// The review-relevant scope of one authorized dependency.
    pub fn from_dependency(dependency: &ContextDependency) -> Result<Self, GrantValidationError> {
        dependency
            .validate()
            .map_err(|_| GrantValidationError::InvalidState)?;
        Self::try_new(
            dependency.source().connection_id(),
            dependency.source().connector().clone(),
            dependency.resources().to_vec(),
            dependency.categories().to_vec(),
            dependency.operation(),
            dependency.purpose(),
            dependency.consumer().clone(),
            dependency.grant_id(),
            dependency.grant_authority(),
            dependency.source().source_authority(),
            dependency.consumer_policy(),
        )
    }

    pub fn validate(&self) -> Result<(), GrantValidationError> {
        ConnectionId::try_new(self.connection_id.as_str().to_owned())?;
        ConnectorId::try_new(self.connector_id.as_str().to_owned())?;
        if self.resources.is_empty() || self.resources.len() > crate::MAX_RESOURCE_HANDLES {
            return Err(GrantValidationError::ResourceCount);
        }
        if self.categories.is_empty() {
            return Err(GrantValidationError::MissingScope);
        }
        for resource in &self.resources {
            ResourceHandle::try_new(resource.as_str().to_owned())?;
        }
        let mut sorted = self.resources.clone();
        sorted.sort();
        let mut sorted_categories = self.categories.clone();
        sorted_categories.sort();
        if sorted != self.resources || sorted_categories != self.categories {
            return Err(GrantValidationError::InvalidState);
        }
        super::ensure_unique(&self.resources, "resource")?;
        super::ensure_unique(&self.categories, "category")?;
        match &self.consumer {
            GrantConsumer::Builtin(name) => GrantConsumer::builtin(name.clone()),
            GrantConsumer::Extension(name) => GrantConsumer::extension(name.clone()),
        }?;
        if !self.grant_id.is_valid()
            || !self.grant_authority.is_valid()
            || !self.source_authority.is_valid()
            || !self.policy_authority.is_valid()
        {
            return Err(GrantValidationError::InvalidState);
        }
        Ok(())
    }

    fn canonical_bytes(&self) -> Result<Vec<u8>, GrantValidationError> {
        serde_json::to_vec(self).map_err(|_| GrantValidationError::InvalidState)
    }

    pub fn connection_id(&self) -> &ConnectionId {
        &self.connection_id
    }

    pub fn connector_id(&self) -> &ConnectorId {
        &self.connector_id
    }

    pub fn resources(&self) -> &[ResourceHandle] {
        &self.resources
    }

    pub fn categories(&self) -> &[GrantDataCategory] {
        &self.categories
    }

    pub fn operation(&self) -> GrantOperation {
        self.operation
    }

    pub fn purpose(&self) -> GrantPurpose {
        self.purpose
    }

    pub fn consumer(&self) -> &GrantConsumer {
        &self.consumer
    }

    pub fn grant_id(&self) -> GrantId {
        self.grant_id
    }

    pub fn grant_authority(&self) -> GrantAuthority {
        self.grant_authority
    }

    pub fn source_authority(&self) -> SourceAuthority {
        self.source_authority
    }

    pub fn policy_authority(&self) -> ConsumerPolicyAuthority {
        self.policy_authority
    }
}

/// The typed recoverable requirement for one blocked external dispatch.
///
/// Derived by Access from the actual dispatch request when the route is
/// otherwise admissible but no usable contextual consent exists. Prohibited
/// input (credentials, device-only raw, highly sensitive to external),
/// unknown coverage, LocalOnly-to-external, recipient mismatch, forged or
/// stale dependencies, and missing lineage never become one: those stay
/// fail-closed without a card.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessingRequirement {
    recipient: String,
    profile_id: String,
    purpose: String,
    consumer: String,
    input_data_classes: Vec<DataClass>,
    source_scopes: Vec<ProcessingSourceScope>,
    projection_ref: Uuid,
    projection_revision: u64,
    lineage: RecipientLineage,
}

impl ProcessingRequirement {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        recipient: impl Into<String>,
        profile_id: impl Into<String>,
        purpose: impl Into<String>,
        consumer: impl Into<String>,
        mut input_data_classes: Vec<DataClass>,
        mut source_scopes: Vec<ProcessingSourceScope>,
        projection_ref: Uuid,
        projection_revision: u64,
        lineage: RecipientLineage,
    ) -> Result<Self, GrantValidationError> {
        input_data_classes.sort();
        source_scopes.sort_by_cached_key(|scope| scope.canonical_bytes().unwrap_or_default());
        let requirement = Self {
            recipient: recipient.into(),
            profile_id: profile_id.into(),
            purpose: purpose.into(),
            consumer: consumer.into(),
            input_data_classes,
            source_scopes,
            projection_ref,
            projection_revision,
            lineage,
        };
        requirement.validate()?;
        Ok(requirement)
    }

    pub fn validate(&self) -> Result<(), GrantValidationError> {
        super::validate_identifier(&self.recipient, MAX_RECIPIENT_BYTES, "recipient")?;
        super::validate_identifier(&self.profile_id, MAX_PROFILE_ID_BYTES, "profile")?;
        super::validate_identifier(&self.purpose, MAX_PROCESSING_PURPOSE_BYTES, "purpose")?;
        super::validate_identifier(&self.consumer, MAX_PROCESSING_CONSUMER_BYTES, "consumer")?;
        if self.input_data_classes.is_empty()
            || self.input_data_classes.len() > MAX_PROCESSING_DATA_CLASSES
        {
            return Err(GrantValidationError::MissingScope);
        }
        let mut sorted_classes = self.input_data_classes.clone();
        sorted_classes.sort();
        if sorted_classes != self.input_data_classes {
            return Err(GrantValidationError::InvalidState);
        }
        super::ensure_unique(&self.input_data_classes, "data class")?;
        if self.source_scopes.len() > MAX_PROCESSING_SOURCE_SCOPES {
            return Err(GrantValidationError::TooLarge("source scopes"));
        }
        for scope in &self.source_scopes {
            scope.validate()?;
        }
        let mut sorted_scopes = self.source_scopes.clone();
        sorted_scopes.sort_by_cached_key(|scope| scope.canonical_bytes().unwrap_or_default());
        if sorted_scopes != self.source_scopes {
            return Err(GrantValidationError::InvalidState);
        }
        super::ensure_unique(
            &sorted_scopes
                .iter()
                .map(|scope| scope.canonical_bytes().unwrap_or_default())
                .collect::<Vec<_>>(),
            "source scope",
        )?;
        if self.projection_ref.is_nil() || self.projection_revision == 0 {
            return Err(GrantValidationError::InvalidState);
        }
        self.lineage.validate()?;
        let bytes =
            serde_json::to_vec(self).map_err(|_| GrantValidationError::InvalidState)?;
        if bytes.len() > MAX_PROCESSING_REQUIREMENT_BYTES {
            return Err(GrantValidationError::TooLarge("requirement"));
        }
        Ok(())
    }

    pub fn recipient(&self) -> &str {
        &self.recipient
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn consumer(&self) -> &str {
        &self.consumer
    }

    pub fn input_data_classes(&self) -> &[DataClass] {
        &self.input_data_classes
    }

    pub fn source_scopes(&self) -> &[ProcessingSourceScope] {
        &self.source_scopes
    }

    pub fn projection_ref(&self) -> Uuid {
        self.projection_ref
    }

    pub fn projection_revision(&self) -> u64 {
        self.projection_revision
    }

    pub fn lineage(&self) -> RecipientLineage {
        self.lineage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ConsumerPolicyAuthority, ContextDependency, ExecutionOwnerId, GrantAuthority, GrantId,
        GrantSourceBinding, SourceAuthority,
    };
    use chrono::{Duration, Utc};
    use floe_kernel::PersonId;

    fn lineage() -> RecipientLineage {
        RecipientLineage::try_new(Uuid::new_v4(), Uuid::new_v4()).unwrap()
    }

    fn dependency(person_id: PersonId) -> ContextDependency {
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("connection").unwrap(),
            ConnectorId::try_new("connector").unwrap(),
            ExecutionOwnerId::try_new("owner").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let now = Utc::now();
        ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("resource").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("assistant").unwrap(),
            crate::ProcessingRestriction::ApprovedRecipient {
                recipient: "model.example".into(),
                categories: vec![GrantDataCategory::Metadata],
            },
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"fingerprint".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now - Duration::minutes(1),
            now + Duration::minutes(5),
        )
        .unwrap()
    }

    fn requirement() -> ProcessingRequirement {
        ProcessingRequirement::try_new(
            "model.example",
            "server-model",
            "everyday_assistance",
            "conversation.root",
            vec![DataClass::Personal],
            vec![],
            Uuid::new_v4(),
            1,
            lineage(),
        )
        .unwrap()
    }

    #[test]
    fn independent_requirement_round_trips() {
        let requirement = requirement();
        assert!(requirement.validate().is_ok());
        assert!(requirement.source_scopes().is_empty());
        let decoded: ProcessingRequirement =
            serde_json::from_str(&serde_json::to_string(&requirement).unwrap()).unwrap();
        assert_eq!(decoded, requirement);
    }

    #[test]
    fn dependent_requirement_binds_review_relevant_scope() {
        let person_id = PersonId::new();
        let scope = ProcessingSourceScope::from_dependency(&dependency(person_id)).unwrap();
        assert_eq!(scope.resources().len(), 1);
        let requirement = ProcessingRequirement::try_new(
            "model.example",
            "server-model",
            "everyday_assistance",
            "conversation.root",
            vec![DataClass::Personal],
            vec![scope],
            Uuid::new_v4(),
            1,
            lineage(),
        )
        .unwrap();
        assert!(requirement.validate().is_ok());
        // Observation-specific values never enter the reviewed scope.
        let json = serde_json::to_string(&requirement).unwrap();
        assert!(!json.contains("observation"));
        assert!(!json.contains("fingerprint"));
        assert!(!json.contains("lease"));
    }

    #[test]
    fn serialized_requirement_contains_no_secret_or_payload() {
        let json = serde_json::to_string(&requirement()).unwrap();
        for forbidden in [
            "token",
            "bearer",
            "credential",
            "secret",
            "password",
            "endpoint",
            "prompt",
            "wildcard",
        ] {
            assert!(!json.contains(forbidden), "leaked {forbidden}");
        }
    }

    #[test]
    fn blank_wildcard_oversized_or_untrimmed_identity_is_rejected() {
        let valid = requirement();
        for (field, mut invalid) in [
            ("recipient", valid.clone()),
            ("profile", valid.clone()),
            ("purpose", valid.clone()),
            ("consumer", valid.clone()),
        ] {
            match field {
                "recipient" => invalid.recipient = String::new(),
                "profile" => invalid.profile_id = String::new(),
                "purpose" => invalid.purpose = String::new(),
                _ => invalid.consumer = String::new(),
            }
            assert!(invalid.validate().is_err(), "{field} blank");
        }
        let mut wildcard = valid.clone();
        wildcard.recipient = "*".into();
        assert!(wildcard.validate().is_err());
        let mut untrimmed = valid.clone();
        untrimmed.recipient = " model.example".into();
        assert!(untrimmed.validate().is_err());
        let mut oversized = valid.clone();
        oversized.profile_id = "x".repeat(MAX_PROFILE_ID_BYTES + 1);
        assert!(oversized.validate().is_err());
    }

    #[test]
    fn empty_duplicate_unsorted_or_oversized_members_are_rejected() {
        let valid = requirement();
        let mut empty_classes = valid.clone();
        empty_classes.input_data_classes = vec![];
        assert!(empty_classes.validate().is_err());
        let mut duplicate_classes = valid.clone();
        duplicate_classes.input_data_classes = vec![DataClass::Personal, DataClass::Personal];
        assert!(duplicate_classes.validate().is_err());
        let mut unsorted_classes = valid.clone();
        unsorted_classes.input_data_classes = vec![DataClass::Personal, DataClass::Synthetic];
        assert!(unsorted_classes.validate().is_err());
        let mut nil_projection = valid.clone();
        nil_projection.projection_ref = Uuid::nil();
        assert!(nil_projection.validate().is_err());
        let mut zero_revision = valid.clone();
        zero_revision.projection_revision = 0;
        assert!(zero_revision.validate().is_err());
        let mut nil_lineage = valid;
        nil_lineage.lineage = RecipientLineage {
            session_id: Uuid::nil(),
            origin_run_id: Uuid::new_v4(),
        };
        assert!(nil_lineage.validate().is_err());
    }
}
