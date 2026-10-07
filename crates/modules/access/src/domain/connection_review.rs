use crate::{DataAccessGrant, GrantState, VerifiedGatewayBinding};
use chrono::{DateTime, Utc};
use floe_context_contract::{
    DataClass, GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantPurpose,
    GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
};
use floe_kernel::{AgentFailure, PersonId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRef {
    pub id: Uuid,
    pub revision: u64,
    pub digest: [u8; 32],
}
impl ReviewRef {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.id.is_nil()
            || self.revision == 0
            || self.revision > i64::MAX as u64
            || self.digest == [0; 32]
        {
            Err(AgentFailure::InvalidInput)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceExpectation {
    pub source: GrantSourceBinding,
    /// None is proven absence for reviewed native creation, never an unchecked revision.
    pub revision: Option<u64>,
    pub provider_revision: Option<u64>,
    pub authority: SourceAuthority,
    pub physical_resources: Vec<ResourceHandle>,
    pub subject_fingerprint: String,
    pub gateway: Option<VerifiedGatewayBinding>,
}
impl SourceExpectation {
    pub fn validate_device(&self, device_id: &str) -> Result<(), AgentFailure> {
        validate_source_device(&self.source, device_id)?;
        if self
            .gateway
            .as_ref()
            .is_some_and(|binding| binding.device_id != device_id)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.source
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if self.revision == Some(0)
            || !self.authority.is_valid()
            || self.physical_resources.is_empty()
            || self.physical_resources.len() > floe_context_contract::MAX_SOURCE_RESOURCES
            || self
                .physical_resources
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self.subject_fingerprint.is_empty()
            || self.subject_fingerprint.len() > 256
            || self.subject_fingerprint.chars().any(char::is_control)
        {
            return Err(AgentFailure::InvalidInput);
        }
        if let Some(binding) = &self.gateway {
            if self.provider_revision.is_none_or(|revision| revision == 0) {
                return Err(AgentFailure::InvalidInput);
            }
            binding.validate()?;
            if binding.person_id != self.source.person_id().to_string() {
                return Err(AgentFailure::PolicyDenied);
            }
        } else if self.provider_revision.is_some()
            || !(crate::local_calendar_provider(self.source.connector().as_str()).is_some()
                || matches!(
                    self.source.connector().as_str(),
                    "contacts.apple"
                        | "contacts.android"
                        | "attention.macos"
                        | "health.apple"
                ))
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpectedGrant {
    Absent {
        resource: ResourceHandle,
    },
    Present {
        grant_id: GrantId,
        authority: GrantAuthority,
        resource: ResourceHandle,
    },
}
impl ExpectedGrant {
    pub fn resource(&self) -> &ResourceHandle {
        match self {
            Self::Absent { resource } | Self::Present { resource, .. } => resource,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedView {
    pub view_id: String,
    pub data_class: DataClass,
    pub expected: ExpectedGrant,
    pub consumers: Vec<GrantConsumer>,
    pub purpose: GrantPurpose,
    pub categories: Vec<GrantDataCategory>,
    pub current_processing: Option<ProcessingRestriction>,
    pub requested_processing: ProcessingRestriction,
    pub successor: DataAccessGrant,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionReview {
    pub reference: ReviewRef,
    pub command_id: Uuid,
    pub intent_digest: [u8; 32],
    pub person_id: PersonId,
    pub device_id: String,
    pub source: SourceExpectation,
    pub expires_at: DateTime<Utc>,
    pub views: Vec<ReviewedView>,
    pub policy_digest: [u8; 32],
}
impl ConnectionReview {
    pub fn digest(&self) -> Result<[u8; 32], AgentFailure> {
        digest(&(
            &self.reference.id,
            self.reference.revision,
            self.command_id,
            self.intent_digest,
            self.person_id,
            &self.device_id,
            &self.source,
            self.expires_at,
            &self.views,
            self.policy_digest,
        ))
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.reference.validate()?;
        self.source.validate()?;
        self.source.validate_device(&self.device_id)?;
        if self.command_id.is_nil()
            || self.intent_digest == [0; 32]
            || !self.person_id.is_valid()
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.device_id.chars().any(char::is_control)
            || self.source.source.person_id() != self.person_id
            || self.views.is_empty()
            || self.views.len() > 64
            || self.policy_digest == [0; 32]
            || self.digest()? != self.reference.digest
        {
            return Err(AgentFailure::InvalidInput);
        }
        for (index, view) in self.views.iter().enumerate() {
            view.successor
                .validate()
                .map_err(|_| AgentFailure::InvalidInput)?;
            if matches!(view.expected, ExpectedGrant::Absent { .. })
                != view.current_processing.is_none()
            {
                return Err(AgentFailure::InvalidInput);
            }
            if let Some(ProcessingRestriction::GatewayAllowed { categories }) =
                &view.current_processing
            {
                if ProcessingRestriction::gateway_allowed(categories.clone())
                    .map_err(|_| AgentFailure::InvalidInput)?
                    != *view
                        .current_processing
                        .as_ref()
                        .ok_or(AgentFailure::InvalidInput)?
                {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            if !crate::source_view_ids(self.source.source.connector().as_str())
                .contains(&view.view_id.as_str())
                || floe_context_contract::source_view_data_class(&view.view_id)
                    != Some(view.data_class)
                || floe_context_contract::connection_view_resource(
                    &view.view_id,
                    &self.source.source.connection_id(),
                )
                .map_err(|_| AgentFailure::InvalidInput)?
                    != *view.expected.resource()
                || view.successor.source() != &self.source.source
                || view.successor.state() != GrantState::Active
                || view.successor.review_required()
                || view.successor.scope().resources() != [view.expected.resource().clone()]
                || view.successor.scope().consumers() != view.consumers
                || view.successor.scope().purposes() != [view.purpose]
                || view.successor.scope().categories() != view.categories
                || view.successor.scope().processing() != &view.requested_processing
                || self.views[..index]
                    .iter()
                    .any(|other| other.expected.resource() == view.expected.resource())
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceReservationEvidence {
    pub operation_id: Uuid,
    pub command_id: Uuid,
    pub device_id: String,
    pub request_digest: [u8; 32],
    pub reservation_id: Uuid,
    pub reservation_generation: u64,
    pub source: SourceExpectation,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrantOperationIdentity {
    pub operation_id: Uuid,
    pub command_id: Uuid,
    pub device_id: String,
    pub request_digest: [u8; 32],
    pub reservation_id: Uuid,
    pub reservation_generation: u64,
    pub source: GrantSourceBinding,
    pub source_revision: Option<u64>,
    pub source_authority: SourceAuthority,
}
impl GrantOperationIdentity {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.source
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        validate_source_device(&self.source, &self.device_id)?;
        if self.operation_id.is_nil()
            || self.command_id.is_nil()
            || self.reservation_id.is_nil()
            || self.reservation_generation == 0
            || self.request_digest == [0; 32]
            || self.source_revision == Some(0)
            || !self.source_authority.is_valid()
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}
impl SourceReservationEvidence {
    pub fn identity(&self) -> GrantOperationIdentity {
        GrantOperationIdentity {
            operation_id: self.operation_id,
            command_id: self.command_id,
            device_id: self.device_id.clone(),
            request_digest: self.request_digest,
            reservation_id: self.reservation_id,
            reservation_generation: self.reservation_generation,
            source: self.source.source.clone(),
            source_revision: self.source.revision,
            source_authority: self.source.authority,
        }
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.source.validate()?;
        self.source.validate_device(&self.device_id)?;
        self.identity().validate()?;
        if self.operation_id.is_nil()
            || self.command_id.is_nil()
            || self.reservation_id.is_nil()
            || self.reservation_generation == 0
            || self.request_digest == [0; 32]
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

fn validate_source_device(
    source: &GrantSourceBinding,
    device_id: &str,
) -> Result<(), AgentFailure> {
    if device_id.is_empty()
        || device_id.len() > 256
        || device_id.trim() != device_id
        || device_id.chars().any(char::is_control)
    {
        return Err(AgentFailure::InvalidInput);
    }
    if let Some(expected) = crate::local_calendar_execution_owner_for_connector(
        source.connector().as_str(),
        device_id,
    ) {
        if source.execution_owner().as_str() != expected {
            return Err(AgentFailure::PolicyDenied);
        }
    } else if crate::is_device_local_source(source.connector().as_str())
        && source.execution_owner().as_str() != device_id
        && source.execution_owner().as_str() != crate::apple_execution_owner(device_id)
        && !(source.connector().as_str() == "attention.macos"
            && source.execution_owner().as_str() == format!("macos:{device_id}"))
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantCommitKind {
    Reviewed { review: ReviewRef },
    PauseObserve,
    InvalidateSource,
    Disconnect,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrantMutation {
    pub expected: ExpectedGrant,
    pub successor: DataAccessGrant,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrantCommit {
    pub reservation: SourceReservationEvidence,
    pub kind: GrantCommitKind,
    pub mutations: Vec<GrantMutation>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrantResult {
    pub grant_id: GrantId,
    pub authority: GrantAuthority,
    pub state: GrantState,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrantCommitReceipt {
    pub reservation: SourceReservationEvidence,
    pub kind: GrantCommitKind,
    pub commit_id: Uuid,
    pub grants: Vec<GrantResult>,
    pub cleanup_ids: Vec<String>,
    pub commit_digest: [u8; 32],
}
impl GrantCommitReceipt {
    pub fn digest(&self) -> Result<[u8; 32], AgentFailure> {
        digest(self)
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.reservation.validate()?;
        if self.commit_id.is_nil()
            || self.commit_digest == [0; 32]
            || self.grants.len() > 128
            || self.cleanup_ids.len() > 128
            || self.grants.iter().enumerate().any(|(index, grant)| {
                !grant.grant_id.is_valid()
                    || !grant.authority.is_valid()
                    || self.grants[..index]
                        .iter()
                        .any(|other| other.grant_id == grant.grant_id)
            })
            || self
                .cleanup_ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 256)
        {
            return Err(AgentFailure::InvalidInput);
        }
        if let GrantCommitKind::Reviewed { review } = &self.kind {
            review.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrantAbortReceipt {
    pub identity: GrantOperationIdentity,
    pub abort_id: Uuid,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantOperationReceipt {
    Committed(GrantCommitReceipt),
    Aborted(GrantAbortReceipt),
}
impl GrantOperationReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        match self {
            Self::Committed(receipt) => receipt.validate(),
            Self::Aborted(receipt) => {
                receipt.identity.validate()?;
                if receipt.abort_id.is_nil() {
                    Err(AgentFailure::InvalidInput)
                } else {
                    Ok(())
                }
            }
        }
    }
}
#[derive(Clone, Debug)]
pub enum GrantAbortOutcome {
    Aborted(GrantAbortReceipt),
    AlreadyCommitted(GrantCommitReceipt),
}
#[derive(Clone, Debug)]
pub struct GrantReceiptQuery {
    pub identity: GrantOperationIdentity,
}
#[derive(Clone, Debug)]
pub struct GrantAbort {
    pub identity: GrantOperationIdentity,
}
#[derive(Clone, Debug)]
pub struct GrantSnapshot {
    pub authority_owner: Uuid,
    pub source: GrantSourceBinding,
    pub grants: Vec<DataAccessGrant>,
}

/// Shared expectation predicate for fresh review admission and transactional commit.
pub fn review_grants_current(review: &ConnectionReview, snapshot: &GrantSnapshot) -> bool {
    review.source.source == snapshot.source
        && review.views.iter().all(|view| {
            if view.successor.authority_owner() != snapshot.authority_owner {
                return false;
            }
            let matching = snapshot
                .grants
                .iter()
                .filter(|grant| {
                    grant.state() != GrantState::Revoked
                        && grant.scope().resources().contains(view.expected.resource())
                })
                .collect::<Vec<_>>();
            match (&view.expected, matching.as_slice()) {
                (ExpectedGrant::Absent { .. }, []) => !snapshot
                    .grants
                    .iter()
                    .any(|grant| grant.id() == view.successor.id()),
                (
                    ExpectedGrant::Present {
                        grant_id,
                        authority,
                        ..
                    },
                    [current],
                ) => {
                    current.id() == *grant_id
                        && current.authority() == *authority
                        && view.current_processing.as_ref() == Some(current.scope().processing())
                }
                _ => false,
            }
        })
}

pub fn validate_commit(
    command: &GrantCommit,
    review: Option<&ConnectionReview>,
    snapshot: &GrantSnapshot,
    now: DateTime<Utc>,
) -> Result<(), AgentFailure> {
    command.reservation.validate()?;
    if command.reservation.source.source != snapshot.source || command.mutations.len() > 128 {
        return Err(AgentFailure::Conflict);
    }
    match (&command.kind, review) {
        (GrantCommitKind::Reviewed { review: expected }, Some(review)) => {
            review.validate()?;
            if expected != &review.reference
                || review.expires_at <= now
                || review.source != command.reservation.source
                || command.mutations
                    != review
                        .views
                        .iter()
                        .map(|view| GrantMutation {
                            expected: view.expected.clone(),
                            successor: view.successor.clone(),
                        })
                        .collect::<Vec<_>>()
            {
                return Err(AgentFailure::Conflict);
            }
            if !review_grants_current(review, snapshot) {
                return Err(AgentFailure::Conflict);
            }
        }
        (GrantCommitKind::PauseObserve, None) => {
            let active = snapshot
                .grants
                .iter()
                .filter(|grant| grant.state() == GrantState::Active)
                .collect::<Vec<_>>();
            if command.mutations.len() != active.len()
                || command
                    .mutations
                    .iter()
                    .any(|mutation| mutation.successor.state() != GrantState::Paused)
                || active.iter().any(|grant| {
                    !command
                        .mutations
                        .iter()
                        .any(|mutation| mutation.successor.id() == grant.id())
                })
            {
                return Err(AgentFailure::Conflict);
            }
        }
        (GrantCommitKind::InvalidateSource, None) => {
            let live = snapshot
                .grants
                .iter()
                .filter(|grant| grant.state() != GrantState::Revoked)
                .collect::<Vec<_>>();
            if command.mutations.len() != live.len()
                || command.mutations.iter().any(|mutation| {
                    mutation.successor.state() != GrantState::Paused
                        || !mutation.successor.review_required()
                })
                || live.iter().any(|grant| {
                    !command
                        .mutations
                        .iter()
                        .any(|mutation| mutation.successor.id() == grant.id())
                })
            {
                return Err(AgentFailure::Conflict);
            }
        }
        (GrantCommitKind::Disconnect, None) => {
            let live = snapshot
                .grants
                .iter()
                .filter(|grant| grant.state() != GrantState::Revoked)
                .collect::<Vec<_>>();
            if command.mutations.len() != live.len()
                || command
                    .mutations
                    .iter()
                    .any(|mutation| mutation.successor.state() != GrantState::Revoked)
                || live.iter().any(|grant| {
                    !command
                        .mutations
                        .iter()
                        .any(|mutation| mutation.successor.id() == grant.id())
                })
            {
                return Err(AgentFailure::Conflict);
            }
        }
        _ => return Err(AgentFailure::Conflict),
    }
    for (index, mutation) in command.mutations.iter().enumerate() {
        mutation
            .successor
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if mutation.successor.source() != &snapshot.source
            || mutation.successor.authority_owner() != snapshot.authority_owner
            || command.mutations[..index]
                .iter()
                .any(|other| other.successor.id() == mutation.successor.id())
        {
            return Err(AgentFailure::Conflict);
        }
        let matching = snapshot
            .grants
            .iter()
            .filter(|grant| {
                grant.state() != GrantState::Revoked
                    && grant
                        .scope()
                        .resources()
                        .contains(mutation.expected.resource())
            })
            .collect::<Vec<_>>();
        match (&mutation.expected, matching.as_slice()) {
            (ExpectedGrant::Absent { .. }, []) => {
                if snapshot
                    .grants
                    .iter()
                    .any(|grant| grant.id() == mutation.successor.id())
                {
                    return Err(AgentFailure::Conflict);
                }
            }
            (
                ExpectedGrant::Present {
                    grant_id,
                    authority,
                    ..
                },
                [current],
            ) if current.id() == *grant_id && current.authority() == *authority => {
                let mut rebuilt = (**current).clone();
                match &command.kind {
                    GrantCommitKind::Disconnect => {
                        rebuilt
                            .revoke(*authority)
                            .map_err(|_| AgentFailure::Conflict)?;
                    }
                    GrantCommitKind::PauseObserve => {
                        rebuilt
                            .pause(*authority)
                            .map_err(|_| AgentFailure::Conflict)?;
                    }
                    GrantCommitKind::InvalidateSource => {
                        rebuilt
                            .invalidate_source(*authority)
                            .map_err(|_| AgentFailure::Conflict)?;
                    }
                    GrantCommitKind::Reviewed { .. } if rebuilt.state() == GrantState::Active => {
                        rebuilt
                            .review_active(*authority, mutation.successor.scope().clone())
                            .map_err(|_| AgentFailure::Conflict)?;
                    }
                    GrantCommitKind::Reviewed { .. } => {
                        rebuilt
                            .activate_review(*authority, mutation.successor.scope().clone())
                            .map_err(|_| AgentFailure::Conflict)?;
                    }
                }
                if rebuilt != mutation.successor {
                    return Err(AgentFailure::Conflict);
                }
            }
            _ => return Err(AgentFailure::Conflict),
        }
    }
    Ok(())
}
pub fn digest(value: &impl Serialize) -> Result<[u8; 32], AgentFailure> {
    Ok(Sha256::digest(serde_json::to_vec(value).map_err(|_| AgentFailure::InvalidInput)?).into())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceProcessingChoice {
    DeviceOnly,
    GatewayAllowed,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceObserveStatus {
    Enabled,
    Paused,
    ReviewRequired,
    Disabled,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceObserveViewState {
    Absent,
    Active,
    Paused,
    ReviewRequired,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceObserveView {
    pub view_id: String,
    pub data_class: DataClass,
    pub categories: Vec<GrantDataCategory>,
    pub state: SourceObserveViewState,
    pub processing: Option<ProcessingRestriction>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceObserveState {
    pub status: SourceObserveStatus,
    pub views: Vec<SourceObserveView>,
}

/// Durable projection origin. Identity and opaque refs are derived by Access;
/// replay lookup precedes every new source observation or policy calculation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionReviewOrigin {
    pub run_id: floe_kernel::RunId,
    pub projection_operation_id: Uuid,
    pub target_digest: [u8; 32],
    pub connection_id: floe_context_contract::ConnectionId,
    pub requirements_digest: [u8; 32],
}
impl ProjectionReviewOrigin {
    pub fn validate_review_binding(
        &self,
        person_id: PersonId,
        device_id: &str,
        review: &ConnectionReview,
    ) -> Result<(), AgentFailure> {
        self.validate()?;
        review.validate()?;
        review.source.validate_device(device_id)?;
        if !person_id.is_valid()
            || review.person_id != person_id
            || review.device_id != device_id
            || review.source.source.connection_id() != self.connection_id
            || review.command_id != self.command_id()?
            || review.intent_digest != digest(&(person_id, device_id, self))?
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
    pub fn for_requirements(
        run_id: floe_kernel::RunId,
        projection_operation_id: Uuid,
        target_digest: [u8; 32],
        connection_id: floe_context_contract::ConnectionId,
        requirements: &[floe_context_contract::SourceAccessRequirement],
    ) -> Result<Self, AgentFailure> {
        if requirements.is_empty()
            || requirements.len() > floe_context_contract::MAX_SOURCE_ACCESS_BLOCKERS
        {
            return Err(AgentFailure::InvalidInput);
        }
        let mut bytes = Vec::with_capacity(requirements.len());
        for requirement in requirements {
            requirement
                .validate()
                .map_err(|_| AgentFailure::InvalidInput)?;
            if requirement.connection_id() != Some(&connection_id) {
                return Err(AgentFailure::Conflict);
            }
            bytes.push(serde_json::to_vec(requirement).map_err(|_| AgentFailure::InvalidInput)?);
        }
        bytes.sort();
        bytes.dedup();
        let origin = Self {
            run_id,
            projection_operation_id,
            target_digest,
            connection_id,
            requirements_digest: digest(&bytes)?,
        };
        origin.validate()?;
        Ok(origin)
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.run_id.is_valid()
            || self.projection_operation_id.is_nil()
            || self.target_digest == [0; 32]
            || self.requirements_digest == [0; 32]
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
    pub(crate) fn command_id(&self) -> Result<Uuid, AgentFailure> {
        let hash = digest(&(
            "floe.access.projection-review.v1",
            self.run_id,
            self.projection_operation_id,
            self.target_digest,
            &self.connection_id,
        ))?;
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&hash[..16]);
        bytes[6] = (bytes[6] & 15) | 64;
        bytes[8] = (bytes[8] & 63) | 128;
        Ok(Uuid::from_bytes(bytes))
    }
    pub(crate) fn intent_digest(
        &self,
        actor: &floe_kernel::OwnerActor,
    ) -> Result<[u8; 32], AgentFailure> {
        digest(&(actor.person_id, &actor.device_id, self))
    }
}

/// Applicability of a fresh decision, distinct from immutable receipt replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewInapplicability {
    Expired,
    PolicyChanged,
    GrantChanged,
}
