use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use floe_agent::{
    AgentFailure, Cancellation, DataClass, ExpertTimelineView, ExpertViews,
    MAX_TIMELINE_VIEW_BYTES, MAX_TIMELINE_VIEW_DAYS, MAX_TIMELINE_VIEW_ITEMS, TimelineViewItem,
    TimelineViewRead,
};
use floe_domain::{
    CalendarMirror, CalendarProvider, CalendarRange, ConsumerPolicyAuthority, ContextDependency,
    EventSchedule, GrantAuthority, GrantConsumer, GrantId, GrantOperation, GrantPurpose,
    GrantScope, GrantSourceBinding, PersonId, ProcessingRestriction, SourceRef,
};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use floe_context::MAX_LEASE_BYTES;

use crate::FloeCore;
use crate::calendar_lease::{
    CalendarLeaseDependencies, CalendarLeaseEntry, CalendarLeaseKey,
};

#[derive(Clone)]
pub struct CalendarTimelineGrant {
    pub person_id: PersonId,
    pub handle: Uuid,
    pub provider: CalendarProvider,
    pub device_id: String,
    pub calendar_ids: Vec<String>,
    pub connection_revision: u64,
    pub day: CalendarRange,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl CalendarTimelineGrant {
    pub fn data_class(&self) -> DataClass {
        match self.provider {
            CalendarProvider::Fixture => DataClass::Synthetic,
            CalendarProvider::EventKit
            | CalendarProvider::Google
            | CalendarProvider::Microsoft
            | CalendarProvider::Android => DataClass::Personal,
        }
    }

    fn validate(&self, now: DateTime<Utc>) -> Result<(), AgentFailure> {
        let identifiers: HashSet<_> = self.calendar_ids.iter().collect();
        let (day_start, day_end) = range_bounds(&self.day)?;
        let range_days = (self.day.end_date_exclusive - self.day.start_date).num_days();
        if !(1..=MAX_TIMELINE_VIEW_DAYS).contains(&range_days)
            || self.device_id.trim().is_empty()
            || self.device_id.len() > 128
            || self.calendar_ids.is_empty()
            || self.calendar_ids.len() > 4
            || identifiers.len() != self.calendar_ids.len()
            || self
                .calendar_ids
                .iter()
                .any(|identifier| identifier.trim().is_empty() || identifier.len() > 512)
            || self.starts_at < day_start
            || self.ends_at > day_end
            || self.starts_at >= self.ends_at
            || self.expires_at - now > chrono::Duration::minutes(5)
        {
            return Err(AgentFailure::InvalidInput);
        }
        if self.expires_at <= now {
            return Err(AgentFailure::StaleContext);
        }
        Ok(())
    }
}

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
    pub batches: Vec<floe_domain::CalendarBatch>,
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

pub struct CalendarTimelineViews<'host, Access, Clock> {
    core: &'host FloeCore,
    access: &'host Access,
    clock: Clock,
    grant: CalendarTimelineGrant,
    stamp: Mutex<Option<CalendarReadAccessStamp>>,
    live_observation_expires_at: Mutex<Option<DateTime<Utc>>>,
    invocation_id: Uuid,
    process_incarnation: Uuid,
    leases: Mutex<HashMap<CalendarLeaseKey, Arc<CalendarLeaseEntry>>>,
    consumed: floe_context::ConsumedLineage,
    acquisition: tokio::sync::Mutex<()>,
    source_observed: AtomicBool,
    authorized_once: AtomicBool,
    fatal_source_denial: AtomicBool,
}

pub(crate) struct GovernedDependencyResolver<'views, 'host, Access, Clock> {
    views: &'views CalendarTimelineViews<'host, Access, Clock>,
}

impl<'views, 'host, Access, Clock> GovernedDependencyResolver<'views, 'host, Access, Clock> {
    pub(crate) fn new(views: &'views CalendarTimelineViews<'host, Access, Clock>) -> Self {
        Self { views }
    }
}

impl<'views, 'host, Access: CalendarReadAccess, Clock: Fn() -> DateTime<Utc> + Sync>
    GovernedDependencyResolver<'views, 'host, Access, Clock>
{
    pub(crate) async fn resolve(
        &self,
        dependency: &ContextDependency,
        deadline: Instant,
        cancellation: Cancellation,
    ) -> Result<(), AgentFailure> {
        self.views
            .resolve_dependency(dependency, deadline, cancellation)
            .await
    }
}

struct AuthorizedRead {
    stamp: CalendarReadAccessStamp,
    admission: Option<CalendarReadAccessAdmission>,
}

fn admission_matches(
    admission: &CalendarReadAccessAdmission,
    dependency: &CalendarLeaseDependencies,
) -> bool {
    admission.person_id == dependency.person_id
        && admission.grant_id == dependency.grant_id
        && admission.grant_authority == dependency.grant_authority
        && admission.source == dependency.source
        && admission.scope == dependency.scope
        && admission.consumer_policy == dependency.consumer_policy
        && admission.operation == dependency.operation
        && admission.purpose == dependency.purpose
        && admission.consumer == dependency.consumer
        && admission.processing == dependency.processing
}

