//! Calendar read-access admission: which grant, authority and source stamp a
//! timeline read is allowed to rely on.
//!
//! Access judges admission only. Acquiring the observation and projecting it
//! belong to the source adapter and to Context; the stamp both sides quote is a
//! shared contract value.

use std::future::Future;

use floe_context_contract::{
    CalendarProvider, CalendarReadAccessStamp, ConsumerPolicyAuthority, ContextDependency,
    GrantAuthority, GrantConsumer, GrantId, GrantOperation, GrantPurpose, GrantScope,
    GrantSourceBinding, ProcessingRestriction,
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
    pub(crate) scope: GrantScope,
    pub(crate) consumer_policy: ConsumerPolicyAuthority,
    pub(crate) operation: GrantOperation,
    pub(crate) purpose: GrantPurpose,
    pub(crate) consumer: GrantConsumer,
    pub(crate) processing: ProcessingRestriction,
}

impl CalendarReadAccessAdmission {
    /// The admission a device-local read stands under: the Person's own grant,
    /// read for the assistant, and never processed anywhere but this device.
    #[allow(clippy::too_many_arguments)]
    pub fn device_local(
        person_id: PersonId,
        grant_id: GrantId,
        grant_authority: GrantAuthority,
        source: GrantSourceBinding,
        scope: GrantScope,
        consumer_policy: ConsumerPolicyAuthority,
        consumer: GrantConsumer,
    ) -> Self {
        Self {
            person_id,
            grant_id,
            grant_authority,
            source,
            scope,
            consumer_policy,
            operation: GrantOperation::Read,
            purpose: GrantPurpose::Assistant,
            consumer,
            processing: ProcessingRestriction::LocalOnly,
        }
    }

    pub fn remote(
        person_id: PersonId,
        grant_id: GrantId,
        grant_authority: GrantAuthority,
        source: GrantSourceBinding,
        scope: GrantScope,
        consumer_policy: ConsumerPolicyAuthority,
        consumer: GrantConsumer,
        processing: ProcessingRestriction,
    ) -> Self {
        Self {
            person_id,
            grant_id,
            grant_authority,
            source,
            scope,
            consumer_policy,
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

    pub fn source(&self) -> &GrantSourceBinding {
        &self.source
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

/// That where this run's model runs is where the grant said its contents may be
/// processed.
///
/// A grant the Person confined to their device may not be read into a model
/// somewhere else, and one approved for a named recipient is not what a
/// device-local read stands on.
pub fn admits_processing(
    processing: &ProcessingRestriction,
    remote_processing: bool,
) -> Result<(), AgentFailure> {
    match processing {
        ProcessingRestriction::LocalOnly if remote_processing => Err(AgentFailure::PolicyDenied),
        ProcessingRestriction::ApprovedRecipient { .. } if !remote_processing => {
            Err(AgentFailure::PolicyDenied)
        }
        ProcessingRestriction::LocalOnly | ProcessingRestriction::ApprovedRecipient { .. } => {
            Ok(())
        }
    }
}

/// Whether an admission still covers the source view that was assembled from it.
///
/// Access does not know which Expert's view this is; it only checks that the
/// scope and the dependency still match what was admitted.
pub fn admission_matches(
    admission: &CalendarReadAccessAdmission,
    view: &impl floe_context_contract::HeldGrant,
) -> bool {
    admission.scope == *view.scope() && admission_matches_dependency(admission, view.dependency())
}

pub fn admission_matches_dependency(
    admission: &CalendarReadAccessAdmission,
    dependency: &ContextDependency,
) -> bool {
    admission.person_id == dependency.person_id()
        && admission.grant_id == dependency.grant_id()
        && admission.grant_authority == dependency.grant_authority()
        && admission.source == dependency.source().clone()
        && admission.scope.resources() == dependency.resources()
        && admission.scope.categories() == dependency.categories()
        && admission.consumer_policy == dependency.consumer_policy()
        && admission.operation == dependency.operation()
        && admission.purpose == dependency.purpose()
        && admission.consumer == dependency.consumer().clone()
        && admission.processing == dependency.processing().clone()
}
