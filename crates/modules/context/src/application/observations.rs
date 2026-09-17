//! What this device has actually observed, and how long that stays true.
//!
//! Context owns the trusted projections a device produced: which subject they
//! stood on, when they were observed, and when they stop counting. The host
//! queue that produced them is the platform's; applying them to a read is the
//! reader's. Freshness is judged twice — against the wall clock the observation
//! recorded, and against a monotonic clock that a wall-clock change cannot move.

use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::Duration,
};

use floe_agent_contract::AgentFailure;
use floe_context_contract::CalendarProvider;
use floe_day::CalendarBatch;
use floe_context_contract::PersonId;
use serde_json::Value;
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    AttentionView, FeasibilityView, PeopleView, WellbeingView, validate_attention_view,
    validate_feasibility_view, validate_people_view, validate_wellbeing_view,
};

/// The views a device may publish here.
pub const ALLOWED_VIEW_IDS: [&str; 5] = [
    "people.identity",
    "schedule.feasibility",
    "attention.coarse",
    "wellbeing.derived",
    "calendar.timeline",
];

/// The most trusted observations kept per kind.
const MAX_TRUSTED_ATTENTION: usize = 16;
const MAX_TRUSTED_PERSONAL: usize = 32;

#[derive(Clone)]
pub struct ObservationEntry {
    pub device_id: String,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    observed_monotonic: Instant,
    monotonic_ttl: Duration,
    pub process_incarnation: Uuid,
    pub observation_id: Uuid,
    pub native_subject_fingerprint: Option<String>,
    pub view: Value,
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
    monotonic_ttl: Duration,
}

#[derive(Clone)]
pub struct TrustedPersonalObservation {
    pub person_id: PersonId,
    pub device_id: String,
    pub host_epoch: String,
    pub observation_id: Uuid,
    pub process_incarnation: Uuid,
    pub native_subject_fingerprint: String,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub query_fingerprint: Vec<u8>,
    observed_monotonic: Instant,
    monotonic_ttl: Duration,
}

/// One calendar observation a device published, before any grant is applied.
#[derive(Clone)]
pub struct PublishedCalendarObservation {
    pub connection_id: String,
    pub source_authority: floe_context_contract::SourceAuthority,
    pub connection_revision: u64,
    pub provider: CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub batches: Vec<CalendarBatch>,
}

/// The device observations one process is willing to stand behind.
pub struct ObservationRegistry {
    entries: Mutex<HashMap<(PersonId, String, String), ObservationEntry>>,
    calendar_observations: Mutex<HashMap<(PersonId, String), PublishedCalendarObservation>>,
    trusted_attention: Mutex<VecDeque<TrustedAttentionObservation>>,
    trusted_personal: Mutex<VecDeque<TrustedPersonalObservation>>,
    process_incarnation: Uuid,
}

impl Default for ObservationRegistry {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            calendar_observations: Mutex::new(HashMap::new()),
            trusted_attention: Mutex::new(VecDeque::new()),
            trusted_personal: Mutex::new(VecDeque::new()),
            process_incarnation: Uuid::new_v4(),
        }
    }
}

