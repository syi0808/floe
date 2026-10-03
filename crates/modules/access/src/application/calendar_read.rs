//! Calendar read-access admission: which grant, authority and source stamp a
//! timeline read is allowed to rely on.
//!
//! Access judges admission only. Acquiring the observation and projecting it
//! belong to the source adapter and to Context; the stamp both sides quote is a
//! shared contract value.

use floe_context_contract::{
    CALENDAR_CONTEXT_VIEW_ID, CalendarProvider, CalendarReadAccessStamp, ContextDependency,
    GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
    GrantScope, GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    connection_view_resource,
};
use floe_execution::Cancellation;
use floe_kernel::AgentFailure;
use floe_kernel::PersonId;
use tokio::time::Instant;

#[derive(Clone)]
pub struct CalendarReadAccessRequest {
    pub person_id: PersonId,
    pub device_id: String,
    pub provider: CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub expected_native_subject_fingerprint: Option<String>,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarReadAccessAdmission {
    pub(crate) person_id: PersonId,
    pub(crate) grant_id: GrantId,
    pub(crate) grant_authority: GrantAuthority,
    pub(crate) source: GrantSourceBinding,
    pub(crate) source_authority: SourceAuthority,
    pub(crate) scope: GrantScope,
    pub(crate) operation: GrantOperation,
    pub(crate) purpose: GrantPurpose,
    pub(crate) consumer: GrantConsumer,
    pub(crate) processing: ProcessingRestriction,
}

impl CalendarReadAccessAdmission {
    /// Native acquisition uses the Person's exact read grant and preserves its
    /// processing policy for later model projection and dispatch.
    #[allow(clippy::too_many_arguments)]
    pub fn device_local(
        person_id: PersonId,
        grant_id: GrantId,
        grant_authority: GrantAuthority,
        source: GrantSourceBinding,
        source_authority: SourceAuthority,
        scope: GrantScope,
        consumer: GrantConsumer,
    ) -> Self {
        Self {
            person_id,
            grant_id,
            grant_authority,
            source,
            source_authority,
            processing: scope.processing().clone(),
            scope,
            operation: GrantOperation::Read,
            purpose: GrantPurpose::Assistant,
            consumer,
        }
    }

    pub fn person_id(&self) -> PersonId {
        self.person_id
    }

    pub fn grant_id(&self) -> GrantId {
        self.grant_id
    }

    pub fn grant_authority(&self) -> GrantAuthority {
        self.grant_authority
    }

    pub fn source(&self) -> &GrantSourceBinding {
        &self.source
    }

    pub fn source_authority(&self) -> SourceAuthority {
        self.source_authority
    }

    pub fn processing(&self) -> &ProcessingRestriction {
        &self.processing
    }

    pub fn scope(&self) -> &GrantScope {
        &self.scope
    }
}

pub fn admits_native_calendar_read(
    admission: &CalendarReadAccessAdmission,
    person_id: PersonId,
    connection_id: &str,
    provider: CalendarProvider,
    device_id: &str,
    source_authority: SourceAuthority,
    consumer: &GrantConsumer,
) -> Result<(), AgentFailure> {
    let connector = super::native_calendar::native_calendar_connector(provider)
        .ok_or(AgentFailure::CapabilityUnavailable)?;
    let logical_resource = native_calendar_resource(connection_id)?;
    let execution_owner = if provider == CalendarProvider::EventKit {
        crate::apple_execution_owner(device_id)
    } else {
        device_id.to_owned()
    };
    if admission.person_id != person_id
        || admission.source.person_id() != person_id
        || admission.source.connection_id().as_str() != connection_id
        || admission.source.connector().as_str() != connector
        || admission.source.execution_owner().as_str() != execution_owner
        || admission.source_authority != source_authority
        || admission.scope.resources() != [logical_resource]
        || admission.scope.categories().len() != 2
        || !admission
            .scope
            .categories()
            .contains(&GrantDataCategory::Metadata)
        || !admission
            .scope
            .categories()
            .contains(&GrantDataCategory::Content)
        || admission.scope.operations() != [GrantOperation::Read]
        || admission.scope.purposes() != [GrantPurpose::Assistant]
        || !admission.scope.consumers().contains(consumer)
        || admission.operation != GrantOperation::Read
        || admission.purpose != GrantPurpose::Assistant
        || admission.consumer != *consumer
        || admission.processing != *admission.scope.processing()
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    Ok(())
}

pub fn native_calendar_resource(connection_id: &str) -> Result<ResourceHandle, AgentFailure> {
    connection_view_resource(
        CALENDAR_CONTEXT_VIEW_ID,
        &floe_context_contract::ConnectionId::try_new(connection_id)
            .map_err(|_| AgentFailure::InvalidInput)?,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

/// Whether an admission still covers the source view that was assembled from it.
///
/// Access does not know which Expert's view this is; it only checks that the
/// scope and the dependency still match what was admitted.
pub fn admission_matches_dependency(
    admission: &CalendarReadAccessAdmission,
    dependency: &ContextDependency,
) -> bool {
    admission.person_id == dependency.person_id()
        && admission.grant_id == dependency.grant_id()
        && admission.grant_authority == dependency.grant_authority()
        && admission.source == dependency.source().clone()
        && admission.source_authority == dependency.source_authority()
        && admission.scope.resources() == dependency.resources()
        && !dependency.source_resources().is_empty()
        && admission.scope.categories() == dependency.categories()
        && admission.operation == dependency.operation()
        && admission.purpose == dependency.purpose()
        && admission.consumer == dependency.consumer().clone()
        && admission.processing == dependency.processing().clone()
}

/// Resolve the exact current native Calendar grant through the Access repository.
/// Source lifecycle and native subject are independently checked by Connections
/// and the acquisition host before and after the actual read.
/// `execution_owner` is the complete source identity, not the admitted device ID.
pub async fn current_native_calendar_grant(
    repository: &(impl crate::GrantRepository + ?Sized),
    person_id: PersonId,
    connection_id: &str,
    provider: CalendarProvider,
    execution_owner: &floe_context_contract::ExecutionOwnerId,
    consumer: &GrantConsumer,
) -> Result<crate::DataAccessGrant, AgentFailure> {
    let connector = super::native_calendar::native_calendar_connector(provider)
        .ok_or(AgentFailure::CapabilityUnavailable)?;
    let source = GrantSourceBinding::try_new(
        person_id,
        floe_context_contract::ConnectionId::try_new(connection_id)
            .map_err(|_| AgentFailure::InvalidInput)?,
        floe_context_contract::ConnectorId::try_new(connector)
            .map_err(|_| AgentFailure::InvalidInput)?,
        execution_owner.clone(),
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    let snapshot = repository.snapshot(source.clone()).await?;
    let resource = native_calendar_resource(connection_id)?;
    let matching = snapshot
        .grants
        .into_iter()
        .filter(|grant| {
            grant.state() != crate::GrantState::Revoked
                && grant.scope().resources().contains(&resource)
        })
        .collect::<Vec<_>>();
    let [grant] = matching.as_slice() else {
        return Err(AgentFailure::AccessReviewRequired);
    };
    if grant.state() != crate::GrantState::Active
        || grant.review_required()
        || grant.authority_owner() != snapshot.authority_owner
        || grant.source() != &source
        || grant.scope().resources() != [resource]
        || grant.scope().categories() != [GrantDataCategory::Metadata, GrantDataCategory::Content]
        || grant.scope().operations() != [GrantOperation::Read]
        || !grant.scope().purposes().contains(&GrantPurpose::Assistant)
        || !grant.scope().consumers().contains(consumer)
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(grant.clone())
}