fn admission_matches_dependency(
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

impl<'host, Access: CalendarReadAccess, Clock: Fn() -> DateTime<Utc> + Sync>
    CalendarTimelineViews<'host, Access, Clock>
{
    pub fn new(
        core: &'host FloeCore,
        access: &'host Access,
        grant: CalendarTimelineGrant,
        clock: Clock,
    ) -> Result<Self, AgentFailure> {
        grant.validate(clock())?;
        Ok(Self {
            core,
            access,
            clock,
            grant,
            stamp: Mutex::new(None),
            live_observation_expires_at: Mutex::new(None),
            invocation_id: Uuid::new_v4(),
            process_incarnation: core.lease_registry.process_incarnation(),
            leases: Mutex::new(HashMap::new()),
            consumed: floe_context::ConsumedLineage::default(),
            acquisition: tokio::sync::Mutex::new(()),
            source_observed: AtomicBool::new(false),
            authorized_once: AtomicBool::new(false),
            fatal_source_denial: AtomicBool::new(false),
        })
    }

    pub fn grant(&self) -> &CalendarTimelineGrant {
        &self.grant
    }

    pub fn current_time(&self) -> DateTime<Utc> {
        (self.clock)()
    }

    pub fn consumed_context_dependencies(&self) -> Result<Vec<ContextDependency>, AgentFailure> {
        self.consumed.dependencies()
    }

    pub fn source_was_observed(&self) -> bool {
        self.source_observed.load(Ordering::Acquire)
    }

    pub(crate) fn validate_dependency_liveness(
        &self,
        dependency: &ContextDependency,
    ) -> Result<(), AgentFailure> {
        if dependency.person_id() != self.grant.person_id
            || dependency.process_incarnation_id() != self.process_incarnation
            || dependency.expires_at() <= (self.clock)()
        {
            return Err(AgentFailure::StaleContext);
        }
        let (evidence, _) = self.core.lease_registry.observation(dependency)?;
        if evidence != *dependency {
            return Err(AgentFailure::StaleContext);
        }
        Ok(())
    }

    pub fn source_denial_requires_halt(&self) -> bool {
        self.fatal_source_denial.load(Ordering::Acquire)
    }

    pub(crate) async fn resolve_dependency(
        &self,
        dependency: &ContextDependency,
        deadline: Instant,
        cancellation: Cancellation,
    ) -> Result<(), AgentFailure> {
        if dependency.person_id() != self.grant.person_id
            || dependency.process_incarnation_id() != self.process_incarnation
            || dependency.expires_at() <= (self.clock)()
        {
            return Err(AgentFailure::StaleContext);
        }
        let (evidence, subject) = self.core.lease_registry.observation(dependency)?;
        if evidence != *dependency {
            return Err(AgentFailure::StaleContext);
        }
        check_running(deadline, &cancellation)?;
        let calendar_ids = dependency
            .resources()
            .iter()
            .map(|resource| resource.as_str().to_owned())
            .collect::<Vec<_>>();
        let request = CalendarReadAccessRequest {
            person_id: self.grant.person_id,
            device_id: self.grant.device_id.clone(),
            provider: self.grant.provider,
            calendar_ids,
            expected_native_subject_fingerprint: Some(subject.clone()),
            deadline,
            cancellation: cancellation.clone(),
        };
        let stamp = self.access.check(request.clone()).await?;
        if stamp.native_subject_fingerprint != subject {
            return Err(AgentFailure::StaleContext);
        }
        let admission = self.access.admission_after_check(&request, &stamp).await?;
        let Some(admission) = admission else {
            return Err(AgentFailure::StaleContext);
        };
        if !admission_matches_dependency(&admission, dependency) {
            return Err(AgentFailure::StaleContext);
        }
        check_running(deadline, &cancellation)
    }

    fn lease_key(
        &self,
        request: &TimelineViewRead,
        range_start: DateTime<Utc>,
        range_end: DateTime<Utc>,
    ) -> Result<CalendarLeaseKey, AgentFailure> {
        Ok(CalendarLeaseKey {
            invocation_id: self.invocation_id,
            person_id: self.grant.person_id,
            handle: self.grant.handle,
            device_id: self.grant.device_id.clone(),
            calendar_ids: {
                let mut calendar_ids = self.grant.calendar_ids.clone();
                calendar_ids.sort();
                calendar_ids
            },
            range_start_unix_ms: range_start.timestamp_millis(),
            range_end_unix_ms: range_end.timestamp_millis(),
            timezone_offset_seconds: self.grant.day.timezone_offset_seconds,
            end_timezone_offset_seconds: self.grant.day.end_timezone_offset_seconds,
            max_items: request.max_items.min(MAX_TIMELINE_VIEW_ITEMS),
            max_bytes: request.max_bytes.min(MAX_TIMELINE_VIEW_BYTES),
        })
    }

    fn cached_lease(
        &self,
        key: &CalendarLeaseKey,
    ) -> Result<Option<Arc<CalendarLeaseEntry>>, AgentFailure> {
        let mut leases = self
            .leases
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        leases.retain(|_, lease| lease.is_fresh());
        let Some(lease) = leases.get(key).cloned() else {
            return Ok(None);
        };
        Ok(Some(lease))
    }

    fn validate_live_leases(
        &self,
        admission: Option<&CalendarReadAccessAdmission>,
    ) -> Result<(), AgentFailure> {
        let leases = self
            .leases
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        for lease in leases.values() {
            if !lease.is_fresh() {
                return Err(AgentFailure::StaleContext);
            }
            let Some(admission) = admission else {
                return Err(AgentFailure::StaleContext);
            };
            if !admission_matches(admission, &lease.dependencies) {
                return Err(AgentFailure::StaleContext);
            }
        }
        self.consumed
            .validate((self.clock)(), Instant::now(), |dependency, scope| {
                admission.is_some_and(|admission| {
                    admission.scope == *scope && admission_matches_dependency(admission, dependency)
                })
            })
    }

    async fn finish_lease(
        &self,
        key: CalendarLeaseKey,
        before: AuthorizedRead,
        after: AuthorizedRead,
        mut view: ExpertTimelineView,
        reservation: Option<floe_context::SourceLeaseReservation>,
        observed_at: DateTime<Utc>,
        acquisition_wall: DateTime<Utc>,
        acquisition_mono: Instant,
        deadline: Instant,
    ) -> Result<ExpertTimelineView, AgentFailure> {
        if before.stamp != after.stamp || before.admission != after.admission {
            return Err(AgentFailure::StaleContext);
        }
        let Some(admission) = before.admission else {
            let mut saved = self
                .stamp
                .lock()
                .map_err(|_| AgentFailure::CapabilityUnavailable)?;
            if saved
                .as_ref()
                .is_some_and(|previous| previous != &before.stamp)
            {
                return Err(AgentFailure::StaleContext);
            }
            *saved = Some(before.stamp);
            self.source_observed.store(true, Ordering::Release);
            return Ok(view);
        };
        let expires_at = DateTime::from_timestamp_millis(
            i64::try_from(view.expires_at_unix_ms).map_err(|_| AgentFailure::InvalidInput)?,
        )
        .ok_or(AgentFailure::InvalidInput)?;
        let completion_wall = (self.clock)();
        if completion_wall < acquisition_wall || expires_at <= completion_wall {
            return Err(AgentFailure::StaleContext);
        }
        let duration = (expires_at - acquisition_wall)
            .to_std()
            .map_err(|_| AgentFailure::StaleContext)?;
        let expires_at_monotonic = acquisition_mono
            .checked_add(duration)
            .ok_or(AgentFailure::BudgetExceeded)?
            .min(deadline);
        if expires_at_monotonic <= Instant::now() {
            return Err(AgentFailure::StaleContext);
        }
        let observation_id = Uuid::new_v4();
        view.source_handle = format!("calendar.lease:{observation_id}");
        let bytes = serde_json::to_vec(&view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len();
        if bytes == 0 || bytes > MAX_LEASE_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        let dependency = CalendarLeaseDependencies::from_admission(
            self.invocation_id,
            self.process_incarnation,
            observation_id,
            &admission,
            &key,
            observed_at,
            expires_at,
        )?;
        let reservation = reservation.ok_or(AgentFailure::CapabilityUnavailable)?;
        let lease = Arc::new(CalendarLeaseEntry {
            _key: key.clone(),
            dependencies: dependency.clone(),
            view: view.clone(),
            expires_at: expires_at_monotonic,
            _reservation: reservation,
        });
        self.core.lease_registry.retain_observation(
            dependency.dependency.clone(),
            before.stamp.native_subject_fingerprint.clone(),
            expires_at_monotonic,
        )?;
        self.leases
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .insert(key, lease);
        self.consumed.record(
            dependency.dependency,
            dependency.scope,
            expires_at_monotonic,
        )?;
        let mut saved = self
            .stamp
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        *saved = Some(after.stamp);
        self.source_observed.store(true, Ordering::Release);
        Ok(view)
    }

    async fn authorized(
        &self,
        deadline: Instant,
        cancellation: Cancellation,
        allow_generation_change: bool,
    ) -> Result<AuthorizedRead, AgentFailure> {
        check_running(deadline, &cancellation)?;
        let request = CalendarReadAccessRequest {
            person_id: self.grant.person_id,
            device_id: self.grant.device_id.clone(),
            provider: self.grant.provider,
            calendar_ids: self.grant.calendar_ids.clone(),
            expected_native_subject_fingerprint: None,
            deadline,
            cancellation: cancellation.clone(),
        };
        let mut actual = self.access.check(request.clone()).await?;
        let admission = self.access.admission_after_check(&request, &actual).await?;
        check_running(deadline, &cancellation)?;
        actual.calendar_ids.sort();
        let mut expected = self.grant.calendar_ids.clone();
        expected.sort();
        floe_access::validate_read_authority(
            &floe_access::ReadAuthorityIdentity {
                person_id: self.grant.person_id,
                device_id: &self.grant.device_id,
                provider: &self.grant.provider,
                resource_ids: &expected,
            },
            &floe_access::ReadAuthorityEvidence {
                schema_version: actual.schema_version,
                identity: floe_access::ReadAuthorityIdentity {
                    person_id: actual.person_id,
                    device_id: &actual.device_id,
                    provider: &actual.provider,
                    resource_ids: &actual.calendar_ids,
                },
                subject_fingerprint: &actual.native_subject_fingerprint,
                generation: &actual.generation,
            },
        )?;
        if let Some(previous) = self
            .stamp
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .as_ref()
        {
            floe_access::validate_read_continuity(
                &previous.native_subject_fingerprint,
                &previous.generation,
                &actual.native_subject_fingerprint,
                &actual.generation,
                allow_generation_change,
            )?;
        }
        self.authorized_once.store(true, Ordering::Release);
        Ok(AuthorizedRead {
            stamp: actual,
            admission,
        })
    }

    pub async fn revalidate(
        &self,
        deadline: Instant,
        cancellation: Cancellation,
    ) -> Result<(), AgentFailure> {
        check_running(deadline, &cancellation)?;
        let has_leases = !self
            .leases
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .is_empty();
        if !has_leases
            && self
                .stamp
                .lock()
                .map_err(|_| AgentFailure::CapabilityUnavailable)?
                .is_none()
        {
            return Ok(());
        }
        let deadline = deadline.min(Instant::now() + Duration::from_secs(30));
        let child = Cancellation::default();
        let _cancel = CancelAccess(child.clone());
        let result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(deadline) => Err(AgentFailure::DeadlineExceeded),
            result = async {
                let current = self.authorized(deadline, child.clone(), true).await?;
                self.validate_live_leases(current.admission.as_ref())?;
                let live_observation_expires_at = {
                    *self
                        .live_observation_expires_at
                        .lock()
                        .map_err(|_| AgentFailure::CapabilityUnavailable)?
                };
                if let Some(expires_at) = live_observation_expires_at {
                    if expires_at <= (self.clock)() {
                        return Err(AgentFailure::StaleContext);
                    }
                    self.authorized(deadline, child.clone(), true).await?;
                    return Ok(());
                }
                let mirror = self.core.store.bounded_calendar_mirror(self.grant.person_id).await?;
                self.validate_mirror(&mirror, (self.clock)())?;
                self.authorized(deadline, child.clone(), true).await?;
                Ok(())
            } => result,
        };
        check_running(deadline, &cancellation)?;
        result
    }

    fn validate_mirror(
        &self,
        mirror: &CalendarMirror,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, AgentFailure> {
        self.grant.validate(now)?;
        let connection = &mirror.connection;
        if connection.disconnected
            || connection.provider != self.grant.provider
            || connection.revision != self.grant.connection_revision
        {
            return Err(AgentFailure::StaleContext);
        }
        let selected = connection.calendars.clone();
        let mut expiry = self.grant.expires_at;
        for identifier in &self.grant.calendar_ids {
            if !selected
                .iter()
                .any(|calendar| &calendar.calendar_id == identifier)
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let status = connection
                .source_statuses
                .get(identifier)
                .ok_or(AgentFailure::StaleContext)?;
            if let Some(failure) = status.error {
                return Err(match failure {
                    floe_domain::CalendarFailure::PermissionDenied => {
                        AgentFailure::CapabilityDenied
                    }
                    _ => AgentFailure::CapabilityUnavailable,
                });
            }
            let success = status.last_success_at.ok_or(AgentFailure::StaleContext)?;
            let source_expiry = success
                .checked_add_signed(chrono::Duration::minutes(5))
                .ok_or(AgentFailure::InvalidInput)?;
            let range = status
                .last_range
                .as_ref()
                .ok_or(AgentFailure::StaleContext)?;
            let (start, end) = range_bounds(range)?;
            if success > now
                || source_expiry <= now
                || self.grant.starts_at < start
                || self.grant.ends_at > end
                || self.grant.day.start_date < range.start_date
                || self.grant.day.end_date_exclusive > range.end_date_exclusive
            {
                return Err(AgentFailure::StaleContext);
            }
            expiry = expiry.min(source_expiry);
        }
        if mirror.events.len() > 10_000
            || mirror
                .events
                .iter()
                .any(|event| event.person_id != self.grant.person_id)
        {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        Ok(expiry)
    }

    async fn read(
        &self,
        request: &TimelineViewRead,
        deadline: Instant,
    ) -> Result<ExpertTimelineView, AgentFailure> {
        if request.person_id != self.grant.person_id || request.handle != self.grant.handle {
            return Err(AgentFailure::CapabilityDenied);
        }
        if request.max_items == 0 || request.max_bytes == 0 || request.cursor.is_some() {
            return Err(AgentFailure::BudgetExceeded);
        }
        self.grant.validate((self.clock)())?;
        let (range_start, range_end) = requested_range(request, &self.grant)?;
        let key = self.lease_key(request, range_start, range_end)?;
        let _acquisition = tokio::select! {
            biased;
            _ = request.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(deadline) => return Err(AgentFailure::DeadlineExceeded),
            lock = self.acquisition.lock() => lock,
        };
        check_running(deadline, &request.cancellation)?;
        let cached = self.cached_lease(&key)?;
        let has_native_lease = !self
            .leases
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .is_empty();
        let before = self
            .authorized(
                deadline,
                request.cancellation.clone(),
                cached.is_some() || has_native_lease,
            )
            .await?;
        if let Some(lease) = cached {
            let Some(admission) = before.admission.as_ref() else {
                return Err(AgentFailure::StaleContext);
            };
            if admission_matches(admission, &lease.dependencies) {
                return Ok(lease.view.clone());
            }
            self.leases
                .lock()
                .map_err(|_| AgentFailure::CapabilityUnavailable)?
                .remove(&key);
        }
        let reservation = before
            .admission
            .as_ref()
            .map(|_| {
                self.core.lease_registry.reserve(
                    self.grant.person_id,
                    request.max_bytes.min(MAX_TIMELINE_VIEW_BYTES),
                )
            })
            .transpose()?;
        let acquisition_wall = (self.clock)();
        let acquisition_mono = Instant::now();
        let projected = self
            .access
            .observe_projected(CalendarObserveRequest {
                person_id: self.grant.person_id,
                device_id: self.grant.device_id.clone(),
                provider: self.grant.provider,
                calendar_ids: self.grant.calendar_ids.clone(),
                expected_native_subject_fingerprint: before
                    .stamp
                    .native_subject_fingerprint
                    .clone()
                    .into(),
                starts_at: range_start,
                ends_at: range_end,
                deadline,
                cancellation: request.cancellation.clone(),
            })
            .await;
        let projected = match projected {
            Err(AgentFailure::CapabilityDenied) if self.authorized_once.load(Ordering::Acquire) => {
                self.fatal_source_denial.store(true, Ordering::Release);
                return Err(AgentFailure::CapabilityDenied);
            }
            Err(failure) => return Err(failure),
            Ok(value) => value,
        };
        if let Some(observation) = projected {
            return self
                .project_projected_observation(
                    request,
                    range_start,
                    range_end,
                    before,
                    key.clone(),
                    reservation,
                    observation,
                    acquisition_wall,
                    acquisition_mono,
                    deadline,
                )
                .await;
        }
        let observed = self
            .access
            .observe(CalendarObserveRequest {
                person_id: self.grant.person_id,
                device_id: self.grant.device_id.clone(),
                provider: self.grant.provider,
                calendar_ids: self.grant.calendar_ids.clone(),
                expected_native_subject_fingerprint: before
                    .stamp
                    .native_subject_fingerprint
                    .clone()
                    .into(),
                starts_at: range_start,
                ends_at: range_end,
                deadline,
                cancellation: request.cancellation.clone(),
            })
            .await;
        let observed = match observed {
            Err(AgentFailure::CapabilityDenied) if self.authorized_once.load(Ordering::Acquire) => {
                self.fatal_source_denial.store(true, Ordering::Release);
                return Err(AgentFailure::CapabilityDenied);
            }
            Err(failure) => return Err(failure),
            Ok(value) => value,
        };
        if let Some(observation) = observed {
            return self
                .project_observation(
                    request,
                    range_start,
                    range_end,
                    before,
                    key.clone(),
                    reservation,
                    observation,
                    acquisition_wall,
                    acquisition_mono,
                    deadline,
                )
                .await;
        }
        if self.grant.provider != CalendarProvider::Fixture {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let mirror = self
            .core
            .store
            .bounded_calendar_mirror(self.grant.person_id)
            .await?;
        let expires = self.validate_mirror(&mirror, (self.clock)())?;
        let mut items = vec![];
        let mut identifiers = HashSet::new();
        for event in &mirror.events {
            if event.deleted_at.is_some() {
                continue;
            }
            let SourceRef::Calendar(source) = &event.source else {
                return Err(AgentFailure::CapabilityUnavailable);
            };
            if !self.grant.calendar_ids.contains(&source.calendar_id) {
                continue;
            }
            if source.provider != self.grant.provider || !identifiers.insert(event.id) {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            let (start, end) = match &event.schedule {
                EventSchedule::Timed(schedule) => {
                    if schedule.starts_at >= schedule.ends_at {
                        return Err(AgentFailure::CapabilityUnavailable);
                    }
                    (
                        schedule.starts_at.max(range_start),
                        schedule.ends_at.min(range_end),
                    )
                }
                EventSchedule::AllDay(schedule) => {
                    if schedule.start_date >= schedule.end_date_exclusive {
                        return Err(AgentFailure::CapabilityUnavailable);
                    }
                    if schedule.start_date >= self.grant.day.end_date_exclusive
                        || schedule.end_date_exclusive <= self.grant.day.start_date
                    {
                        continue;
                    }
                    let initial_offset = self.grant.day.timezone_offset_seconds;
                    let final_offset = self
                        .grant
                        .day
                        .end_timezone_offset_seconds
                        .unwrap_or(initial_offset);
                    (
                        date_boundary(schedule.start_date, initial_offset.max(final_offset))?
                            .max(range_start),
                        date_boundary(
                            schedule.end_date_exclusive,
                            initial_offset.min(final_offset),
                        )?
                        .min(range_end),
                    )
                }
            };
            if start >= end {
                continue;
            }
            if items.len() >= request.max_items.min(MAX_TIMELINE_VIEW_ITEMS) {
                return Err(AgentFailure::BudgetExceeded);
            }
            items.push(TimelineViewItem {
                evidence_handle: event.id.0,
                untrusted_title: bounded_title(&event.title),
                starts_at_unix_ms: milliseconds(start)?,
                ends_at_unix_ms: milliseconds(end)?,
            });
        }
        items.sort_by_key(|item| {
            (
                item.starts_at_unix_ms,
                item.ends_at_unix_ms,
                item.evidence_handle,
            )
        });
        let view = ExpertTimelineView {
            schema_version: 1,
            handle: self.grant.handle,
            person_id: self.grant.person_id,
            data_class: self.grant.data_class(),
            source_handle: format!(
                "calendar.timeline:{}:{}",
                self.grant.handle, self.grant.connection_revision
            ),
            range_start_unix_ms: milliseconds(range_start)?,
            range_end_unix_ms: milliseconds(range_end)?,
            expires_at_unix_ms: milliseconds(expires)?,
            coverage_complete: true,
            next_cursor: None,
            items,
        };
        if serde_json::to_vec(&view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > request.max_bytes.min(MAX_TIMELINE_VIEW_BYTES)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let after = self
            .authorized(
                deadline,
                request.cancellation.clone(),
                before.admission.is_some(),
            )
            .await?;
        if before.stamp != after.stamp || before.admission != after.admission {
            return Err(AgentFailure::StaleContext);
        }
        let latest = self
            .core
            .store
            .bounded_calendar_mirror(self.grant.person_id)
            .await?;
        self.validate_mirror(&latest, (self.clock)())?;
        if latest != mirror {
            return Err(AgentFailure::StaleContext);
        }
        check_running(deadline, &request.cancellation)?;
        let mut saved = self
            .stamp
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        if saved
            .as_ref()
            .is_some_and(|previous| previous != &before.stamp)
        {
            return Err(AgentFailure::StaleContext);
        }
        *saved = Some(before.stamp);
        Ok(view)
    }

    async fn project_observation(
        &self,
        request: &TimelineViewRead,
        range_start: DateTime<Utc>,
        range_end: DateTime<Utc>,
        before: AuthorizedRead,
        key: CalendarLeaseKey,
        reservation: Option<floe_context::SourceLeaseReservation>,
        mut observation: CalendarObservation,
        acquisition_wall: DateTime<Utc>,
        acquisition_mono: Instant,
        deadline: Instant,
    ) -> Result<ExpertTimelineView, AgentFailure> {
        let observed_at = observation.observed_at;
        observation.stamp.calendar_ids.sort();
        if observation.stamp != before.stamp
            || observed_at > (self.clock)()
            || (self.clock)() - observed_at > chrono::Duration::minutes(5)
        {
            return Err(AgentFailure::StaleContext);
        }
        let expected: HashSet<_> = self.grant.calendar_ids.iter().map(String::as_str).collect();
        let received: HashSet<_> = observation
            .batches
            .iter()
            .map(|batch| batch.calendar_id.as_str())
            .collect();
        if expected != received || observation.batches.len() != expected.len() {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let mut items = vec![];
        let mut evidence = HashSet::new();
        for batch in observation.batches {
            if let Some(failure) = batch.failure {
                return Err(match failure {
                    floe_domain::CalendarFailure::PermissionDenied => {
                        AgentFailure::CapabilityDenied
                    }
                    _ => AgentFailure::CapabilityUnavailable,
                });
            }
            for record in batch.records {
                if record.calendar_id != batch.calendar_id
                    || record.external_id.trim().is_empty()
                    || record.external_revision.trim().is_empty()
                {
                    return Err(AgentFailure::CapabilityUnavailable);
                }
                let (start, end) = observation_schedule_bounds(
                    &record.schedule,
                    range_start,
                    range_end,
                    self.grant.day.timezone_offset_seconds,
                    self.grant.day.end_timezone_offset_seconds,
                )?;
                if start >= end {
                    continue;
                }
                let evidence_handle = Uuid::new_v5(
                    &Uuid::NAMESPACE_URL,
                    format!(
                        "floe:calendar:{:?}:{}:{}",
                        self.grant.provider, batch.calendar_id, record.external_id
                    )
                    .as_bytes(),
                );
                if !evidence.insert(evidence_handle) {
                    return Err(AgentFailure::CapabilityUnavailable);
                }
                if items.len() >= request.max_items.min(MAX_TIMELINE_VIEW_ITEMS) {
                    return Err(AgentFailure::BudgetExceeded);
                }
                items.push(TimelineViewItem {
                    evidence_handle,
                    untrusted_title: bounded_title(&record.title),
                    starts_at_unix_ms: milliseconds(start)?,
                    ends_at_unix_ms: milliseconds(end)?,
                });
            }
        }
        items.sort_by_key(|item| {
            (
                item.starts_at_unix_ms,
                item.ends_at_unix_ms,
                item.evidence_handle,
            )
        });
        let expires = self
            .grant
            .expires_at
            .min(observed_at + chrono::Duration::minutes(5));
        let view = ExpertTimelineView {
            schema_version: 1,
            handle: self.grant.handle,
            person_id: self.grant.person_id,
            data_class: self.grant.data_class(),
            source_handle: format!(
                "calendar.observe:{}:{}",
                self.grant.handle, observation.stamp.generation
            ),
            range_start_unix_ms: milliseconds(range_start)?,
            range_end_unix_ms: milliseconds(range_end)?,
            expires_at_unix_ms: milliseconds(expires)?,
            coverage_complete: true,
            next_cursor: None,
            items,
        };
        if serde_json::to_vec(&view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > request.max_bytes.min(MAX_TIMELINE_VIEW_BYTES)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let after = self
            .authorized(
                deadline,
                request.cancellation.clone(),
                before.admission.is_some(),
            )
            .await?;
        self.finish_lease(
            key,
            before,
            after,
            view,
            reservation,
            observed_at,
            acquisition_wall,
            acquisition_mono,
            deadline,
        )
        .await
    }

    async fn project_projected_observation(
        &self,
        request: &TimelineViewRead,
        range_start: DateTime<Utc>,
        range_end: DateTime<Utc>,
        before: AuthorizedRead,
        key: CalendarLeaseKey,
        reservation: Option<floe_context::SourceLeaseReservation>,
        mut observation: ProjectedCalendarObservation,
        acquisition_wall: DateTime<Utc>,
        acquisition_mono: Instant,
        deadline: Instant,
    ) -> Result<ExpertTimelineView, AgentFailure> {
        let observed_at = observation.observed_at;
        observation.stamp.calendar_ids.sort();
        let now = (self.clock)();
        if observation.stamp != before.stamp
            || observed_at > now
            || now - observed_at > chrono::Duration::minutes(5)
            || observation.expires_at <= now
            || observation.range_start > range_start
            || observation.range_end < range_end
        {
            return Err(AgentFailure::StaleContext);
        }
        if observation.source_handle.trim().is_empty()
            || observation.source_handle.len() > 128
            || observation.expires_at <= observation.observed_at
            || observation.expires_at - observation.observed_at > chrono::Duration::minutes(5)
            || observation.range_start >= observation.range_end
            || observation.coverage_complete == observation.next_cursor.is_some()
            || observation.next_cursor.as_ref().is_some_and(|cursor| {
                cursor.trim().is_empty()
                    || cursor.len() > 2048
                    || cursor.chars().any(char::is_control)
            })
        {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        if observation.items.len() > request.max_items.min(MAX_TIMELINE_VIEW_ITEMS) {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut handles = HashSet::new();
        let mut items = Vec::with_capacity(observation.items.len());
        for item in observation.items {
            if item.evidence_handle.trim().is_empty()
                || item.evidence_handle.len() > 128
                || !handles.insert(item.evidence_handle.clone())
                || item.starts_at >= item.ends_at
                || item.starts_at >= range_end
                || item.ends_at <= range_start
            {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            items.push(TimelineViewItem {
                evidence_handle: Uuid::new_v5(
                    &Uuid::NAMESPACE_URL,
                    format!(
                        "floe:calendar:{}:{}",
                        observation.source_handle, item.evidence_handle
                    )
                    .as_bytes(),
                ),
                untrusted_title: bounded_title(&item.untrusted_title),
                starts_at_unix_ms: milliseconds(item.starts_at.max(range_start))?,
                ends_at_unix_ms: milliseconds(item.ends_at.min(range_end))?,
            });
        }
        items.sort_by_key(|item| {
            (
                item.starts_at_unix_ms,
                item.ends_at_unix_ms,
                item.evidence_handle,
            )
        });
        let view = ExpertTimelineView {
            schema_version: 1,
            handle: self.grant.handle,
            person_id: self.grant.person_id,
            data_class: self.grant.data_class(),
            source_handle: observation.source_handle,
            range_start_unix_ms: milliseconds(range_start)?,
            range_end_unix_ms: milliseconds(range_end)?,
            expires_at_unix_ms: milliseconds(self.grant.expires_at.min(observation.expires_at))?,
            coverage_complete: observation.coverage_complete,
            next_cursor: observation.next_cursor,
            items,
        };
        if serde_json::to_vec(&view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > request.max_bytes.min(MAX_TIMELINE_VIEW_BYTES)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let after = self
            .authorized(
                deadline,
                request.cancellation.clone(),
                before.admission.is_some(),
            )
            .await?;
        self.finish_lease(
            key,
            before,
            after,
            view,
            reservation,
            observed_at,
            acquisition_wall,
            acquisition_mono,
            deadline,
        )
        .await
    }
}

fn observation_schedule_bounds(
    schedule: &EventSchedule,
    range_start: DateTime<Utc>,
    range_end: DateTime<Utc>,
    start_offset: i32,
    end_offset: Option<i32>,
) -> Result<(DateTime<Utc>, DateTime<Utc>), AgentFailure> {
    let (start, end) = match schedule {
        EventSchedule::Timed(schedule) => (schedule.starts_at, schedule.ends_at),
        EventSchedule::AllDay(schedule) => {
            let end_offset = end_offset.unwrap_or(start_offset);
            (
                date_boundary(schedule.start_date, start_offset.max(end_offset))?,
                date_boundary(schedule.end_date_exclusive, start_offset.min(end_offset))?,
            )
        }
    };
    if start >= end {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    Ok((start.max(range_start), end.min(range_end)))
}

impl<Access: CalendarReadAccess, Clock: Fn() -> DateTime<Utc> + Sync> ExpertViews
    for CalendarTimelineViews<'_, Access, Clock>
{
    async fn timeline(
        &self,
        request: TimelineViewRead,
    ) -> Result<ExpertTimelineView, AgentFailure> {
        let deadline = request
            .deadline
            .min(Instant::now() + Duration::from_secs(30));
        let child = Cancellation::default();
        let _cancel = CancelAccess(child.clone());
        let bounded = TimelineViewRead {
            person_id: request.person_id,
            handle: request.handle,
            range_start_unix_ms: request.range_start_unix_ms,
            range_end_unix_ms: request.range_end_unix_ms,
            cursor: request.cursor,
            max_items: request.max_items,
            max_bytes: request.max_bytes,
            deadline,
            cancellation: child,
        };
        let result = tokio::select! {
            biased;
            _ = request.cancellation.cancelled() => Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(deadline) => Err(AgentFailure::DeadlineExceeded),
            result = self.read(&bounded, deadline) => result,
        };
        check_running(deadline, &request.cancellation)?;
        result
    }
}

fn requested_range(
    request: &TimelineViewRead,
    grant: &CalendarTimelineGrant,
) -> Result<(DateTime<Utc>, DateTime<Utc>), AgentFailure> {
    let (start, end) = match (request.range_start_unix_ms, request.range_end_unix_ms) {
        (None, None) => (grant.starts_at, grant.ends_at),
        (Some(start), Some(end)) => (
            DateTime::from_timestamp_millis(
                i64::try_from(start).map_err(|_| AgentFailure::InvalidInput)?,
            )
            .ok_or(AgentFailure::InvalidInput)?,
            DateTime::from_timestamp_millis(
                i64::try_from(end).map_err(|_| AgentFailure::InvalidInput)?,
            )
            .ok_or(AgentFailure::InvalidInput)?,
        ),
        _ => return Err(AgentFailure::InvalidInput),
    };
    if start >= end || end - start > chrono::Duration::days(MAX_TIMELINE_VIEW_DAYS + 1) {
        return Err(AgentFailure::InvalidInput);
    }
    Ok((start, end))
}

struct CancelAccess(Cancellation);

impl Drop for CancelAccess {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

fn range_bounds(range: &CalendarRange) -> Result<(DateTime<Utc>, DateTime<Utc>), AgentFailure> {
    if !range.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let start = range
        .start_date
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        .checked_sub_signed(chrono::Duration::seconds(i64::from(
            range.timezone_offset_seconds,
        )))
        .ok_or(AgentFailure::InvalidInput)?;
    let end = range
        .end_date_exclusive
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        .checked_sub_signed(chrono::Duration::seconds(i64::from(
            range
                .end_timezone_offset_seconds
                .unwrap_or(range.timezone_offset_seconds),
        )))
        .ok_or(AgentFailure::InvalidInput)?;
    Ok((start, end))
}

fn date_boundary(
    date: chrono::NaiveDate,
    timezone_offset_seconds: i32,
) -> Result<DateTime<Utc>, AgentFailure> {
    date.and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        .checked_sub_signed(chrono::Duration::seconds(i64::from(
            timezone_offset_seconds,
        )))
        .ok_or(AgentFailure::InvalidInput)
}

fn milliseconds(time: DateTime<Utc>) -> Result<u64, AgentFailure> {
    u64::try_from(time.timestamp_millis()).map_err(|_| AgentFailure::InvalidInput)
}

fn check_running(deadline: Instant, cancellation: &Cancellation) -> Result<(), AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

fn bounded_title(title: &str) -> String {
    if title.len() <= 256 {
        return title.to_owned();
    }
    let boundary = title
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= 253)
        .last()
        .unwrap_or(0);
    format!("{}…", &title[..boundary])
}

#[cfg(test)]
mod tests;
