use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::time::Instant;

use floe_agent_contract::{AgentFailure};
use floe_execution::{Cancellation};
use floe_experts_builtin::{AttentionView, FeasibilityView, PeopleView, WellbeingView, validate_attention_view, validate_feasibility_view, validate_people_view, validate_wellbeing_view};
use floe_day::{CalendarBatch, CalendarFailure, CalendarProvider as DomainCalendarProvider, CalendarRecord};
use floe_kernel::{PersonId};
use floe_protocol::{
    CalendarProviderDto, LocalContextAcquisitionModeDto, LocalContextAcquisitionRequestDto,
    LocalContextAcquisitionResultDto, LocalContextAttentionAcquisitionModeDto,
    LocalContextAttentionAcquisitionRequestDto, LocalContextAttentionAcquisitionResultDto,
    LocalContextOperationDto, LocalContextPersonalAcquisitionRequestDto,
    LocalContextPersonalAcquisitionResultDto, LocalContextPersonalDomainDto, LocalContextResultDto,
};
#[cfg(test)]
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

use crate::{BridgeResult, agent_failure, invalid};

const ALLOWED_VIEW_IDS: [&str; 5] = [
    "people.identity",
    "schedule.feasibility",
    "attention.coarse",
    "wellbeing.derived",
    "calendar.timeline",
];

#[derive(Clone)]
struct Entry {
    device_id: String,
    observed_at_unix_ms: i64,
    expires_at_unix_ms: i64,
    observed_monotonic: Instant,
    monotonic_ttl: std::time::Duration,
    process_incarnation: Uuid,
    observation_id: Uuid,
    native_subject_fingerprint: Option<String>,
    view: Value,
}

#[derive(Clone)]
struct TrustedAttentionObservation {
    person_id: PersonId,
    device_id: String,
    host_epoch: String,
    observation_id: Uuid,
    process_incarnation: Uuid,
    native_subject_fingerprint: String,
    view: AttentionView,
    observed_monotonic: Instant,
    monotonic_ttl: std::time::Duration,
}

#[derive(Clone)]
pub struct TrustedPersonalObservation {
    pub(crate) person_id: PersonId,
    pub(crate) device_id: String,
    pub(crate) host_epoch: String,
    pub(crate) observation_id: Uuid,
    pub(crate) process_incarnation: Uuid,
    pub(crate) native_subject_fingerprint: String,
    pub(crate) observed_at_unix_ms: i64,
    pub(crate) expires_at_unix_ms: i64,
    pub(crate) query_fingerprint: Vec<u8>,
    observed_monotonic: Instant,
    monotonic_ttl: std::time::Duration,
}

pub struct LocalContextStore {
    entries: Mutex<HashMap<(PersonId, String, String), Entry>>,
    calendar_observations: Mutex<HashMap<(PersonId, String), PublishedCalendarObservation>>,
    acquisition: Mutex<AcquisitionState>,
    attention_acquisition: Mutex<AttentionAcquisitionState>,
    personal_acquisition: Mutex<PersonalAcquisitionState>,
    trusted_attention_observations: Mutex<VecDeque<TrustedAttentionObservation>>,
    trusted_personal_observations: Mutex<VecDeque<TrustedPersonalObservation>>,
    process_incarnation: Uuid,
}

impl Default for LocalContextStore {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            calendar_observations: Mutex::new(HashMap::new()),
            acquisition: Mutex::new(AcquisitionState::default()),
            attention_acquisition: Mutex::new(AttentionAcquisitionState::default()),
            personal_acquisition: Mutex::new(PersonalAcquisitionState::default()),
            trusted_attention_observations: Mutex::new(VecDeque::new()),
            trusted_personal_observations: Mutex::new(VecDeque::new()),
            process_incarnation: Uuid::new_v4(),
        }
    }
}

struct AcquisitionState {
    host_epoch: Option<String>,
    host_person: Option<PersonId>,
    pending: VecDeque<String>,
    requests: HashMap<String, LocalContextAcquisitionRequestDto>,
    waiters: HashMap<
        String,
        tokio::sync::oneshot::Sender<Result<LocalContextAcquisitionResultDto, AgentFailure>>,
    >,
    deadlines: HashMap<String, Instant>,
    in_flight: Option<String>,
}

impl Default for AcquisitionState {
    fn default() -> Self {
        Self {
            host_epoch: None,
            host_person: None,
            pending: VecDeque::new(),
            requests: HashMap::new(),
            waiters: HashMap::new(),
            deadlines: HashMap::new(),
            in_flight: None,
        }
    }
}

struct AcquisitionReservation<'a> {
    store: &'a LocalContextStore,
    request_id: String,
}

struct AttentionAcquisitionReservation<'a> {
    store: &'a LocalContextStore,
    request_id: String,
}

impl Drop for AttentionAcquisitionReservation<'_> {
    fn drop(&mut self) {
        self.store.cancel_attention_acquisition(&self.request_id);
    }
}

struct AttentionAcquisitionState {
    host_epoch: Option<String>,
    host_person: Option<PersonId>,
    pending: VecDeque<String>,
    requests: HashMap<String, LocalContextAttentionAcquisitionRequestDto>,
    waiters: HashMap<
        String,
        tokio::sync::oneshot::Sender<
            Result<LocalContextAttentionAcquisitionResultDto, AgentFailure>,
        >,
    >,
    deadlines: HashMap<String, Instant>,
    in_flight: Option<String>,
}

impl Default for AttentionAcquisitionState {
    fn default() -> Self {
        Self {
            host_epoch: None,
            host_person: None,
            pending: VecDeque::new(),
            requests: HashMap::new(),
            waiters: HashMap::new(),
            deadlines: HashMap::new(),
            in_flight: None,
        }
    }
}

struct PersonalAcquisitionState {
    host_epoch: Option<String>,
    host_person: Option<PersonId>,
    pending: VecDeque<String>,
    requests: HashMap<String, LocalContextPersonalAcquisitionRequestDto>,
    waiters: HashMap<
        String,
        tokio::sync::oneshot::Sender<
            Result<LocalContextPersonalAcquisitionResultDto, AgentFailure>,
        >,
    >,
    deadlines: HashMap<String, Instant>,
    in_flight: Option<String>,
}

impl Default for PersonalAcquisitionState {
    fn default() -> Self {
        Self {
            host_epoch: None,
            host_person: None,
            pending: VecDeque::new(),
            requests: HashMap::new(),
            waiters: HashMap::new(),
            deadlines: HashMap::new(),
            in_flight: None,
        }
    }
}

impl Drop for AcquisitionReservation<'_> {
    fn drop(&mut self) {
        self.store.cancel_acquisition(&self.request_id);
    }
}

struct PersonalAcquisitionReservation<'a> {
    store: &'a LocalContextStore,
    request_id: String,
}

impl Drop for PersonalAcquisitionReservation<'_> {
    fn drop(&mut self) {
        self.store.cancel_personal_acquisition(&self.request_id);
    }
}

const MAX_ACQUISITION_PENDING: usize = 16;
const MAX_ACQUISITION_CALENDARS: usize = 4;
const MAX_ACQUISITION_ITEMS: usize = 128;
const MAX_ACQUISITION_BYTES: usize = 65_536;
const MAX_ACQUISITION_DEADLINE_MS: i64 = 30_000;

#[derive(Clone)]
pub struct PublishedCalendarObservation {
    #[cfg(test)]
    pub(crate) connection_id: String,
    #[cfg(test)]
    pub(crate) source_authority: floe_context_contract::SourceAuthority,
    pub(crate) connection_revision: u64,
    pub(crate) provider: DomainCalendarProvider,
    pub(crate) calendar_ids: Vec<String>,
    pub(crate) observed_at_unix_ms: i64,
    pub(crate) expires_at_unix_ms: i64,
    pub(crate) range_start_unix_ms: i64,
    pub(crate) range_end_unix_ms: i64,
    pub(crate) batches: Vec<CalendarBatch>,
}

impl LocalContextStore {
    #[cfg(test)]
    pub(crate) fn request(
        &self,
        person_id: PersonId,
        operation: LocalContextOperationDto,
    ) -> BridgeResult<LocalContextResultDto> {
        self.request_bound(person_id, operation, None)
    }

