//! Calendar read-access admission: which grant, authority and source stamp a
//! timeline read is allowed to rely on.
//!
//! Access judges admission only. Acquiring the observation and projecting it
//! belong to the source adapter and to Context; the stamp both sides quote is a
//! shared contract value.

use std::future::Future;

use floe_context_contract::{
    CALENDAR_CONTEXT_VIEW_ID, CalendarProvider, CalendarReadAccessStamp,
    ContextDependency, GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation,
    GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction, ResourceHandle,
    SourceAuthority, connection_view_resource,
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

    pub fn remote(
        person_id: PersonId,
        grant_id: GrantId,
        grant_authority: GrantAuthority,
        source: GrantSourceBinding,
        source_authority: SourceAuthority,
        scope: GrantScope,
        consumer: GrantConsumer,
        processing: ProcessingRestriction,
    ) -> Self {
        Self {
            person_id,
            grant_id,
            grant_authority,
            source,
            source_authority,
            scope,
            operation: GrantOperation::Read,
            purpose: GrantPurpose::Assistant,
            consumer,
            processing,
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

/// The grant side of a calendar read.
///
/// Whether this Person's grant still admits the read is Access's judgment; the
/// subject check and the observation itself are the source adapter's.
pub trait CalendarReadAdmission: Sync {
    fn admission(
        &self,
        _: &CalendarReadAccessRequest,
    ) -> impl Future<Output = Result<Option<CalendarReadAccessAdmission>, AgentFailure>> + Send
    {
        async { Ok(None) }
    }

    fn admission_after_check(
        &self,
        request: &CalendarReadAccessRequest,
        _: &CalendarReadAccessStamp,
    ) -> impl Future<Output = Result<Option<CalendarReadAccessAdmission>, AgentFailure>> + Send
    {
        self.admission(request)
    }
}

/// That a calendar read request stays inside the grant it runs under.
///
/// A read for another Person, another device, another provider, or a calendar
/// the grant does not name is not this grant's read, whatever admitted it.
pub fn admits_calendar_read_request(
    request: &CalendarReadAccessRequest,
    person_id: PersonId,
    provider: CalendarProvider,
    device_id: &str,
    calendar_ids: &[String],
) -> Result<(), AgentFailure> {
    if request.person_id != person_id
        || request.provider != provider
        || request.device_id != device_id
        || request
            .calendar_ids
            .iter()
            .any(|calendar_id| !calendar_ids.contains(calendar_id))
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    Ok(())
}

/// That an admission is one the grant this read runs under may stand on.
///
/// The admission has to be this Person's, on the connector their source is
/// reached through, and narrowed to calendars the grant names.
pub fn admits_calendar_read(
    admission: &CalendarReadAccessAdmission,
    person_id: PersonId,
    connector: &str,
    calendar_ids: &[String],
) -> Result<(), AgentFailure> {
    if admission.person_id() != person_id
        || admission.source().connector().as_str() != connector
        || admission.scope().resources().iter().any(|resource| {
            !calendar_ids
                .iter()
                .any(|calendar_id| calendar_id == resource.as_str())
        })
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    Ok(())
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
    if admission.person_id != person_id
        || admission.source.person_id() != person_id
        || admission.source.connection_id().as_str() != connection_id
        || admission.source.connector().as_str() != connector
        || admission.source.execution_owner().as_str() != device_id
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
pub fn admission_matches(
    admission: &CalendarReadAccessAdmission,
    view: &impl floe_context_contract::HeldGrant,
) -> bool {
    let [binding] = view.bindings() else {
        return false;
    };
    admission.scope == binding.scope && admission_matches_dependency(admission, &binding.dependency)
}

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
pub async fn current_native_calendar_grant(
    repository: &(impl crate::GrantRepository + ?Sized),
    person_id: PersonId,
    connection_id: &str,
    provider: CalendarProvider,
    device_id: &str,
    consumer: &GrantConsumer,
) -> Result<crate::DataAccessGrant, AgentFailure> {
    let connector = super::native_calendar::native_calendar_connector(provider).ok_or(AgentFailure::CapabilityUnavailable)?;
    let source = GrantSourceBinding::try_new(person_id,
        floe_context_contract::ConnectionId::try_new(connection_id).map_err(|_| AgentFailure::InvalidInput)?,
        floe_context_contract::ConnectorId::try_new(connector).map_err(|_| AgentFailure::InvalidInput)?,
        floe_context_contract::ExecutionOwnerId::try_new(device_id).map_err(|_| AgentFailure::InvalidInput)?)
        .map_err(|_| AgentFailure::InvalidInput)?;
    let snapshot = repository.snapshot(source.clone()).await?;
    let resource = native_calendar_resource(connection_id)?;
    let matching = snapshot.grants.into_iter().filter(|grant| grant.state() != crate::GrantState::Revoked
        && grant.scope().resources().contains(&resource)).collect::<Vec<_>>();
    let [grant] = matching.as_slice() else { return Err(AgentFailure::AccessReviewRequired); };
    if grant.state() != crate::GrantState::Active || grant.review_required() || grant.authority_owner() != snapshot.authority_owner
        || grant.source() != &source || grant.scope().resources() != [resource]
        || grant.scope().categories() != [GrantDataCategory::Metadata, GrantDataCategory::Content]
        || grant.scope().operations() != [GrantOperation::Read] || !grant.scope().purposes().contains(&GrantPurpose::Assistant)
        || !grant.scope().consumers().contains(consumer) { return Err(AgentFailure::AccessReviewRequired); }
    Ok(grant.clone())
}
