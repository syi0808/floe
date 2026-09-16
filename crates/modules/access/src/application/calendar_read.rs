//! Calendar read-access admission: which grant, authority and source stamp a
//! timeline read is allowed to rely on, and the observation shapes it returns.

use std::future::Future;

use chrono::{DateTime, Utc};
use floe_agent_contract::AgentFailure;
use floe_context_contract::{
    ConsumerPolicyAuthority, ContextDependency, GrantAuthority, GrantConsumer, GrantId,
    GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction,
};
use floe_day::{CalendarBatch, CalendarProvider};
use floe_execution::Cancellation;
use floe_kernel::PersonId;
use serde::{Deserialize, Serialize};
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarReadAccessStamp {
    pub schema_version: u32,
    pub person_id: PersonId,
    pub device_id: String,
    pub provider: CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub native_subject_fingerprint: String,
    pub generation: String,
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
}

pub trait CalendarReadAccess: Sync {
    fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> impl Future<Output = Result<CalendarReadAccessStamp, AgentFailure>> + Send;

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

    fn observe(
        &self,
        _: CalendarObserveRequest,
    ) -> impl Future<Output = Result<Option<CalendarObservation>, AgentFailure>> + Send {
        async { Ok(None) }
    }

    fn observe_projected(
        &self,
        _: CalendarObserveRequest,
    ) -> impl Future<Output = Result<Option<ProjectedCalendarObservation>, AgentFailure>> + Send
    {
        async { Ok(None) }
    }
}

/// Whether an admission still covers the source view that was assembled from it.
///
/// Access does not know which Expert's view this is; it only checks that the
/// scope and the dependency still match what was admitted.
pub fn admission_matches<View: serde::Serialize>(
    admission: &CalendarReadAccessAdmission,
    view: &floe_context::SourceView<View>,
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

#[derive(Clone)]
pub struct CalendarObserveRequest {
    pub person_id: PersonId,
    pub device_id: String,
    pub provider: CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub expected_native_subject_fingerprint: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

pub struct CalendarObservation {
    pub stamp: CalendarReadAccessStamp,
    pub observed_at: DateTime<Utc>,
    pub batches: Vec<CalendarBatch>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectedCalendarObservation {
    pub stamp: CalendarReadAccessStamp,
    pub source_handle: String,
    pub observed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub range_start: DateTime<Utc>,
    pub range_end: DateTime<Utc>,
    pub coverage_complete: bool,
    pub next_cursor: Option<String>,
    pub items: Vec<ProjectedCalendarItem>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectedCalendarItem {
    pub evidence_handle: String,
    pub untrusted_title: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}