    pub(crate) fn request_bound(
        &self,
        person_id: PersonId,
        operation: LocalContextOperationDto,
        connection: Option<&floe_day::CalendarConnection>,
    ) -> BridgeResult<LocalContextResultDto> {
        match operation {
            LocalContextOperationDto::RegisterAcquisitionHost { host_epoch } => {
                validate_host_epoch(&host_epoch)?;
                self.register_acquisition_host(person_id, host_epoch)?;
                Ok(result(person_id, None, None, 0, None))
            }
            LocalContextOperationDto::PollAcquisitions { host_epoch } => {
                validate_host_epoch(&host_epoch)?;
                let acquisitions = self.poll_acquisitions(person_id, &host_epoch)?;
                Ok(result_with_acquisitions(person_id, acquisitions))
            }
            LocalContextOperationDto::CompleteAcquisition {
                host_epoch,
                result: completion,
            } => {
                validate_host_epoch(&host_epoch)?;
                self.complete_acquisition(person_id, &host_epoch, completion)?;
                Ok(result(person_id, None, None, 0, None))
            }
            LocalContextOperationDto::FailAcquisition {
                host_epoch,
                request_id,
                failure,
            } => {
                validate_host_epoch(&host_epoch)?;
                validate_handle(&request_id, "operation.request_id")?;
                self.fail_acquisition(
                    person_id,
                    &host_epoch,
                    &request_id,
                    crate::conversion::calendar_failure_from_dto(failure),
                )?;
                Ok(result(person_id, None, None, 0, None))
            }
            LocalContextOperationDto::DisposeAcquisitionHost { host_epoch } => {
                validate_host_epoch(&host_epoch)?;
                self.dispose_acquisition_host(person_id, &host_epoch)?;
                Ok(result(person_id, None, None, 0, None))
            }
            LocalContextOperationDto::RegisterAttentionHost { host_epoch } => {
                validate_host_epoch(&host_epoch)?;
                self.register_attention_host(person_id, host_epoch)?;
                Ok(result(person_id, None, None, 0, None))
            }
            LocalContextOperationDto::PollAttentionAcquisitions { host_epoch } => {
                validate_host_epoch(&host_epoch)?;
                let acquisitions = self.poll_attention_acquisitions(person_id, &host_epoch)?;
                Ok(result_with_attention_acquisitions(person_id, acquisitions))
            }
            LocalContextOperationDto::CompleteAttentionAcquisition {
                host_epoch,
                result: attention_result,
            } => {
                validate_host_epoch(&host_epoch)?;
                self.complete_attention_acquisition(person_id, &host_epoch, attention_result)?;
                Ok(result(person_id, None, None, 0, None))
            }
            LocalContextOperationDto::FailAttentionAcquisition {
                host_epoch,
                request_id,
                failure,
            } => {
                validate_host_epoch(&host_epoch)?;
                validate_handle(&request_id, "operation.request_id")?;
                self.fail_attention_acquisition(person_id, &host_epoch, &request_id, &failure)?;
                Ok(result(person_id, None, None, 0, None))
            }
            LocalContextOperationDto::DisposeAttentionHost { host_epoch } => {
                validate_host_epoch(&host_epoch)?;
                self.dispose_attention_host(person_id, &host_epoch)?;
                Ok(result(person_id, None, None, 0, None))
            }
            LocalContextOperationDto::RegisterPersonalHost { host_epoch } => {
                validate_host_epoch(&host_epoch)?;
                self.register_personal_host(person_id, host_epoch)?;
                Ok(result(person_id, None, None, 0, None))
            }
            LocalContextOperationDto::PollPersonalAcquisitions { host_epoch } => {
                validate_host_epoch(&host_epoch)?;
                let acquisitions = self.poll_personal_acquisitions(person_id, &host_epoch)?;
                Ok(result_with_personal_acquisitions(person_id, acquisitions))
            }
            LocalContextOperationDto::CompletePersonalAcquisition {
                host_epoch,
                result: completion,
            } => {
                validate_host_epoch(&host_epoch)?;
                self.complete_personal_acquisition(person_id, &host_epoch, completion)?;
                Ok(result(person_id, None, None, 0, None))
            }
            LocalContextOperationDto::FailPersonalAcquisition {
                host_epoch,
                request_id,
                failure,
            } => {
                validate_host_epoch(&host_epoch)?;
                validate_handle(&request_id, "operation.request_id")?;
                self.fail_personal_acquisition(person_id, &host_epoch, &request_id, &failure)?;
                Ok(result(person_id, None, None, 0, None))
            }
            LocalContextOperationDto::DisposePersonalHost { host_epoch } => {
                validate_host_epoch(&host_epoch)?;
                self.dispose_personal_host(person_id, &host_epoch)?;
                Ok(result(person_id, None, None, 0, None))
            }
            LocalContextOperationDto::Publish { device_id, view } => {
                validate_handle(&device_id, "operation.device_id")?;
                let view_id = view
                    .get("view_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("operation.view.view_id", "must be a string"))?
                    .to_owned();
                let received_wall = now_unix_ms()?;
                let (observed_at_unix_ms, expires_at_unix_ms) =
                    validate_view(&view_id, &view, received_wall).map_err(agent_failure)?;
                let monotonic_ttl = std::time::Duration::from_millis(
                    u64::try_from(expires_at_unix_ms.saturating_sub(received_wall))
                        .map_err(|_| agent_failure(AgentFailure::StaleContext))?,
                );
                if view_id == "attention.coarse"
                    && self
                        .entries
                        .lock()
                        .map_err(|_| agent_failure(AgentFailure::Interrupted))?
                        .get(&(person_id, device_id.clone(), view_id.clone()))
                        .is_some_and(|entry| entry.native_subject_fingerprint.is_some())
                {
                    return Ok(result(person_id, Some(device_id), Some(view_id), 0, None));
                }
                let entry = Entry {
                    device_id: device_id.clone(),
                    observed_at_unix_ms,
                    expires_at_unix_ms,
                    observed_monotonic: Instant::now(),
                    monotonic_ttl,
                    process_incarnation: self.process_incarnation,
                    observation_id: Uuid::new_v4(),
                    native_subject_fingerprint: None,
                    view,
                };
                self.entries
                    .lock()
                    .map_err(|_| agent_failure(AgentFailure::Interrupted))?
                    .insert((person_id, device_id.clone(), view_id.clone()), entry);
                Ok(result(person_id, Some(device_id), Some(view_id), 0, None))
            }
            LocalContextOperationDto::PublishCalendarObservation {
                device_id,
                connection_id,
                connection_revision,
                provider,
                calendar_ids,
                observed_at_unix_ms,
                expires_at_unix_ms,
                range_start_unix_ms,
                range_end_unix_ms,
                batches,
            } => {
                let provider = crate::conversion::calendar_provider_from_dto(provider);
                validate_handle(&device_id, "operation.device_id")?;
                validate_handle(&connection_id, "operation.connection_id")?;
                if !matches!(
                    provider,
                    DomainCalendarProvider::EventKit | DomainCalendarProvider::Android
                ) {
                    return Err(agent_failure(AgentFailure::InvalidInput));
                }
                let connection =
                    connection.ok_or_else(|| agent_failure(AgentFailure::CapabilityUnavailable))?;
                let batches = batches
                    .into_iter()
                    .map(|batch| {
                        Ok(CalendarBatch {
                            calendar_id: batch.calendar_id,
                            records: batch
                                .records
                                .into_iter()
                                .map(|record| {
                                    Ok(CalendarRecord {
                                        can_modify: record.can_modify,
                                        calendar_id: record.calendar_id,
                                        external_id: record.external_id,
                                        external_revision: record.external_revision,
                                        title: record.title,
                                        schedule: crate::conversion::event_schedule_from_dto(
                                            record.schedule,
                                        )
                                        .map_err(|_| AgentFailure::InvalidInput)?,
                                    })
                                })
                                .collect::<Result<Vec<_>, AgentFailure>>()?,
                            failure: batch
                                .failure
                                .map(crate::conversion::calendar_failure_from_dto),
                        })
                    })
                    .collect::<Result<Vec<_>, AgentFailure>>()
                    .map_err(agent_failure)?;
                let observation = PublishedCalendarObservation {
                    #[cfg(test)]
                    connection_id: connection_id.clone(),
                    #[cfg(test)]
                    source_authority: connection.source_authority,
                    connection_revision,
                    provider,
                    calendar_ids,
                    observed_at_unix_ms,
                    expires_at_unix_ms,
                    range_start_unix_ms,
                    range_end_unix_ms,
                    batches,
                };
                validate_calendar_observation(&observation, now_unix_ms()?)
                    .map_err(agent_failure)?;
                {
                    let mut expected_ids: Vec<_> = connection
                        .calendars
                        .iter()
                        .map(|calendar| calendar.calendar_id.clone())
                        .collect();
                    let mut actual_ids = observation.calendar_ids.clone();
                    expected_ids.sort();
                    actual_ids.sort();
                    if connection.disconnected
                        || connection.connection_id != connection_id
                        || connection.device_id != device_id
                        || connection.provider != provider
                        || connection.revision != connection_revision
                        || expected_ids != actual_ids
                    {
                        return Err(agent_failure(AgentFailure::StaleContext));
                    }
                }
                self.calendar_observations
                    .lock()
                    .map_err(|_| agent_failure(AgentFailure::Interrupted))?
                    .insert((person_id, device_id.clone()), observation);
                Ok(result(
                    person_id,
                    Some(device_id),
                    Some("calendar.timeline".into()),
                    0,
                    None,
                ))
            }
            LocalContextOperationDto::Read { view_id, device_id } => {
                validate_view_id(&view_id)?;
                if let Some(device_id) = device_id.as_deref() {
                    validate_handle(device_id, "operation.device_id")?;
                }
                let entry = self
                    .read_entry(person_id, &view_id, device_id.as_deref())
                    .map_err(agent_failure)?;
                Ok(result(
                    person_id,
                    Some(entry.device_id),
                    Some(view_id),
                    0,
                    Some(entry.view),
                ))
            }
            LocalContextOperationDto::Revoke { device_id, view_id } => {
                validate_handle(&device_id, "operation.device_id")?;
                if let Some(view_id) = view_id.as_deref() {
                    validate_view_id(view_id)?;
                }
                let mut entries = self
                    .entries
                    .lock()
                    .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
                let before = entries.len();
                entries.retain(|(person, device, id), entry| {
                    *person != person_id
                        || device != &device_id
                        || view_id.as_ref().is_some_and(|view_id| view_id != id)
                        || (id == "attention.coarse" && entry.native_subject_fingerprint.is_some())
                });
                let mut removed_count = before - entries.len();
                drop(entries);
                if view_id
                    .as_deref()
                    .is_none_or(|id| id == "calendar.timeline")
                {
                    removed_count += self
                        .calendar_observations
                        .lock()
                        .map_err(|_| agent_failure(AgentFailure::Interrupted))?
                        .remove(&(person_id, device_id.clone()))
                        .is_some() as usize;
                }
                Ok(result(
                    person_id,
                    Some(device_id),
                    view_id,
                    removed_count,
                    None,
                ))
            }
        }
    }

    pub(crate) async fn acquire_calendar(
        &self,
        request: LocalContextAcquisitionRequestDto,
        cancellation: Cancellation,
    ) -> Result<LocalContextAcquisitionResultDto, AgentFailure> {
        validate_acquisition_request(&request).map_err(|_| AgentFailure::InvalidInput)?;
        let deadline = request.deadline_unix_ms;
        let request_id = request.request_id.clone();
        let monotonic_now = Instant::now();
        let wall_now = now_unix_ms().map_err(|_| AgentFailure::StaleContext)?;
        if deadline <= wall_now {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let duration = std::time::Duration::from_millis(
            u64::try_from(deadline - wall_now).map_err(|_| AgentFailure::DeadlineExceeded)?,
        );
        if duration > std::time::Duration::from_millis(MAX_ACQUISITION_DEADLINE_MS as u64) {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let monotonic_deadline = monotonic_now + duration;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        {
            let mut state = self
                .acquisition
                .lock()
                .map_err(|_| AgentFailure::Interrupted)?;
            if state.host_epoch.as_deref() != Some(request.host_epoch.as_str()) {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            let request_person = request
                .person_id
                .parse::<Uuid>()
                .map(PersonId)
                .map_err(|_| AgentFailure::InvalidInput)?;
            if state.host_person != Some(request_person) {
                return Err(AgentFailure::CapabilityDenied);
            }
            if state.requests.len() >= MAX_ACQUISITION_PENDING {
                return Err(AgentFailure::BudgetExceeded);
            }
            if state.requests.contains_key(&request_id) {
                return Err(AgentFailure::InvalidInput);
            }
            state.pending.push_back(request_id.clone());
            state.requests.insert(request_id.clone(), request);
            state.waiters.insert(request_id.clone(), sender);
            state
                .deadlines
                .insert(request_id.clone(), monotonic_deadline);
        }
        let _reservation = AcquisitionReservation {
            store: self,
            request_id: request_id.clone(),
        };
        let received = tokio::select! {
            result = receiver => result.map_err(|_| AgentFailure::Interrupted)?,
            _ = cancellation.cancelled() => {
                self.cancel_acquisition(&request_id);
                return Err(AgentFailure::Cancelled);
            }
            _ = tokio::time::sleep_until(monotonic_deadline) => {
                self.cancel_acquisition(&request_id);
                return Err(AgentFailure::DeadlineExceeded);
            }
        };
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= monotonic_deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        received
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) async fn inspect_calendar_subject(
        &self,
        mut request: LocalContextAcquisitionRequestDto,
        cancellation: Cancellation,
    ) -> Result<LocalContextAcquisitionResultDto, AgentFailure> {
        request.mode = LocalContextAcquisitionModeDto::InspectSubject;
        request.expected_native_subject_fingerprint = None;
        self.acquire_calendar(request, cancellation).await
    }

    pub(crate) async fn acquire_attention(
        &self,
        request: LocalContextAttentionAcquisitionRequestDto,
        cancellation: Cancellation,
    ) -> Result<LocalContextAttentionAcquisitionResultDto, AgentFailure> {
        validate_attention_acquisition_request(&request).map_err(|_| AgentFailure::InvalidInput)?;
        let request_id = request.request_id.clone();
        let wall_now = now_unix_ms().map_err(|_| AgentFailure::StaleContext)?;
        if request.deadline_unix_ms <= wall_now {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let duration = std::time::Duration::from_millis(
            u64::try_from(request.deadline_unix_ms - wall_now)
                .map_err(|_| AgentFailure::DeadlineExceeded)?,
        );
        if duration > std::time::Duration::from_millis(MAX_ACQUISITION_DEADLINE_MS as u64) {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let monotonic_deadline = Instant::now() + duration;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        {
            let mut state = self
                .attention_acquisition
                .lock()
                .map_err(|_| AgentFailure::Interrupted)?;
            let request_person = request
                .person_id
                .parse::<Uuid>()
                .map(PersonId)
                .map_err(|_| AgentFailure::InvalidInput)?;
            if state.host_epoch.as_deref() != Some(request.host_epoch.as_str()) {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            if state.host_person != Some(request_person) {
                return Err(AgentFailure::CapabilityDenied);
            }
            if state.requests.len() >= MAX_ACQUISITION_PENDING {
                return Err(AgentFailure::BudgetExceeded);
            }
            if state.requests.contains_key(&request_id) {
                return Err(AgentFailure::InvalidInput);
            }
            state.pending.push_back(request_id.clone());
            state.requests.insert(request_id.clone(), request);
            state.waiters.insert(request_id.clone(), sender);
            state
                .deadlines
                .insert(request_id.clone(), monotonic_deadline);
        }
        let _reservation = AttentionAcquisitionReservation {
            store: self,
            request_id: request_id.clone(),
        };
        let received = tokio::select! {
            result = receiver => result.map_err(|_| AgentFailure::Interrupted)?,
            _ = cancellation.cancelled() => {
                self.cancel_attention_acquisition(&request_id);
                return Err(AgentFailure::Cancelled);
            }
            _ = tokio::time::sleep_until(monotonic_deadline) => {
                self.cancel_attention_acquisition(&request_id);
                return Err(AgentFailure::DeadlineExceeded);
            }
        };
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= monotonic_deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        received
    }

    pub(crate) fn attention_acquisition_host_epoch(
        &self,
        person_id: PersonId,
    ) -> Result<String, AgentFailure> {
        let state = self
            .attention_acquisition
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if state.host_person != Some(person_id) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        state
            .host_epoch
            .clone()
            .ok_or(AgentFailure::CapabilityUnavailable)
    }

    pub(crate) fn process_incarnation(&self) -> Uuid {
        self.process_incarnation
    }

    pub(crate) fn personal_acquisition_host_epoch(
        &self,
        person_id: PersonId,
    ) -> Result<String, AgentFailure> {
        let state = self
            .personal_acquisition
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if state.host_person != Some(person_id) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        state
            .host_epoch
            .clone()
            .ok_or(AgentFailure::CapabilityUnavailable)
    }

    pub(crate) fn commit_trusted_personal_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        observation_id: Uuid,
        process_incarnation: Uuid,
        native_subject_fingerprint: &str,
        observed_at_unix_ms: i64,
        expires_at_unix_ms: i64,
        query_fingerprint: Vec<u8>,
    ) -> Result<(), AgentFailure> {
        if expires_at_unix_ms <= observed_at_unix_ms
            || native_subject_fingerprint.len() != 64
            || !native_subject_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(AgentFailure::InvalidInput);
        }
        let host_epoch = self.personal_acquisition_host_epoch(person_id)?;
        let ttl = std::time::Duration::from_millis(
            u64::try_from(expires_at_unix_ms - observed_at_unix_ms)
                .map_err(|_| AgentFailure::InvalidInput)?,
        );
        let observation = TrustedPersonalObservation {
            person_id,
            device_id: device_id.to_owned(),
            host_epoch,
            observation_id,
            process_incarnation,
            native_subject_fingerprint: native_subject_fingerprint.to_owned(),
            observed_at_unix_ms,
            expires_at_unix_ms,
            query_fingerprint,
            observed_monotonic: Instant::now(),
            monotonic_ttl: ttl,
        };
        let mut values = self
            .trusted_personal_observations
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        values.retain(|item| {
            item.person_id != person_id
                || item.observation_id != observation_id
                || item.process_incarnation != process_incarnation
        });
        values.push_back(observation);
        while values.len() > 32 {
            values.pop_front();
        }
        Ok(())
    }

    pub(crate) fn trusted_personal_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        observation_id: Uuid,
        process_incarnation: Uuid,
    ) -> Result<TrustedPersonalObservation, AgentFailure> {
        let host_epoch = self.personal_acquisition_host_epoch(person_id)?;
        let now = chrono::Utc::now().timestamp_millis();
        let mut values = self
            .trusted_personal_observations
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        values.retain(|item| {
            item.expires_at_unix_ms > now && item.observed_monotonic.elapsed() < item.monotonic_ttl
        });
        values
            .iter()
            .find(|item| {
                item.person_id == person_id
                    && item.device_id == device_id
                    && item.host_epoch == host_epoch
                    && item.observation_id == observation_id
                    && item.process_incarnation == process_incarnation
            })
            .cloned()
            .ok_or(AgentFailure::StaleContext)
    }

    #[cfg(test)]
    pub(crate) fn commit_trusted_attention_projection(
        &self,
        person_id: PersonId,
        device_id: &str,
        view: &AttentionView,
        native_subject_fingerprint: &str,
    ) -> Result<(Uuid, Uuid), AgentFailure> {
        let host_epoch = self.attention_acquisition_host_epoch(person_id)?;
        self.commit_trusted_attention_projection_for_host(
            person_id,
            &host_epoch,
            device_id,
            view,
            native_subject_fingerprint,
        )
    }

    pub(crate) fn commit_trusted_attention_projection_for_host(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        device_id: &str,
        view: &AttentionView,
        native_subject_fingerprint: &str,
    ) -> Result<(Uuid, Uuid), AgentFailure> {
        {
            let state = self
                .attention_acquisition
                .lock()
                .map_err(|_| AgentFailure::Interrupted)?;
            if !host_epoch.is_empty()
                && (state.host_person != Some(person_id)
                    || state.host_epoch.as_deref() != Some(host_epoch))
            {
                return Err(AgentFailure::CapabilityUnavailable);
            }
        }
        if !valid_native_subject_fingerprint(native_subject_fingerprint) {
            return Err(AgentFailure::InvalidInput);
        }
        let now = now_unix_ms().map_err(|_| AgentFailure::StaleContext)?;
        validate_attention_view(view, now).map_err(|_| AgentFailure::InvalidInput)?;
        let ttl_ms = view
            .expires_at_unix_ms
            .checked_sub(now)
            .ok_or(AgentFailure::StaleContext)?;
        let observation_id = Uuid::new_v4();
        let process_incarnation = self.process_incarnation;
        let entry = Entry {
            device_id: device_id.to_owned(),
            observed_at_unix_ms: view.observed_at_unix_ms,
            expires_at_unix_ms: view.expires_at_unix_ms,
            observed_monotonic: Instant::now(),
            monotonic_ttl: std::time::Duration::from_millis(
                u64::try_from(ttl_ms).map_err(|_| AgentFailure::StaleContext)?,
            ),
            process_incarnation,
            observation_id,
            native_subject_fingerprint: Some(native_subject_fingerprint.to_owned()),
            view: serde_json::to_value(view).map_err(|_| AgentFailure::InvalidInput)?,
        };
        self.entries
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?
            .insert(
                (person_id, device_id.to_owned(), view.view_id.clone()),
                entry,
            );
        let mut observations = self
            .trusted_attention_observations
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        observations.push_back(TrustedAttentionObservation {
            person_id,
            device_id: device_id.to_owned(),
            host_epoch: host_epoch.to_owned(),
            observation_id,
            process_incarnation,
            native_subject_fingerprint: native_subject_fingerprint.to_owned(),
            view: view.clone(),
            observed_monotonic: Instant::now(),
            monotonic_ttl: std::time::Duration::from_millis(
                u64::try_from(ttl_ms).map_err(|_| AgentFailure::StaleContext)?,
            ),
        });
        while observations.len() > 16 {
            observations.pop_front();
        }
        Ok((observation_id, process_incarnation))
    }

    pub(crate) fn trusted_attention_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        observation_id: Uuid,
        process_incarnation: Uuid,
    ) -> Result<(AttentionView, String), AgentFailure> {
        let host_epoch = self
            .attention_acquisition
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?
            .host_epoch
            .clone()
            .ok_or(AgentFailure::PolicyDenied)?;
        let now = now_unix_ms().map_err(|_| AgentFailure::StaleContext)?;
        let mut observations = self
            .trusted_attention_observations
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        observations.retain(|observation| {
            observation.view.expires_at_unix_ms > now
                && observation.observed_monotonic.elapsed() < observation.monotonic_ttl
        });
        observations
            .iter()
            .rev()
            .find(|observation| {
                observation.person_id == person_id
                    && observation.device_id == device_id
                    && observation.host_epoch == host_epoch
                    && observation.observation_id == observation_id
                    && observation.process_incarnation == process_incarnation
            })
            .map(|observation| {
                (
                    observation.view.clone(),
                    observation.native_subject_fingerprint.clone(),
                )
            })
            .ok_or(AgentFailure::PolicyDenied)
    }

    #[cfg(test)]
    pub(crate) fn trusted_attention_subject(
        &self,
        person_id: PersonId,
        device_id: &str,
    ) -> Result<String, AgentFailure> {
        let entry = self.read_entry(person_id, "attention.coarse", Some(device_id))?;
        entry
            .native_subject_fingerprint
            .ok_or(AgentFailure::AccessReviewRequired)
    }

    fn register_attention_host(&self, person_id: PersonId, host_epoch: String) -> BridgeResult<()> {
        let mut state = self
            .attention_acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_person.is_some_and(|owner| owner != person_id) {
            return Err(agent_failure(AgentFailure::CapabilityDenied));
        }
        if state.host_epoch.as_deref() == Some(host_epoch.as_str()) {
            return Ok(());
        }
        for (_, waiter) in state.waiters.drain() {
            let _ = waiter.send(Err(AgentFailure::Interrupted));
        }
        state.pending.clear();
        state.requests.clear();
        state.deadlines.clear();
        state.in_flight = None;
        self.invalidate_trusted_attention(person_id)
            .map_err(agent_failure)?;
        state.host_epoch = Some(host_epoch);
        state.host_person = Some(person_id);
        Ok(())
    }

    fn poll_attention_acquisitions(
        &self,
        person_id: PersonId,
        host_epoch: &str,
    ) -> BridgeResult<Vec<LocalContextAttentionAcquisitionRequestDto>> {
        let mut state = self
            .attention_acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_epoch.as_deref() != Some(host_epoch) || state.host_person != Some(person_id) {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        if state.in_flight.is_some() {
            return Ok(Vec::new());
        }
        while let Some(request_id) = state.pending.pop_front() {
            if let Some(request) = state.requests.get(&request_id).cloned() {
                if state
                    .deadlines
                    .get(&request_id)
                    .is_none_or(|deadline| *deadline <= Instant::now())
                {
                    state.requests.remove(&request_id);
                    state.deadlines.remove(&request_id);
                    if let Some(waiter) = state.waiters.remove(&request_id) {
                        let _ = waiter.send(Err(AgentFailure::DeadlineExceeded));
                    }
                    continue;
                }
                state.in_flight = Some(request_id);
                return Ok(vec![request]);
            }
        }
        Ok(Vec::new())
    }

    fn complete_attention_acquisition(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        result: LocalContextAttentionAcquisitionResultDto,
    ) -> BridgeResult<()> {
        validate_attention_acquisition_result(&result)
            .map_err(|_| invalid("operation.result", "invalid attention result"))?;
        let mut state = self
            .attention_acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_epoch.as_deref() != Some(host_epoch)
            || state.host_person != Some(person_id)
            || state.in_flight.as_deref() != Some(result.request_id.as_str())
        {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        if state
            .deadlines
            .get(&result.request_id)
            .is_some_and(|deadline| *deadline <= Instant::now())
        {
            self.reject_attention_locked(
                &mut state,
                &result.request_id,
                AgentFailure::DeadlineExceeded,
            );
            return Err(agent_failure(AgentFailure::DeadlineExceeded));
        }
        let request = state
            .requests
            .get(&result.request_id)
            .cloned()
            .ok_or_else(|| agent_failure(AgentFailure::StaleContext))?;
        if request.person_id != result.person_id
            || request.device_id != result.device_id
            || request.host_epoch != result.host_epoch
            || request.mode != result.mode
            || (request.mode == LocalContextAttentionAcquisitionModeDto::ReadProjection
                && request.expected_native_subject_fingerprint.as_deref()
                    != Some(result.native_subject_fingerprint_before.as_str()))
        {
            self.reject_attention_locked(
                &mut state,
                &result.request_id,
                AgentFailure::StaleContext,
            );
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        let waiter = state.waiters.remove(&result.request_id);
        state.requests.remove(&result.request_id);
        state.deadlines.remove(&result.request_id);
        state.in_flight = None;
        if let Some(waiter) = waiter {
            let _ = waiter.send(Ok(result));
        }
        Ok(())
    }

    fn fail_attention_acquisition(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        request_id: &str,
        failure: &str,
    ) -> BridgeResult<()> {
        let failure = match failure {
            "permission_denied" => AgentFailure::PolicyDenied,
            "attention_unavailable" => AgentFailure::CapabilityUnavailable,
            "provider_unavailable" => AgentFailure::CapabilityUnavailable,
            "cancelled" => AgentFailure::Cancelled,
            _ => return Err(invalid("operation.failure", "unsupported failure")),
        };
        let mut state = self
            .attention_acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_epoch.as_deref() != Some(host_epoch)
            || state.host_person != Some(person_id)
            || state.in_flight.as_deref() != Some(request_id)
        {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        self.reject_attention_locked(&mut state, request_id, failure);
        Ok(())
    }

    fn reject_attention_locked(
        &self,
        state: &mut AttentionAcquisitionState,
        request_id: &str,
        failure: AgentFailure,
    ) {
        state.requests.remove(request_id);
        state.deadlines.remove(request_id);
        state.in_flight = None;
        if let Some(waiter) = state.waiters.remove(request_id) {
            let _ = waiter.send(Err(failure));
        }
    }

    fn cancel_attention_acquisition(&self, request_id: &str) {
        if let Ok(mut state) = self.attention_acquisition.lock() {
            self.reject_attention_locked(&mut state, request_id, AgentFailure::Cancelled);
        }
    }

    fn dispose_attention_host(&self, person_id: PersonId, host_epoch: &str) -> BridgeResult<()> {
        let mut state = self
            .attention_acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_person != Some(person_id) || state.host_epoch.as_deref() != Some(host_epoch) {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        for (_, waiter) in state.waiters.drain() {
            let _ = waiter.send(Err(AgentFailure::Interrupted));
        }
        state.pending.clear();
        state.requests.clear();
        state.deadlines.clear();
        state.in_flight = None;
        state.host_epoch = None;
        state.host_person = None;
        self.invalidate_trusted_attention(person_id)
            .map_err(agent_failure)?;
        Ok(())
    }

    pub(crate) async fn acquire_personal(
        &self,
        request: LocalContextPersonalAcquisitionRequestDto,
        cancellation: Cancellation,
    ) -> Result<LocalContextPersonalAcquisitionResultDto, AgentFailure> {
        validate_personal_acquisition_request(&request).map_err(|_| AgentFailure::InvalidInput)?;
        let wall_now = now_unix_ms().map_err(|_| AgentFailure::StaleContext)?;
        if request.deadline_unix_ms <= wall_now {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let duration = std::time::Duration::from_millis(
            u64::try_from(request.deadline_unix_ms - wall_now)
                .map_err(|_| AgentFailure::DeadlineExceeded)?,
        );
        if duration > std::time::Duration::from_millis(MAX_ACQUISITION_DEADLINE_MS as u64) {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let deadline = Instant::now() + duration;
        let request_id = request.request_id.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        {
            let mut state = self
                .personal_acquisition
                .lock()
                .map_err(|_| AgentFailure::Interrupted)?;
            if state.host_epoch.as_deref() != Some(request.host_epoch.as_str()) {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            let request_person = request
                .person_id
                .parse::<Uuid>()
                .map(PersonId)
                .map_err(|_| AgentFailure::InvalidInput)?;
            if state.host_person != Some(request_person) {
                return Err(AgentFailure::CapabilityDenied);
            }
            if state.requests.len() >= MAX_ACQUISITION_PENDING
                || state.requests.contains_key(&request_id)
            {
                return Err(if state.requests.len() >= MAX_ACQUISITION_PENDING {
                    AgentFailure::BudgetExceeded
                } else {
                    AgentFailure::InvalidInput
                });
            }
            state.pending.push_back(request_id.clone());
            state.requests.insert(request_id.clone(), request);
            state.waiters.insert(request_id.clone(), sender);
            state.deadlines.insert(request_id.clone(), deadline);
        }
        let _reservation = PersonalAcquisitionReservation {
            store: self,
            request_id: request_id.clone(),
        };
        tokio::select! {
            result = receiver => result.map_err(|_| AgentFailure::Interrupted)?,
            _ = cancellation.cancelled() => {
                self.cancel_personal_acquisition(&request_id);
                Err(AgentFailure::Cancelled)
            }
            _ = tokio::time::sleep_until(deadline) => {
                self.cancel_personal_acquisition(&request_id);
                Err(AgentFailure::DeadlineExceeded)
            }
        }
    }

    fn register_personal_host(&self, person_id: PersonId, host_epoch: String) -> BridgeResult<()> {
        let mut state = self
            .personal_acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_person.is_some_and(|owner| owner != person_id) {
            return Err(agent_failure(AgentFailure::CapabilityDenied));
        }
        if state.host_epoch.as_deref() == Some(host_epoch.as_str()) {
            return Ok(());
        }
        for (_, waiter) in state.waiters.drain() {
            let _ = waiter.send(Err(AgentFailure::Interrupted));
        }
        state.pending.clear();
        state.requests.clear();
        state.deadlines.clear();
        state.in_flight = None;
        state.host_epoch = Some(host_epoch);
        state.host_person = Some(person_id);
        Ok(())
    }

    fn poll_personal_acquisitions(
        &self,
        person_id: PersonId,
        host_epoch: &str,
    ) -> BridgeResult<Vec<LocalContextPersonalAcquisitionRequestDto>> {
        let mut state = self
            .personal_acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_epoch.as_deref() != Some(host_epoch) || state.host_person != Some(person_id) {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        if state.in_flight.is_some() {
            return Ok(Vec::new());
        }
        while let Some(request_id) = state.pending.pop_front() {
            if let Some(request) = state.requests.get(&request_id).cloned() {
                if state
                    .deadlines
                    .get(&request_id)
                    .is_none_or(|deadline| *deadline <= Instant::now())
                {
                    state.requests.remove(&request_id);
                    state.deadlines.remove(&request_id);
                    if let Some(waiter) = state.waiters.remove(&request_id) {
                        let _ = waiter.send(Err(AgentFailure::DeadlineExceeded));
                    }
                    continue;
                }
                state.in_flight = Some(request_id);
                return Ok(vec![request]);
            }
        }
        Ok(Vec::new())
    }

    fn complete_personal_acquisition(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        result: LocalContextPersonalAcquisitionResultDto,
    ) -> BridgeResult<()> {
        validate_personal_acquisition_result(&result)?;
        let mut state = self
            .personal_acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_epoch.as_deref() != Some(host_epoch)
            || state.host_person != Some(person_id)
            || state.in_flight.as_deref() != Some(result.request_id.as_str())
        {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        if state
            .deadlines
            .get(&result.request_id)
            .is_none_or(|deadline| *deadline <= Instant::now())
        {
            self.reject_personal_locked(
                &mut state,
                &result.request_id,
                AgentFailure::DeadlineExceeded,
            );
            return Err(agent_failure(AgentFailure::DeadlineExceeded));
        }
        let request = state
            .requests
            .get(&result.request_id)
            .ok_or_else(|| agent_failure(AgentFailure::StaleContext))?;
        if request.host_epoch != result.host_epoch
            || request.person_id != result.person_id
            || request.device_id != result.device_id
            || request.domain != result.domain
            || result.native_subject_fingerprint_before != result.native_subject_fingerprint_after
            || request
                .expected_native_subject_fingerprint
                .as_deref()
                .is_some_and(|expected| expected != result.native_subject_fingerprint_before)
        {
            self.reject_personal_locked(
                &mut state,
                &result.request_id,
                AgentFailure::AccessReviewRequired,
            );
            return Err(agent_failure(AgentFailure::AccessReviewRequired));
        }
        let waiter = state.waiters.remove(&result.request_id);
        state.requests.remove(&result.request_id);
        state.deadlines.remove(&result.request_id);
        state.in_flight = None;
        if let Some(waiter) = waiter {
            let _ = waiter.send(Ok(result));
        }
        Ok(())
    }

    fn fail_personal_acquisition(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        request_id: &str,
        failure: &str,
    ) -> BridgeResult<()> {
        let mut state = self
            .personal_acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_epoch.as_deref() != Some(host_epoch)
            || state.host_person != Some(person_id)
            || state.in_flight.as_deref() != Some(request_id)
        {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        let code = match failure {
            "permission_denied" => AgentFailure::CapabilityDenied,
            "cancelled" => AgentFailure::Cancelled,
            _ => AgentFailure::CapabilityUnavailable,
        };
        self.reject_personal_locked(&mut state, request_id, code);
        Ok(())
    }

    fn reject_personal_locked(
        &self,
        state: &mut PersonalAcquisitionState,
        request_id: &str,
        failure: AgentFailure,
    ) {
        state.requests.remove(request_id);
        state.deadlines.remove(request_id);
        state.in_flight = None;
        if let Some(waiter) = state.waiters.remove(request_id) {
            let _ = waiter.send(Err(failure));
        }
    }

    fn cancel_personal_acquisition(&self, request_id: &str) {
        if let Ok(mut state) = self.personal_acquisition.lock() {
            state.pending.retain(|value| value != request_id);
            if state.in_flight.as_deref() == Some(request_id) {
                state.in_flight = None;
            }
            state.requests.remove(request_id);
            state.deadlines.remove(request_id);
            if let Some(waiter) = state.waiters.remove(request_id) {
                let _ = waiter.send(Err(AgentFailure::Cancelled));
            }
        }
    }

    fn dispose_personal_host(&self, person_id: PersonId, host_epoch: &str) -> BridgeResult<()> {
        let mut state = self
            .personal_acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_epoch.as_deref() != Some(host_epoch) || state.host_person != Some(person_id) {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        for (_, waiter) in state.waiters.drain() {
            let _ = waiter.send(Err(AgentFailure::Interrupted));
        }
        state.pending.clear();
        state.requests.clear();
        state.deadlines.clear();
        state.in_flight = None;
        state.host_epoch = None;
        state.host_person = None;
        Ok(())
    }

    fn invalidate_trusted_attention(&self, person_id: PersonId) -> Result<(), AgentFailure> {
        self.trusted_attention_observations
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?
            .retain(|observation| observation.person_id != person_id);
        self.entries
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?
            .retain(|(entry_person, _, view_id), _| {
                *entry_person != person_id || view_id != "attention.coarse"
            });
        Ok(())
    }

    pub(crate) fn acquisition_host_epoch(
        &self,
        person_id: PersonId,
    ) -> Result<String, AgentFailure> {
        let state = self
            .acquisition
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if state.host_person != Some(person_id) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        state
            .host_epoch
            .clone()
            .ok_or(AgentFailure::CapabilityUnavailable)
    }

    fn register_acquisition_host(
        &self,
        person_id: PersonId,
        host_epoch: String,
    ) -> BridgeResult<()> {
        let mut state = self
            .acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_person.is_some_and(|owner| owner != person_id) {
            return Err(agent_failure(AgentFailure::CapabilityDenied));
        }
        if state.host_epoch.as_deref() == Some(host_epoch.as_str()) {
            if state.host_person != Some(person_id) {
                return Err(agent_failure(AgentFailure::CapabilityDenied));
            }
            return Ok(());
        }
        {
            for (_, waiter) in state.waiters.drain() {
                let _ = waiter.send(Err(AgentFailure::Interrupted));
            }
            state.pending.clear();
            state.requests.clear();
            state.deadlines.clear();
            state.in_flight = None;
            state.host_epoch = Some(host_epoch);
            state.host_person = Some(person_id);
        }
        Ok(())
    }

    fn poll_acquisitions(
        &self,
        person_id: PersonId,
        host_epoch: &str,
    ) -> BridgeResult<Vec<LocalContextAcquisitionRequestDto>> {
        let mut state = self
            .acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_epoch.as_deref() != Some(host_epoch) || state.host_person != Some(person_id) {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        if state.in_flight.is_some() {
            return Ok(Vec::new());
        }
        while let Some(request_id) = state.pending.pop_front() {
            if let Some(request) = state.requests.get(&request_id).cloned() {
                if state
                    .deadlines
                    .get(&request_id)
                    .is_none_or(|deadline| *deadline <= Instant::now())
                {
                    state.requests.remove(&request_id);
                    state.deadlines.remove(&request_id);
                    if let Some(waiter) = state.waiters.remove(&request_id) {
                        let _ = waiter.send(Err(AgentFailure::DeadlineExceeded));
                    }
                    continue;
                }
                state.in_flight = Some(request_id);
                return Ok(vec![request]);
            }
        }
        Ok(Vec::new())
    }

    fn complete_acquisition(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        result: LocalContextAcquisitionResultDto,
    ) -> BridgeResult<()> {
        if validate_acquisition_result(&result).is_err() {
            self.reject_acquisition(
                person_id,
                host_epoch,
                &result.request_id,
                AgentFailure::InvalidInput,
            );
            return Err(invalid(
                "operation.result",
                "acquisition result is outside bounds",
            ));
        }
        let mut state = self
            .acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_epoch.as_deref() != Some(host_epoch)
            || state.host_person != Some(person_id)
            || result.host_epoch != host_epoch
            || state.in_flight.as_deref() != Some(result.request_id.as_str())
        {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        if state
            .deadlines
            .get(&result.request_id)
            .is_some_and(|deadline| *deadline <= Instant::now())
        {
            state.requests.remove(&result.request_id);
            state.deadlines.remove(&result.request_id);
            state.in_flight = None;
            if let Some(waiter) = state.waiters.remove(&result.request_id) {
                let _ = waiter.send(Err(AgentFailure::DeadlineExceeded));
            }
            return Err(agent_failure(AgentFailure::DeadlineExceeded));
        }
        let Some(request) = state.requests.get(&result.request_id) else {
            return Err(agent_failure(AgentFailure::StaleContext));
        };
        if !acquisition_identity_matches(request, &result) {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        if result.mode == LocalContextAcquisitionModeDto::ReadEvents
            && request.expected_native_subject_fingerprint.as_deref()
                != Some(result.native_subject_fingerprint_before.as_str())
        {
            let request_id = result.request_id.clone();
            let failure = AgentFailure::AccessReviewRequired;
            let waiter = state.waiters.remove(&request_id);
            state.requests.remove(&request_id);
            state.in_flight = None;
            if let Some(waiter) = waiter {
                let _ = waiter.send(Err(failure));
            }
            return Err(agent_failure(AgentFailure::AccessReviewRequired));
        }
        if request
            .calendar_ids
            .iter()
            .any(|calendar_id| !result.available_calendar_ids.contains(calendar_id))
        {
            let request_id = result.request_id.clone();
            let waiter = state.waiters.remove(&request_id);
            state.requests.remove(&request_id);
            state.in_flight = None;
            if let Some(waiter) = waiter {
                let _ = waiter.send(Err(AgentFailure::StaleContext));
            }
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        let waiter = state.waiters.remove(&result.request_id);
        state.requests.remove(&result.request_id);
        state.deadlines.remove(&result.request_id);
        state.in_flight = None;
        if let Some(waiter) = waiter {
            let _ = waiter.send(Ok(result));
        }
        Ok(())
    }

    fn reject_acquisition(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        request_id: &str,
        failure: AgentFailure,
    ) {
        if let Ok(mut state) = self.acquisition.lock() {
            if state.host_epoch.as_deref() != Some(host_epoch)
                || state.host_person != Some(person_id)
                || state.in_flight.as_deref() != Some(request_id)
            {
                return;
            }
            state.requests.remove(request_id);
            state.deadlines.remove(request_id);
            state.in_flight = None;
            if let Some(waiter) = state.waiters.remove(request_id) {
                let _ = waiter.send(Err(failure));
            }
        }
    }

    fn fail_acquisition(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        request_id: &str,
        failure: CalendarFailure,
    ) -> BridgeResult<()> {
        let failure_kind = match failure {
            CalendarFailure::PermissionDenied => AgentFailure::CapabilityDenied,
            CalendarFailure::CalendarUnavailable | CalendarFailure::ProviderUnavailable => {
                AgentFailure::CapabilityUnavailable
            }
        };
        let mut state = self
            .acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_epoch.as_deref() != Some(host_epoch)
            || state.host_person != Some(person_id)
            || state.in_flight.as_deref() != Some(request_id)
        {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        state.requests.remove(request_id);
        state.deadlines.remove(request_id);
        state.in_flight = None;
        if let Some(waiter) = state.waiters.remove(request_id) {
            let _ = waiter.send(Err(failure_kind));
        }
        Ok(())
    }

    fn cancel_acquisition(&self, request_id: &str) {
        if let Ok(mut state) = self.acquisition.lock() {
            state.pending.retain(|value| value != request_id);
            state.requests.remove(request_id);
            if state.in_flight.as_deref() == Some(request_id) {
                state.in_flight = None;
            }
            state.deadlines.remove(request_id);
            if let Some(waiter) = state.waiters.remove(request_id) {
                let _ = waiter.send(Err(AgentFailure::Cancelled));
            }
        }
    }

    fn dispose_acquisition_host(&self, person_id: PersonId, host_epoch: &str) -> BridgeResult<()> {
        let mut state = self
            .acquisition
            .lock()
            .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
        if state.host_epoch.as_deref() != Some(host_epoch) || state.host_person != Some(person_id) {
            return Err(agent_failure(AgentFailure::StaleContext));
        }
        for (_, waiter) in state.waiters.drain() {
            let _ = waiter.send(Err(AgentFailure::Interrupted));
        }
        state.pending.clear();
        state.requests.clear();
        state.deadlines.clear();
        state.in_flight = None;
        state.host_epoch = None;
        state.host_person = None;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn attention(&self, person_id: PersonId) -> Result<AttentionView, AgentFailure> {
        self.read_typed(person_id, "attention.coarse")
    }

    pub(crate) fn attention_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
    ) -> Result<(AttentionView, Uuid, Uuid), AgentFailure> {
        let entry = self.read_entry(person_id, "attention.coarse", Some(device_id))?;
        let view = serde_json::from_value(entry.view).map_err(|_| AgentFailure::InvalidInput)?;
        Ok((view, entry.observation_id, entry.process_incarnation))
    }

    #[cfg(test)]
    pub(crate) fn authorized_calendar_observation(
        &self,
        person_id: PersonId,
        connection: &floe_day::CalendarConnection,
        calendar_ids: &[String],
    ) -> Result<PublishedCalendarObservation, AgentFailure> {
        let authority = connection.source_authority;
        if !authority.is_valid() {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let observations = self
            .calendar_observations
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        let observation = observations
            .get(&(person_id, connection.device_id.clone()))
            .filter(|observation| {
                observation.connection_id == connection.connection_id
                    && observation.source_authority == authority
                    && observation.provider == connection.provider
                    && calendar_ids
                        .iter()
                        .all(|identifier| observation.calendar_ids.contains(identifier))
            })
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        validate_calendar_observation(
            observation,
            now_unix_ms().map_err(|_| AgentFailure::StaleContext)?,
        )?;
        let mut projected = observation.clone();
        projected.calendar_ids = calendar_ids.to_vec();
        projected
            .batches
            .retain(|batch| calendar_ids.contains(&batch.calendar_id));
        Ok(projected)
    }

    #[cfg(test)]
    fn read_typed<T: DeserializeOwned>(
        &self,
        person_id: PersonId,
        view_id: &str,
    ) -> Result<T, AgentFailure> {
        serde_json::from_value(self.read_entry(person_id, view_id, None)?.view)
            .map_err(|_| AgentFailure::InvalidInput)
    }

    fn read_entry(
        &self,
        person_id: PersonId,
        view_id: &str,
        device_id: Option<&str>,
    ) -> Result<Entry, AgentFailure> {
        let now = now_unix_ms().map_err(|_| AgentFailure::StaleContext)?;
        let mut entries = self.entries.lock().map_err(|_| AgentFailure::Interrupted)?;
        entries.retain(|_, entry| {
            entry.expires_at_unix_ms > now
                && entry.observed_monotonic.elapsed().as_millis() < entry.monotonic_ttl.as_millis()
        });
        entries
            .iter()
            .filter(|((person, device, id), _)| {
                *person == person_id
                    && id == view_id
                    && device_id.is_none_or(|expected| expected == device)
            })
            .map(|(_, entry)| entry)
            .max_by_key(|entry| entry.observed_at_unix_ms)
            .cloned()
            .ok_or(AgentFailure::CapabilityUnavailable)
    }
}

fn validate_view(
    view_id: &str,
    value: &Value,
    now_unix_ms: i64,
) -> Result<(i64, i64), AgentFailure> {
    macro_rules! parse {
        ($type:ty, $validate:path) => {{
            let view: $type =
                serde_json::from_value(value.clone()).map_err(|_| AgentFailure::InvalidInput)?;
            $validate(&view, now_unix_ms)?;
            (view.observed_at_unix_ms, view.expires_at_unix_ms)
        }};
    }
    Ok(match view_id {
        "people.identity" => parse!(PeopleView, validate_people_view),
        "schedule.feasibility" => parse!(FeasibilityView, validate_feasibility_view),
        "attention.coarse" => parse!(AttentionView, validate_attention_view),
        "wellbeing.derived" => parse!(WellbeingView, validate_wellbeing_view),
        _ => return Err(AgentFailure::CapabilityDenied),
    })
}

fn validate_view_id(view_id: &str) -> BridgeResult<()> {
    if ALLOWED_VIEW_IDS.contains(&view_id) {
        Ok(())
    } else {
        Err(invalid(
            "operation.view_id",
            "unsupported local context View",
        ))
    }
}

fn validate_calendar_observation(
    observation: &PublishedCalendarObservation,
    now_unix_ms: i64,
) -> Result<(), AgentFailure> {
    let identifiers: std::collections::HashSet<_> = observation.calendar_ids.iter().collect();
    let batch_ids: std::collections::HashSet<_> = observation
        .batches
        .iter()
        .map(|batch| batch.calendar_id.as_str())
        .collect();
    if !matches!(
        observation.provider,
        DomainCalendarProvider::EventKit | DomainCalendarProvider::Android
    ) || observation.connection_revision == 0
        || observation.calendar_ids.is_empty()
        || observation.calendar_ids.len() > 128
        || observation
            .batches
            .iter()
            .map(|batch| batch.records.len())
            .sum::<usize>()
            > 10_000
        || identifiers.len() != observation.calendar_ids.len()
        || observation
            .calendar_ids
            .iter()
            .any(|id| id.trim().is_empty() || id.len() > 512)
        || observation.batches.len() != observation.calendar_ids.len()
        || batch_ids.len() != observation.batches.len()
        || !observation
            .calendar_ids
            .iter()
            .all(|id| batch_ids.contains(id.as_str()))
        || observation.observed_at_unix_ms > now_unix_ms
        || observation.expires_at_unix_ms <= now_unix_ms
        || observation.expires_at_unix_ms <= observation.observed_at_unix_ms
        || observation.expires_at_unix_ms - observation.observed_at_unix_ms > 300_000
        || observation.range_start_unix_ms < 0
        || observation.range_end_unix_ms <= observation.range_start_unix_ms
        || observation.range_end_unix_ms - observation.range_start_unix_ms > 32 * 86_400_000
    {
        return Err(AgentFailure::InvalidInput);
    }
    for batch in &observation.batches {
        if (batch.failure.is_some() && !batch.records.is_empty())
            || batch.records.len() > 10_000
            || batch.records.iter().any(|record| {
                record.calendar_id != batch.calendar_id
                    || record.external_id.trim().is_empty()
                    || record.external_id.len() > 512
                    || record.external_revision.trim().is_empty()
                    || record.external_revision.len() > 512
                    || record.title.len() > 4096
            })
        {
            return Err(AgentFailure::InvalidInput);
        }
    }
    Ok(())
}

fn validate_handle(value: &str, field: &'static str) -> BridgeResult<()> {
    if !value.trim().is_empty() && value.len() <= 128 {
        Ok(())
    } else {
        Err(invalid(field, "must contain 1 to 128 characters"))
    }
}

fn now_unix_ms() -> BridgeResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| agent_failure(AgentFailure::StaleContext))?;
    i64::try_from(duration.as_millis()).map_err(|_| agent_failure(AgentFailure::StaleContext))
}

fn result(
    person_id: PersonId,
    device_id: Option<String>,
    view_id: Option<String>,
    removed_count: usize,
    view: Option<Value>,
) -> LocalContextResultDto {
    LocalContextResultDto {
        person_id: person_id.0.to_string(),
        device_id,
        view_id,
        removed_count,
        view,
        acquisitions: Vec::new(),
        attention_acquisitions: Vec::new(),
        personal_acquisitions: Vec::new(),
    }
}

fn result_with_acquisitions(
    person_id: PersonId,
    acquisitions: Vec<LocalContextAcquisitionRequestDto>,
) -> LocalContextResultDto {
    LocalContextResultDto {
        person_id: person_id.0.to_string(),
        device_id: None,
        view_id: None,
        removed_count: 0,
        view: None,
        acquisitions,
        attention_acquisitions: Vec::new(),
        personal_acquisitions: Vec::new(),
    }
}

fn result_with_attention_acquisitions(
    person_id: PersonId,
    acquisitions: Vec<LocalContextAttentionAcquisitionRequestDto>,
) -> LocalContextResultDto {
    LocalContextResultDto {
        person_id: person_id.0.to_string(),
        device_id: None,
        view_id: None,
        removed_count: 0,
        view: None,
        acquisitions: Vec::new(),
        attention_acquisitions: acquisitions,
        personal_acquisitions: Vec::new(),
    }
}

fn result_with_personal_acquisitions(
    person_id: PersonId,
    acquisitions: Vec<LocalContextPersonalAcquisitionRequestDto>,
) -> LocalContextResultDto {
    LocalContextResultDto {
        person_id: person_id.0.to_string(),
        device_id: None,
        view_id: None,
        removed_count: 0,
        view: None,
        acquisitions: Vec::new(),
        attention_acquisitions: Vec::new(),
        personal_acquisitions: acquisitions,
    }
}

fn validate_host_epoch(value: &str) -> BridgeResult<()> {
    if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(invalid("operation.host_epoch", "invalid host epoch"));
    }
    Ok(())
}

fn validate_attention_acquisition_request(
    request: &LocalContextAttentionAcquisitionRequestDto,
) -> BridgeResult<()> {
    validate_host_epoch(&request.host_epoch)?;
    validate_handle(&request.request_id, "operation.request_id")?;
    validate_handle(&request.device_id, "operation.device_id")?;
    if request.person_id.parse::<Uuid>().is_err()
        || request.deadline_unix_ms <= 0
        || request.mode == LocalContextAttentionAcquisitionModeDto::ReadProjection
            && !request
                .expected_native_subject_fingerprint
                .as_deref()
                .is_some_and(valid_native_subject_fingerprint)
    {
        return Err(invalid(
            "operation",
            "invalid attention acquisition request",
        ));
    }
    Ok(())
}

fn validate_attention_acquisition_result(
    result: &LocalContextAttentionAcquisitionResultDto,
) -> BridgeResult<()> {
    validate_host_epoch(&result.host_epoch)?;
    validate_handle(&result.request_id, "operation.result.request_id")?;
    validate_handle(&result.device_id, "operation.result.device_id")?;
    validate_fingerprint(
        &result.native_subject_fingerprint_before,
        "operation.result.native_subject_fingerprint_before",
    )?;
    validate_fingerprint(
        &result.native_subject_fingerprint_after,
        "operation.result.native_subject_fingerprint_after",
    )?;
    if result.native_subject_fingerprint_before != result.native_subject_fingerprint_after
        || result.person_id.parse::<Uuid>().is_err()
        || result.permission_class.is_empty()
        || result.permission_class.len() > 64
    {
        return Err(invalid("operation.result", "invalid attention evidence"));
    }
    if result.mode == LocalContextAttentionAcquisitionModeDto::ReadProjection {
        let view = result
            .view
            .as_ref()
            .ok_or_else(|| invalid("operation.result.view", "missing attention view"))?;
        let view: AttentionView = serde_json::from_value(view.clone())
            .map_err(|_| invalid("operation.result.view", "invalid attention view"))?;
        validate_attention_view(&view, now_unix_ms()?)
            .map_err(|_| invalid("operation.result.view", "invalid attention view"))?;
    } else if result.view.is_some() {
        return Err(invalid(
            "operation.result.view",
            "inspect cannot return data",
        ));
    }
    Ok(())
}

fn validate_personal_acquisition_request(
    request: &LocalContextPersonalAcquisitionRequestDto,
) -> BridgeResult<()> {
    validate_host_epoch(&request.host_epoch)?;
    validate_handle(&request.request_id, "operation.request_id")?;
    validate_handle(&request.device_id, "operation.device_id")?;
    if request.person_id.parse::<Uuid>().is_err()
        || request.deadline_unix_ms <= 0
        || request
            .expected_native_subject_fingerprint
            .as_deref()
            .is_none_or(|value| !valid_native_subject_fingerprint(value))
    {
        return Err(invalid("operation", "invalid personal acquisition request"));
    }
    match request.domain {
        LocalContextPersonalDomainDto::People => {
            if request.selected_handles.is_empty()
                || request.selected_handles.len() > 64
                || request
                    .selected_handles
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1])
                || request
                    .selected_handles
                    .iter()
                    .any(|value| validate_handle(value, "operation.selected_handles").is_err())
                || request.event_handle.is_some()
                || !request.evidence_handles.is_empty()
                || request.destination_latitude.is_some()
                || request.destination_longitude.is_some()
                || request.event_start_unix_ms.is_some()
                || request.event_end_unix_ms.is_some()
                || request.travel_mode.is_some()
            {
                return Err(invalid("operation", "invalid People acquisition request"));
            }
        }
        LocalContextPersonalDomainDto::Wellbeing => {
            if !request.selected_handles.is_empty()
                || request.event_handle.is_some()
                || !request.evidence_handles.is_empty()
                || request.destination_latitude.is_some()
                || request.destination_longitude.is_some()
                || request.event_start_unix_ms.is_some()
                || request.event_end_unix_ms.is_some()
                || request.travel_mode.is_some()
            {
                return Err(invalid(
                    "operation",
                    "invalid Wellbeing acquisition request",
                ));
            }
        }
        LocalContextPersonalDomainDto::Feasibility => {
            if !request.selected_handles.is_empty()
                || request
                    .event_handle
                    .as_deref()
                    .is_none_or(|value| validate_handle(value, "operation.event_handle").is_err())
                || request.evidence_handles.is_empty()
                || request.evidence_handles.len() > 8
                || request
                    .evidence_handles
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1])
                || request
                    .evidence_handles
                    .iter()
                    .any(|value| validate_handle(value, "operation.evidence_handles").is_err())
                || request.destination_latitude.is_none()
                || request.destination_longitude.is_none()
                || request.event_start_unix_ms.is_none()
                || request.event_end_unix_ms.is_none()
                || request
                    .travel_mode
                    .as_deref()
                    .is_none_or(|value| !matches!(value, "automobile" | "transit" | "walking"))
            {
                return Err(invalid(
                    "operation",
                    "invalid Feasibility acquisition request",
                ));
            }
            let latitude = request.destination_latitude.unwrap();
            let longitude = request.destination_longitude.unwrap();
            let start = request.event_start_unix_ms.unwrap();
            let end = request.event_end_unix_ms.unwrap();
            if !latitude.is_finite()
                || !longitude.is_finite()
                || !(-90.0..=90.0).contains(&latitude)
                || !(-180.0..=180.0).contains(&longitude)
                || start < 0
                || end <= start
                || end - start > 86_400_000
            {
                return Err(invalid("operation", "invalid Feasibility window"));
            }
        }
    }
    Ok(())
}

fn validate_personal_acquisition_result(
    result: &LocalContextPersonalAcquisitionResultDto,
) -> BridgeResult<()> {
    validate_host_epoch(&result.host_epoch)?;
    validate_handle(&result.request_id, "operation.result.request_id")?;
    validate_handle(&result.device_id, "operation.result.device_id")?;
    validate_handle(&result.provider, "operation.result.provider")?;
    validate_handle(
        &result.permission_class,
        "operation.result.permission_class",
    )?;
    validate_fingerprint(
        &result.native_subject_fingerprint_before,
        "operation.result.native_subject_fingerprint_before",
    )?;
    validate_fingerprint(
        &result.native_subject_fingerprint_after,
        "operation.result.native_subject_fingerprint_after",
    )?;
    if result.native_subject_fingerprint_before != result.native_subject_fingerprint_after {
        return Err(invalid("operation.result", "personal subject changed"));
    }
    if result.view.is_none() {
        return Err(invalid("operation.result.view", "missing personal view"));
    }
    let view_id = match result.domain {
        LocalContextPersonalDomainDto::People => "people.identity",
        LocalContextPersonalDomainDto::Wellbeing => "wellbeing.derived",
        LocalContextPersonalDomainDto::Feasibility => "schedule.feasibility",
    };
    validate_view(
        view_id,
        result.view.as_ref().unwrap(),
        now_unix_ms().map_err(|_| invalid("operation.result", "invalid current time"))?,
    )
    .map_err(|_| invalid("operation.result.view", "invalid personal view"))?;
    Ok(())
}

fn validate_fingerprint(value: &str, field: &'static str) -> BridgeResult<()> {
    if valid_native_subject_fingerprint(value) {
        Ok(())
    } else {
        Err(invalid(field, "invalid fingerprint"))
    }
}

fn validate_acquisition_request(request: &LocalContextAcquisitionRequestDto) -> BridgeResult<()> {
    validate_host_epoch(&request.host_epoch)?;
    validate_handle(&request.request_id, "operation.request_id")?;
    validate_handle(&request.person_id, "operation.person_id")?;
    validate_handle(&request.device_id, "operation.device_id")?;
    validate_handle(&request.connection_id, "operation.connection_id")?;
    let now = now_unix_ms()?;
    let fingerprint_valid = request
        .expected_native_subject_fingerprint
        .as_deref()
        .is_some_and(valid_native_subject_fingerprint);
    if request.connection_revision == 0
        || !matches!(
            request.provider,
            CalendarProviderDto::EventKit | CalendarProviderDto::Android
        )
        || request.calendar_ids.is_empty()
        || request.calendar_ids.len() > MAX_ACQUISITION_CALENDARS
        || request
            .calendar_ids
            .iter()
            .any(|id| id.trim().is_empty() || id.len() > 512)
        || request
            .calendar_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || request.range_start_unix_ms < 0
        || request.range_end_unix_ms <= request.range_start_unix_ms
        || request.range_end_unix_ms - request.range_start_unix_ms > 32 * 86_400_000
        || request.deadline_unix_ms <= request.range_start_unix_ms
        || request.deadline_unix_ms <= now
        || request.deadline_unix_ms > now.saturating_add(MAX_ACQUISITION_DEADLINE_MS)
        || (request.mode == LocalContextAcquisitionModeDto::ReadEvents && !fingerprint_valid)
        || (request.mode == LocalContextAcquisitionModeDto::InspectSubject
            && request.expected_native_subject_fingerprint.is_some())
    {
        return Err(invalid(
            "operation",
            "acquisition request is outside bounds",
        ));
    }
    Ok(())
}

fn acquisition_identity_matches(
    request: &LocalContextAcquisitionRequestDto,
    result: &LocalContextAcquisitionResultDto,
) -> bool {
    request.request_id == result.request_id
        && request.host_epoch == result.host_epoch
        && request.person_id == result.person_id
        && request.device_id == result.device_id
        && request.connection_id == result.connection_id
        && request.connection_revision == result.connection_revision
        && request.provider == result.provider
        && request.mode == result.mode
        && request.calendar_ids == result.calendar_ids
        && request.range_start_unix_ms == result.range_start_unix_ms
        && request.range_end_unix_ms == result.range_end_unix_ms
}

fn validate_acquisition_result(result: &LocalContextAcquisitionResultDto) -> BridgeResult<()> {
    if !valid_native_subject_fingerprint(&result.native_subject_fingerprint_before)
        || !valid_native_subject_fingerprint(&result.native_subject_fingerprint_after)
        || result.native_subject_fingerprint_before != result.native_subject_fingerprint_after
        || result.permission_class.trim().is_empty()
        || result.permission_class.len() > 64
        || result.permission_class.chars().any(char::is_control)
        || result.available_calendar_ids.is_empty()
        || result.available_calendar_ids.len() > 128
        || result
            .available_calendar_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || result
            .available_calendar_ids
            .iter()
            .any(|id| id.trim().is_empty() || id.len() > 512)
    {
        return Err(invalid(
            "operation.result",
            "native subject evidence is outside bounds",
        ));
    }
    let request = LocalContextAcquisitionRequestDto {
        request_id: result.request_id.clone(),
        host_epoch: result.host_epoch.clone(),
        person_id: result.person_id.clone(),
        device_id: result.device_id.clone(),
        connection_id: result.connection_id.clone(),
        connection_revision: result.connection_revision,
        provider: result.provider,
        mode: result.mode,
        calendar_ids: result.calendar_ids.clone(),
        range_start_unix_ms: result.range_start_unix_ms,
        range_end_unix_ms: result.range_end_unix_ms,
        deadline_unix_ms: now_unix_ms()?.saturating_add(MAX_ACQUISITION_DEADLINE_MS - 1_000),
        expected_native_subject_fingerprint: (result.mode
            == LocalContextAcquisitionModeDto::ReadEvents)
            .then(|| result.native_subject_fingerprint_before.clone()),
    };
    validate_acquisition_request(&request)?;
    if (result.mode == LocalContextAcquisitionModeDto::InspectSubject && !result.batches.is_empty())
        || (result.mode == LocalContextAcquisitionModeDto::ReadEvents
            && result.batches.len() != result.calendar_ids.len())
        || result
            .calendar_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || result.batches.iter().enumerate().any(|(index, batch)| {
            batch.calendar_id != result.calendar_ids[index]
                || (batch.failure.is_some() && !batch.records.is_empty())
                || batch.records.iter().any(|record| {
                    record.calendar_id != batch.calendar_id
                        || record.external_id.trim().is_empty()
                        || record.external_id.len() > 512
                        || record.external_revision.trim().is_empty()
                        || record.external_revision.len() > 512
                        || record.title.len() > 4096
                })
        })
        || result
            .batches
            .iter()
            .map(|batch| batch.records.len())
            .sum::<usize>()
            > MAX_ACQUISITION_ITEMS
        || serde_json::to_vec(result)
            .map_err(|_| invalid("operation.result", "invalid acquisition result"))?
            .len()
            > MAX_ACQUISITION_BYTES
    {
        return Err(invalid(
            "operation.result",
            "acquisition result is outside bounds",
        ));
    }
    Ok(())
}

fn valid_native_subject_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_protocol::{
        CalendarBatchDto, CalendarFailureDto, CalendarProviderDto, CalendarRecordDto,
        EventScheduleDto, PROTOCOL_VERSION,
    };
    use serde_json::json;
    use uuid::Uuid;

    fn person() -> PersonId {
        PersonId(Uuid::new_v4())
    }

    fn attention(now: i64, source: &str) -> Value {
        json!({
            "schema_version": PROTOCOL_VERSION,
            "view_id": "attention.coarse",
            "source_handle": source,
            "observed_at_unix_ms": now - 1,
            "expires_at_unix_ms": now + 60_000,
            "state": "focused",
            "confidence_millis": 800,
            "evidence_handles": ["activity:coarse"]
        })
    }

    fn calendar_publication(
        now: i64,
        device_id: &str,
        provider: CalendarProviderDto,
    ) -> LocalContextOperationDto {
        LocalContextOperationDto::PublishCalendarObservation {
            device_id: device_id.into(),
            connection_id: "test-connection".into(),
            connection_revision: 7,
            provider,
            calendar_ids: vec!["primary".into()],
            observed_at_unix_ms: now - 1,
            expires_at_unix_ms: now + 299_999,
            range_start_unix_ms: now - 60_000,
            range_end_unix_ms: now + 60_000,
            batches: vec![CalendarBatchDto {
                calendar_id: "primary".into(),
                records: vec![CalendarRecordDto {
                    can_modify: false,
                    calendar_id: "primary".into(),
                    external_id: "event-1".into(),
                    external_revision: "revision-1".into(),
                    title: "Review".into(),
                    schedule: EventScheduleDto::Timed {
                        starts_at: chrono::DateTime::from_timestamp_millis(now + 1_000)
                            .unwrap()
                            .to_rfc3339(),
                        ends_at: chrono::DateTime::from_timestamp_millis(now + 2_000)
                            .unwrap()
                            .to_rfc3339(),
                        timezone: "UTC".into(),
                    },
                }],
                failure: None,
            }],
        }
    }

    #[test]
    fn server_calendar_observation_cannot_be_published_as_device_context() {
        let store = LocalContextStore::default();
        let error = store
            .request(
                person(),
                calendar_publication(now_unix_ms().unwrap(), "mac", CalendarProviderDto::Google),
            )
            .unwrap_err();
        assert_eq!(error.code, floe_protocol::ErrorCodeDto::Validation);
        assert_eq!(
            error.metadata.get("agent_failure").map(String::as_str),
            Some("invalid_input")
        );
    }

    #[test]
    fn native_calendar_publication_requires_a_bound_connection() {
        let store = LocalContextStore::default();
        let error = store
            .request(
                person(),
                calendar_publication(now_unix_ms().unwrap(), "mac", CalendarProviderDto::EventKit),
            )
            .unwrap_err();
        assert_eq!(
            error.metadata.get("agent_failure").map(String::as_str),
            Some("capability_unavailable")
        );
        assert!(store.calendar_observations.lock().unwrap().is_empty());
    }

    fn acquisition_request(person_id: PersonId) -> LocalContextAcquisitionRequestDto {
        LocalContextAcquisitionRequestDto {
            request_id: "request-a".into(),
            host_epoch: "host-a".into(),
            person_id: person_id.0.to_string(),
            device_id: "device-a".into(),
            connection_id: "connection-a".into(),
            connection_revision: 2,
            provider: CalendarProviderDto::Android,
            mode: LocalContextAcquisitionModeDto::ReadEvents,
            calendar_ids: vec!["calendar-a".into()],
            range_start_unix_ms: 1_000,
            range_end_unix_ms: 2_000,
            deadline_unix_ms: now_unix_ms().unwrap() + 20_000,
            expected_native_subject_fingerprint: Some("a".repeat(64)),
        }
    }

    #[test]
    fn acquisition_request_rejects_duplicate_ids_and_long_deadlines() {
        let person_id = person();
        let mut request = acquisition_request(person_id);
        request.calendar_ids = vec!["calendar-a".into(), "calendar-a".into()];
        assert!(validate_acquisition_request(&request).is_err());
        request.calendar_ids = vec!["calendar-a".into()];
        request.deadline_unix_ms = now_unix_ms().unwrap() + MAX_ACQUISITION_DEADLINE_MS + 1;
        assert!(validate_acquisition_request(&request).is_err());
    }

    #[test]
    fn acquisition_host_is_bound_to_person_for_every_operation() {
        let store = LocalContextStore::default();
        let owner = person();
        let other = person();
        store
            .request(
                owner,
                LocalContextOperationDto::RegisterAcquisitionHost {
                    host_epoch: "host-a".into(),
                },
            )
            .unwrap();
        assert!(
            store
                .request(
                    other,
                    LocalContextOperationDto::RegisterAcquisitionHost {
                        host_epoch: "host-b".into(),
                    },
                )
                .is_err()
        );
        assert!(
            store
                .request(
                    other,
                    LocalContextOperationDto::PollAcquisitions {
                        host_epoch: "host-a".into(),
                    },
                )
                .is_err()
        );
        assert!(
            store
                .request(
                    other,
                    LocalContextOperationDto::DisposeAcquisitionHost {
                        host_epoch: "host-a".into(),
                    },
                )
                .is_err()
        );
        store
            .request(
                owner,
                LocalContextOperationDto::PollAcquisitions {
                    host_epoch: "host-a".into(),
                },
            )
            .unwrap();
    }

    #[tokio::test]
    async fn cancelled_acquisition_rejects_late_completion() {
        let store = std::sync::Arc::new(LocalContextStore::default());
        let person_id = person();
        store
            .request(
                person_id,
                LocalContextOperationDto::RegisterAcquisitionHost {
                    host_epoch: "host-a".into(),
                },
            )
            .unwrap();
        let request = acquisition_request(person_id);
        let cancellation = Cancellation::default();
        let waiter = tokio::spawn({
            let store = store.clone();
            let cancellation = cancellation.clone();
            let request = request.clone();
            async move { store.acquire_calendar(request, cancellation).await }
        });
        tokio::task::yield_now().await;
        let polled = store
            .request(
                person_id,
                LocalContextOperationDto::PollAcquisitions {
                    host_epoch: "host-a".into(),
                },
            )
            .unwrap();
        assert_eq!(polled.acquisitions, vec![request.clone()]);
        cancellation.cancel();
        assert_eq!(waiter.await.unwrap(), Err(AgentFailure::Cancelled));
        let result = LocalContextAcquisitionResultDto {
            request_id: request.request_id,
            host_epoch: request.host_epoch,
            person_id: request.person_id,
            device_id: request.device_id,
            connection_id: request.connection_id,
            connection_revision: request.connection_revision,
            provider: request.provider,
            mode: request.mode,
            calendar_ids: request.calendar_ids,
            range_start_unix_ms: request.range_start_unix_ms,
            range_end_unix_ms: request.range_end_unix_ms,
            native_subject_fingerprint_before: "a".repeat(64),
            native_subject_fingerprint_after: "a".repeat(64),
            available_calendar_ids: vec!["calendar-a".into()],
            permission_class: "authorized".into(),
            batches: vec![],
        };
        assert!(
            store
                .request(
                    person_id,
                    LocalContextOperationDto::CompleteAcquisition {
                        host_epoch: "host-a".into(),
                        result,
                    },
                )
                .is_err()
        );
    }

    #[tokio::test]
    async fn failed_acquisition_releases_in_flight_request() {
        let store = std::sync::Arc::new(LocalContextStore::default());
        let person_id = person();
        store
            .request(
                person_id,
                LocalContextOperationDto::RegisterAcquisitionHost {
                    host_epoch: "host-a".into(),
                },
            )
            .unwrap();
        let request = acquisition_request(person_id);
        let waiter = tokio::spawn({
            let store = store.clone();
            let request = request.clone();
            async move {
                store
                    .acquire_calendar(request, Cancellation::default())
                    .await
            }
        });
        tokio::task::yield_now().await;
        assert_eq!(
            store
                .request(
                    person_id,
                    LocalContextOperationDto::PollAcquisitions {
                        host_epoch: "host-a".into(),
                    },
                )
                .unwrap()
                .acquisitions
                .len(),
            1
        );
        store
            .request(
                person_id,
                LocalContextOperationDto::FailAcquisition {
                    host_epoch: "host-a".into(),
                    request_id: request.request_id,
                    failure: CalendarFailureDto::PermissionDenied,
                },
            )
            .unwrap();
        assert_eq!(waiter.await.unwrap(), Err(AgentFailure::CapabilityDenied));
        assert!(
            store
                .request(
                    person_id,
                    LocalContextOperationDto::PollAcquisitions {
                        host_epoch: "host-a".into(),
                    },
                )
                .unwrap()
                .acquisitions
                .is_empty()
        );
    }

    #[tokio::test]
    async fn acquisition_poll_complete_is_bounded_and_identity_fenced() {
        let store = std::sync::Arc::new(LocalContextStore::default());
        let person_id = person();
        store
            .request(
                person_id,
                LocalContextOperationDto::RegisterAcquisitionHost {
                    host_epoch: "host-a".into(),
                },
            )
            .unwrap();
        let request = acquisition_request(person_id);
        let waiter = tokio::spawn({
            let store = store.clone();
            let request = request.clone();
            async move {
                store
                    .acquire_calendar(request, Cancellation::default())
                    .await
            }
        });
        tokio::task::yield_now().await;
        let polled = store
            .request(
                person_id,
                LocalContextOperationDto::PollAcquisitions {
                    host_epoch: "host-a".into(),
                },
            )
            .unwrap();
        assert_eq!(polled.acquisitions, vec![request.clone()]);
        let mut completion = LocalContextAcquisitionResultDto {
            request_id: request.request_id.clone(),
            host_epoch: request.host_epoch.clone(),
            person_id: request.person_id.clone(),
            device_id: request.device_id.clone(),
            connection_id: request.connection_id.clone(),
            connection_revision: request.connection_revision,
            provider: request.provider,
            mode: request.mode,
            calendar_ids: request.calendar_ids.clone(),
            range_start_unix_ms: request.range_start_unix_ms,
            range_end_unix_ms: request.range_end_unix_ms,
            native_subject_fingerprint_before: "a".repeat(64),
            native_subject_fingerprint_after: "a".repeat(64),
            available_calendar_ids: request.calendar_ids.clone(),
            permission_class: "authorized".into(),
            batches: vec![CalendarBatchDto {
                calendar_id: "calendar-a".into(),
                records: Vec::new(),
                failure: None,
            }],
        };
        completion.device_id = "wrong-device".into();
        assert!(
            store
                .request(
                    person_id,
                    LocalContextOperationDto::CompleteAcquisition {
                        host_epoch: "host-a".into(),
                        result: completion.clone(),
                    },
                )
                .is_err()
        );
        let completion = LocalContextAcquisitionResultDto {
            device_id: request.device_id.clone(),
            ..completion
        };
        store
            .request(
                person_id,
                LocalContextOperationDto::CompleteAcquisition {
                    host_epoch: "host-a".into(),
                    result: completion.clone(),
                },
            )
            .unwrap();
        assert!(waiter.await.unwrap().is_ok());
        assert!(
            store
                .request(
                    person_id,
                    LocalContextOperationDto::CompleteAcquisition {
                        host_epoch: "host-a".into(),
                        result: completion,
                    },
                )
                .is_err()
        );
    }

    #[tokio::test]
    async fn attention_acquisition_is_person_and_subject_fenced() {
        let store = std::sync::Arc::new(LocalContextStore::default());
        let owner = person();
        let other = person();
        store
            .request(
                owner,
                LocalContextOperationDto::RegisterAttentionHost {
                    host_epoch: "attention-host".into(),
                },
            )
            .unwrap();
        let request = LocalContextAttentionAcquisitionRequestDto {
            request_id: "attention-request".into(),
            host_epoch: "attention-host".into(),
            person_id: owner.0.to_string(),
            device_id: "mac-device".into(),
            mode: LocalContextAttentionAcquisitionModeDto::ReadProjection,
            deadline_unix_ms: now_unix_ms().unwrap() + 20_000,
            expected_native_subject_fingerprint: Some("a".repeat(64)),
        };
        let waiter = tokio::spawn({
            let store = store.clone();
            let request = request.clone();
            async move {
                store
                    .acquire_attention(request, Cancellation::default())
                    .await
            }
        });
        tokio::task::yield_now().await;
        let polled = store
            .request(
                owner,
                LocalContextOperationDto::PollAttentionAcquisitions {
                    host_epoch: "attention-host".into(),
                },
            )
            .unwrap();
        assert_eq!(polled.attention_acquisitions, vec![request.clone()]);
        let view = attention(now_unix_ms().unwrap(), "attention:mac");
        let result = LocalContextAttentionAcquisitionResultDto {
            request_id: request.request_id.clone(),
            host_epoch: request.host_epoch.clone(),
            person_id: request.person_id.clone(),
            device_id: request.device_id.clone(),
            mode: request.mode,
            native_subject_fingerprint_before: "a".repeat(64),
            native_subject_fingerprint_after: "a".repeat(64),
            permission_class: "session_observation".into(),
            view: Some(view),
        };
        assert!(
            store
                .request(
                    other,
                    LocalContextOperationDto::CompleteAttentionAcquisition {
                        host_epoch: request.host_epoch.clone(),
                        result: result.clone(),
                    },
                )
                .is_err()
        );
        store
            .request(
                owner,
                LocalContextOperationDto::CompleteAttentionAcquisition {
                    host_epoch: request.host_epoch,
                    result,
                },
            )
            .unwrap();
        assert!(waiter.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn personal_acquisition_is_exact_domain_and_selection_fenced() {
        let store = std::sync::Arc::new(LocalContextStore::default());
        let owner = person();
        store
            .request(
                owner,
                LocalContextOperationDto::RegisterPersonalHost {
                    host_epoch: "personal-host".into(),
                },
            )
            .unwrap();
        let request = LocalContextPersonalAcquisitionRequestDto {
            request_id: "personal-request".into(),
            host_epoch: "personal-host".into(),
            person_id: owner.0.to_string(),
            device_id: "iphone".into(),
            domain: LocalContextPersonalDomainDto::People,
            selected_handles: vec!["person.identity:one".into()],
            event_handle: None,
            evidence_handles: vec![],
            destination_latitude: None,
            destination_longitude: None,
            event_start_unix_ms: None,
            event_end_unix_ms: None,
            travel_mode: None,
            deadline_unix_ms: now_unix_ms().unwrap() + 20_000,
            expected_native_subject_fingerprint: Some("a".repeat(64)),
        };
        let waiter = tokio::spawn({
            let store = store.clone();
            let request = request.clone();
            async move {
                store
                    .acquire_personal(request, Cancellation::default())
                    .await
            }
        });
        tokio::task::yield_now().await;
        let polled = store
            .request(
                owner,
                LocalContextOperationDto::PollPersonalAcquisitions {
                    host_epoch: "personal-host".into(),
                },
            )
            .unwrap();
        assert_eq!(polled.personal_acquisitions, vec![request.clone()]);
        let now = now_unix_ms().unwrap();
        let result = LocalContextPersonalAcquisitionResultDto {
            request_id: request.request_id.clone(),
            host_epoch: request.host_epoch.clone(),
            person_id: request.person_id.clone(),
            device_id: request.device_id.clone(),
            domain: request.domain,
            native_subject_fingerprint_before: "a".repeat(64),
            native_subject_fingerprint_after: "a".repeat(64),
            permission_class: "limited".into(),
            provider: "contacts.apple".into(),
            view: Some(json!({
                "schema_version": PROTOCOL_VERSION,
                "view_id": "people.identity",
                "source_handle": "people:opaque",
                "observed_at_unix_ms": now - 1,
                "expires_at_unix_ms": now + 299_999,
                "coverage_complete": true,
                "identities": []
            })),
        };
        store
            .request(
                owner,
                LocalContextOperationDto::CompletePersonalAcquisition {
                    host_epoch: request.host_epoch,
                    result,
                },
            )
            .unwrap();
        assert!(waiter.await.unwrap().is_ok());
    }

    #[test]
    fn generic_attention_publication_cannot_replace_trusted_subject() {
        let store = LocalContextStore::default();
        let owner = person();
        let now = now_unix_ms().unwrap();
        let view: AttentionView = serde_json::from_value(attention(now, "attention:mac")).unwrap();
        store
            .request(
                owner,
                LocalContextOperationDto::RegisterAttentionHost {
                    host_epoch: "attention-host".into(),
                },
            )
            .unwrap();
        store
            .commit_trusted_attention_projection(owner, "mac-device", &view, &"a".repeat(64))
            .unwrap();
        store
            .request(
                owner,
                LocalContextOperationDto::Publish {
                    device_id: "mac-device".into(),
                    view: attention(now, "attention:generic"),
                },
            )
            .unwrap();
        assert_eq!(
            store
                .trusted_attention_subject(owner, "mac-device")
                .unwrap(),
            "a".repeat(64)
        );
    }

    #[test]
    fn trusted_attention_observation_requires_live_host_epoch() {
        let store = LocalContextStore::default();
        let owner = person();
        let now = now_unix_ms().unwrap();
        let view: AttentionView = serde_json::from_value(attention(now, "attention:mac")).unwrap();
        store
            .request(
                owner,
                LocalContextOperationDto::RegisterAttentionHost {
                    host_epoch: "epoch-a".into(),
                },
            )
            .unwrap();
        let (observation_id, process) = store
            .commit_trusted_attention_projection_for_host(
                owner,
                "epoch-a",
                "mac-device",
                &view,
                &"a".repeat(64),
            )
            .unwrap();
        assert!(
            store
                .trusted_attention_observation(owner, "mac-device", observation_id, process)
                .is_ok()
        );
        store
            .request(
                owner,
                LocalContextOperationDto::RegisterAttentionHost {
                    host_epoch: "epoch-b".into(),
                },
            )
            .unwrap();
        assert!(
            store
                .trusted_attention_observation(owner, "mac-device", observation_id, process)
                .is_err()
        );
    }

    #[tokio::test]
    async fn trusted_attention_observation_expires_monotonically() {
        let store = LocalContextStore::default();
        let owner = person();
        store
            .request(
                owner,
                LocalContextOperationDto::RegisterAttentionHost {
                    host_epoch: "epoch".into(),
                },
            )
            .unwrap();
        let now = now_unix_ms().unwrap();
        let mut raw = attention(now, "attention:mac");
        raw["expires_at_unix_ms"] = (now + 1).into();
        let view: AttentionView = serde_json::from_value(raw).unwrap();
        let (observation_id, process) = store
            .commit_trusted_attention_projection_for_host(
                owner,
                "epoch",
                "mac-device",
                &view,
                &"a".repeat(64),
            )
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert!(
            store
                .trusted_attention_observation(owner, "mac-device", observation_id, process)
                .is_err()
        );
    }

    #[test]
    fn authorized_observation_projects_subset_and_rejects_replaced_authority() {
        let store = LocalContextStore::default();
        let person_id = person();
        let now = now_unix_ms().unwrap();
        let mut connection = floe_day::CalendarConnection {
            connection_id: Uuid::new_v4().to_string(),
            device_id: "iphone".into(),
            disconnected: false,
            scope: floe_day::CalendarScope::All,
            provider: DomainCalendarProvider::EventKit,
            revision: 7,
            source_authority: floe_context_contract::SourceAuthority::new(),
            calendars: (0..11)
                .map(|index| floe_day::CalendarSelection {
                    calendar_id: format!("calendar-{index}"),
                    calendar_name: format!("Calendar {index}"),
                })
                .collect(),
            last_success_at: None,
            last_range: None,
            error: None,
            error_at: None,
            source_statuses: Default::default(),
        };
        let ids: Vec<_> = connection
            .calendars
            .iter()
            .map(|calendar| calendar.calendar_id.clone())
            .collect();
        let operation = LocalContextOperationDto::PublishCalendarObservation {
            device_id: "iphone".into(),
            connection_id: connection.connection_id.clone(),
            connection_revision: 7,
            provider: CalendarProviderDto::EventKit,
            calendar_ids: ids.clone(),
            observed_at_unix_ms: now - 1,
            expires_at_unix_ms: now + 60_000,
            range_start_unix_ms: now - 60_000,
            range_end_unix_ms: now + 60_000,
            batches: ids
                .iter()
                .map(|identifier| CalendarBatchDto {
                    calendar_id: identifier.clone(),
                    records: vec![],
                    failure: (identifier == "calendar-1")
                        .then_some(CalendarFailureDto::ProviderUnavailable),
                })
                .collect(),
        };
        store
            .request_bound(person_id, operation.clone(), Some(&connection))
            .unwrap();
        let mut replaced = connection.clone();
        replaced.connection_id = Uuid::new_v4().to_string();
        assert!(
            store
                .request_bound(person_id, operation.clone(), Some(&replaced))
                .is_err()
        );
        let allowed = ids[..2].to_vec();
        let mut wrong_provider = connection.clone();
        wrong_provider.provider = DomainCalendarProvider::Android;
        assert!(
            store
                .authorized_calendar_observation(person_id, &wrong_provider, &allowed)
                .is_err()
        );
        let mut wrong_device = connection.clone();
        wrong_device.device_id = "another-device".into();
        assert!(
            store
                .authorized_calendar_observation(person_id, &wrong_device, &allowed)
                .is_err()
        );
        assert!(
            store
                .authorized_calendar_observation(
                    person_id,
                    &connection,
                    &["unknown-calendar".into()]
                )
                .is_err()
        );
        let projected = store
            .authorized_calendar_observation(person_id, &connection, &allowed)
            .unwrap();
        assert_eq!(projected.calendar_ids, allowed);
        assert_eq!(projected.batches.len(), 2);
        assert_eq!(
            projected.batches[1].failure,
            Some(floe_day::CalendarFailure::ProviderUnavailable)
        );
        assert!(
            store
                .authorized_calendar_observation(person(), &connection, &allowed)
                .is_err()
        );
        connection.revision += 1;
        assert!(
            store
                .request_bound(person_id, operation, Some(&connection))
                .is_err()
        );
        assert!(
            store
                .authorized_calendar_observation(person_id, &connection, &allowed)
                .is_ok()
        );
        connection.source_authority = connection.source_authority.advance().unwrap();
        assert!(
            store
                .authorized_calendar_observation(person_id, &connection, &allowed)
                .is_err()
        );
    }

    #[test]
    fn publication_is_person_bound_and_latest_device_wins() {
        let store = LocalContextStore::default();
        let owner = person();
        let other = person();
        let now = now_unix_ms().unwrap();
        store
            .request(
                owner,
                LocalContextOperationDto::Publish {
                    device_id: "mac".into(),
                    view: attention(now - 10, "attention:mac"),
                },
            )
            .unwrap();
        store
            .request(
                owner,
                LocalContextOperationDto::Publish {
                    device_id: "ipad".into(),
                    view: attention(now, "attention:ipad"),
                },
            )
            .unwrap();

        assert_eq!(
            store.attention(owner).unwrap().source_handle,
            "attention:ipad"
        );
        assert_eq!(
            store.attention(other),
            Err(AgentFailure::CapabilityUnavailable)
        );
    }

    #[test]
    fn rejects_unknown_fields_and_raw_payloads() {
        let store = LocalContextStore::default();
        let now = now_unix_ms().unwrap();
        let mut view = attention(now, "attention:mac");
        view.as_object_mut()
            .unwrap()
            .insert("raw_app_history".into(), json!(["mail", "browser"]));
        assert!(
            store
                .request(
                    person(),
                    LocalContextOperationDto::Publish {
                        device_id: "mac".into(),
                        view,
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn revoke_is_device_scoped() {
        let store = LocalContextStore::default();
        let owner = person();
        let now = now_unix_ms().unwrap();
        for device in ["mac", "ipad"] {
            store
                .request(
                    owner,
                    LocalContextOperationDto::Publish {
                        device_id: device.into(),
                        view: attention(now, &format!("attention:{device}")),
                    },
                )
                .unwrap();
        }
        let result = store
            .request(
                owner,
                LocalContextOperationDto::Revoke {
                    device_id: "ipad".into(),
                    view_id: None,
                },
            )
            .unwrap();
        assert_eq!(result.removed_count, 1);
        assert_eq!(
            store.attention(owner).unwrap().source_handle,
            "attention:mac"
        );
    }
}