impl ObservationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn process_incarnation(&self) -> Uuid {
        self.process_incarnation
    }

    /// Record a view a device published for itself.
    ///
    /// A generic publication never replaces a trusted attention projection: a
    /// view the host proved a subject for outranks one it merely asserted.
    pub fn publish(
        &self,
        person_id: PersonId,
        device_id: &str,
        view_id: &str,
        view: Value,
        received_wall_unix_ms: i64,
    ) -> Result<bool, AgentFailure> {
        let (observed_at_unix_ms, expires_at_unix_ms) =
            validate_view(view_id, &view, received_wall_unix_ms)?;
        let monotonic_ttl = Duration::from_millis(
            u64::try_from(expires_at_unix_ms.saturating_sub(received_wall_unix_ms))
                .map_err(|_| AgentFailure::StaleContext)?,
        );
        if view_id == "attention.coarse"
            && self
                .entries
                .lock()
                .map_err(|_| AgentFailure::Interrupted)?
                .get(&(person_id, device_id.to_owned(), view_id.to_owned()))
                .is_some_and(|entry| entry.native_subject_fingerprint.is_some())
        {
            return Ok(false);
        }
        let entry = ObservationEntry {
            device_id: device_id.to_owned(),
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
            .map_err(|_| AgentFailure::Interrupted)?
            .insert(
                (person_id, device_id.to_owned(), view_id.to_owned()),
                entry,
            );
        Ok(true)
    }

    /// Record one calendar observation, once the connection it names still
    /// matches what the device reported.
    pub fn publish_calendar_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        observation: PublishedCalendarObservation,
        connection: &floe_day::CalendarConnection,
        now_unix_ms: i64,
    ) -> Result<(), AgentFailure> {
        validate_calendar_observation(&observation, now_unix_ms)?;
        let mut expected_ids: Vec<_> = connection
            .calendars
            .iter()
            .map(|calendar| calendar.calendar_id.clone())
            .collect();
        let mut actual_ids = observation.calendar_ids.clone();
        expected_ids.sort();
        actual_ids.sort();
        if connection.disconnected
            || connection.connection_id != observation.connection_id
            || connection.device_id != device_id
            || connection.provider != observation.provider
            || connection.revision != observation.connection_revision
            || expected_ids != actual_ids
        {
            return Err(AgentFailure::StaleContext);
        }
        self.calendar_observations
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?
            .insert((person_id, device_id.to_owned()), observation);
        Ok(())
    }

    pub fn read_entry(
        &self,
        person_id: PersonId,
        view_id: &str,
        device_id: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<ObservationEntry, AgentFailure> {
        let mut entries = self.entries.lock().map_err(|_| AgentFailure::Interrupted)?;
        entries.retain(|_, entry| {
            entry.expires_at_unix_ms > now_unix_ms
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

    /// Forget every view a device published for one Person.
    ///
    /// A trusted attention projection is not revoked this way; only the host
    /// that proved it may take it back.
    pub fn revoke(
        &self,
        person_id: PersonId,
        device_id: &str,
        view_id: Option<&str>,
    ) -> Result<usize, AgentFailure> {
        let mut entries = self.entries.lock().map_err(|_| AgentFailure::Interrupted)?;
        let before = entries.len();
        entries.retain(|(person, device, id), entry| {
            *person != person_id
                || device != device_id
                || view_id.is_some_and(|view_id| view_id != id)
                || (id == "attention.coarse" && entry.native_subject_fingerprint.is_some())
        });
        let mut removed_count = before - entries.len();
        drop(entries);
        if view_id.is_none_or(|id| id == "calendar.timeline") {
            removed_count += self
                .calendar_observations
                .lock()
                .map_err(|_| AgentFailure::Interrupted)?
                .remove(&(person_id, device_id.to_owned()))
                .is_some() as usize;
        }
        Ok(removed_count)
    }

    pub fn attention(
        &self,
        person_id: PersonId,
        now_unix_ms: i64,
    ) -> Result<AttentionView, AgentFailure> {
        serde_json::from_value(
            self.read_entry(person_id, "attention.coarse", None, now_unix_ms)?
                .view,
        )
        .map_err(|_| AgentFailure::InvalidInput)
    }

    pub fn attention_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        now_unix_ms: i64,
    ) -> Result<(AttentionView, Uuid, Uuid), AgentFailure> {
        let entry = self.read_entry(person_id, "attention.coarse", Some(device_id), now_unix_ms)?;
        let view = serde_json::from_value(entry.view).map_err(|_| AgentFailure::InvalidInput)?;
        Ok((view, entry.observation_id, entry.process_incarnation))
    }

    pub fn trusted_attention_subject(
        &self,
        person_id: PersonId,
        device_id: &str,
        now_unix_ms: i64,
    ) -> Result<String, AgentFailure> {
        self.read_entry(person_id, "attention.coarse", Some(device_id), now_unix_ms)?
            .native_subject_fingerprint
            .ok_or(AgentFailure::AccessReviewRequired)
    }

    /// Record an attention projection the host proved a subject for.
    pub fn commit_trusted_attention_projection(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        device_id: &str,
        view: &AttentionView,
        native_subject_fingerprint: &str,
        now_unix_ms: i64,
    ) -> Result<(Uuid, Uuid), AgentFailure> {
        if !valid_native_subject_fingerprint(native_subject_fingerprint) {
            return Err(AgentFailure::InvalidInput);
        }
        validate_attention_view(view, now_unix_ms).map_err(|_| AgentFailure::InvalidInput)?;
        let ttl_ms = view
            .expires_at_unix_ms
            .checked_sub(now_unix_ms)
            .ok_or(AgentFailure::StaleContext)?;
        let monotonic_ttl =
            Duration::from_millis(u64::try_from(ttl_ms).map_err(|_| AgentFailure::StaleContext)?);
        let observation_id = Uuid::new_v4();
        let process_incarnation = self.process_incarnation;
        let entry = ObservationEntry {
            device_id: device_id.to_owned(),
            observed_at_unix_ms: view.observed_at_unix_ms,
            expires_at_unix_ms: view.expires_at_unix_ms,
            observed_monotonic: Instant::now(),
            monotonic_ttl,
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
            .trusted_attention
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
            monotonic_ttl,
        });
        while observations.len() > MAX_TRUSTED_ATTENTION {
            observations.pop_front();
        }
        Ok((observation_id, process_incarnation))
    }

    /// The trusted attention projection one observation id names, if it is
    /// still fresh and still belongs to the host epoch that proved it.
    pub fn trusted_attention_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        host_epoch: &str,
        observation_id: Uuid,
        process_incarnation: Uuid,
        now_unix_ms: i64,
    ) -> Result<(AttentionView, String), AgentFailure> {
        let mut observations = self
            .trusted_attention
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        observations.retain(|observation| {
            observation.view.expires_at_unix_ms > now_unix_ms
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

    /// Forget every attention projection for one Person.
    ///
    /// A host that took over, or went away, cannot vouch for what the previous
    /// one observed.
    pub fn invalidate_trusted_attention(&self, person_id: PersonId) -> Result<(), AgentFailure> {
        self.trusted_attention
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

    #[allow(clippy::too_many_arguments)]
    pub fn commit_trusted_personal_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        host_epoch: String,
        observation_id: Uuid,
        process_incarnation: Uuid,
        native_subject_fingerprint: &str,
        observed_at_unix_ms: i64,
        expires_at_unix_ms: i64,
        query_fingerprint: Vec<u8>,
    ) -> Result<(), AgentFailure> {
        if expires_at_unix_ms <= observed_at_unix_ms
            || !valid_native_subject_fingerprint(native_subject_fingerprint)
        {
            return Err(AgentFailure::InvalidInput);
        }
        let monotonic_ttl = Duration::from_millis(
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
            monotonic_ttl,
        };
        let mut values = self
            .trusted_personal
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        values.retain(|item| {
            item.person_id != person_id
                || item.observation_id != observation_id
                || item.process_incarnation != process_incarnation
        });
        values.push_back(observation);
        while values.len() > MAX_TRUSTED_PERSONAL {
            values.pop_front();
        }
        Ok(())
    }

    pub fn trusted_personal_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        host_epoch: &str,
        observation_id: Uuid,
        process_incarnation: Uuid,
        now_unix_ms: i64,
    ) -> Result<TrustedPersonalObservation, AgentFailure> {
        let mut values = self
            .trusted_personal
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        values.retain(|item| {
            item.expires_at_unix_ms > now_unix_ms
                && item.observed_monotonic.elapsed() < item.monotonic_ttl
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

    /// The calendar observation a grant admits, narrowed to the calendars it
    /// actually names.
    pub fn authorized_calendar_observation(
        &self,
        person_id: PersonId,
        connection: &floe_day::CalendarConnection,
        calendar_ids: &[String],
        now_unix_ms: i64,
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
        validate_calendar_observation(observation, now_unix_ms)?;
        let mut projected = observation.clone();
        projected.calendar_ids = calendar_ids.to_vec();
        projected
            .batches
            .retain(|batch| calendar_ids.contains(&batch.calendar_id));
        Ok(projected)
    }
}

/// Whether the view a device published is one Context accepts, and when it
/// stops being true.
pub fn validate_view(
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

pub fn validate_calendar_observation(
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
        CalendarProvider::EventKit | CalendarProvider::Android
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

pub fn valid_native_subject_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
