//! Day refresh intent, exact acquisition evidence and durable state.
use crate::{
    CalendarFailure, CalendarMirror, CalendarRange, CalendarRecord, CalendarSelection, DayError,
    DaySnapshot,
};
use chrono::{DateTime, NaiveDate, Utc};
use floe_context_contract::{CalendarProvider, GrantSourceBinding, SourceAuthority};
use floe_kernel::{OwnerActor, PersonId, Revision};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const MAX_REFRESH_SOURCES: usize = 64;
pub const MAX_REFRESH_CALENDARS: usize = 256;
pub const MAX_REFRESH_RECORDS: usize = 10_000;
pub const MAX_REFRESH_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CALENDAR_PAGE_RECORDS: usize = 128;
pub const MAX_CALENDAR_PAGE_BYTES: usize = 1024 * 1024;
pub const REFRESH_DEADLINE_SECONDS: u64 = 60;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayQuery {
    pub date: NaiveDate,
    pub timezone_offset_seconds: i32,
    pub end_timezone_offset_seconds: Option<i32>,
    pub now: DateTime<Utc>,
}
impl DayQuery {
    pub fn range(&self) -> Result<CalendarRange, DayError> {
        let range = CalendarRange {
            start_date: self.date,
            end_date_exclusive: self
                .date
                .succ_opt()
                .ok_or_else(|| DayError::validation("day out of range"))?,
            timezone_offset_seconds: self.timezone_offset_seconds,
            end_timezone_offset_seconds: self.end_timezone_offset_seconds,
        };
        if !range.is_valid() {
            return Err(DayError::validation("invalid day offsets"));
        }
        Ok(range)
    }
    /// Product acquisition has its own UTC span ceiling. Display queries keep
    /// the existing civil-date/offset contract.
    pub fn refresh_range(&self) -> Result<CalendarRange, DayError> {
        let range = self.range()?;
        let (start, end) = crate::range_bounds(&range)
            .map_err(|_| DayError::validation("invalid Day refresh range"))?;
        if start.timestamp_millis() < 0
            || end.signed_duration_since(start) > chrono::Duration::hours(48)
        {
            return Err(DayError::validation(
                "Day refresh requires a nonnegative UTC range of at most 48 hours",
            ));
        }
        Ok(range)
    }
    pub fn refresh_intent_digest(
        &self,
        person_id: PersonId,
        device_id: &str,
        command_id: Uuid,
    ) -> Result<[u8; 32], DayError> {
        self.refresh_range()?;
        // Display time is deliberately excluded. Replays retain the first query.
        digest(&(
            "day_refresh",
            person_id,
            device_id,
            command_id,
            self.date,
            self.timezone_offset_seconds,
            self.end_timezone_offset_seconds
                .unwrap_or(self.timezone_offset_seconds),
        ))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    content = "revision",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MirrorExpectation {
    Absent,
    Present(Revision),
}
impl MirrorExpectation {
    pub fn of(mirror: Option<&CalendarMirror>) -> Result<Self, DayError> {
        match mirror {
            Some(mirror)
                if mirror.mirror_revision > 0 && mirror.mirror_revision <= i64::MAX as u64 =>
            {
                Ok(Self::Present(Revision(mirror.mirror_revision)))
            }
            Some(_) => Err(DayError::storage("invalid mirror revision")),
            None => Ok(Self::Absent),
        }
    }
    pub fn next_revision(self) -> Result<u64, DayError> {
        match self {
            Self::Absent => Ok(1),
            Self::Present(value) if value.0 > 0 => value
                .0
                .checked_add(1)
                .filter(|next| *next <= i64::MAX as u64)
                .ok_or_else(|| DayError::conflict("mirror revision exhausted")),
            _ => Err(DayError::validation("invalid mirror expectation")),
        }
    }
}

/// Exact configured source fact for commit comparison. Never a product DTO.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarSourceVersion {
    pub source: GrantSourceBinding,
    pub provider: CalendarProvider,
    pub revision: Revision,
    pub authority: SourceAuthority,
    /// SHA-256 of the canonical serialized Connections SourceConnection record.
    pub configuration_digest: [u8; 32],
    pub calendars: Vec<CalendarSelection>,
}
impl CalendarSourceVersion {
    pub fn validate(&self, person_id: PersonId) -> Result<(), DayError> {
        self.source
            .validate()
            .map_err(|_| DayError::validation("invalid calendar source"))?;
        if self.source.person_id() != person_id
            || self.revision.0 == 0
            || self.revision.0 > i64::MAX as u64
            || !self.authority.is_valid()
            || self.configuration_digest == [0; 32]
            || self.calendars.len() > MAX_REFRESH_CALENDARS
            || self
                .calendars
                .windows(2)
                .any(|pair| pair[0].calendar_id >= pair[1].calendar_id)
            || self.calendars.iter().any(|calendar| {
                calendar.calendar_id.is_empty()
                    || calendar.calendar_id.len() > 512
                    || calendar.calendar_name.is_empty()
                    || calendar.calendar_name.len() > 256
            })
        {
            return Err(DayError::validation("invalid calendar source version"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CalendarRefreshRequest {
    pub actor: OwnerActor,
    pub refresh_operation_id: Uuid,
    pub query: DayQuery,
    pub expected_mirror_revision: MirrorExpectation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum CalendarResourceOutcome {
    Complete {
        calendar_id: String,
        records: Vec<CalendarRecord>,
        observed_at: DateTime<Utc>,
    },
    Failed {
        calendar_id: String,
        reason: CalendarFailure,
        observed_at: DateTime<Utc>,
    },
}
impl CalendarResourceOutcome {
    pub fn calendar_id(&self) -> &str {
        match self {
            Self::Complete { calendar_id, .. } | Self::Failed { calendar_id, .. } => calendar_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum CalendarSourceOutcome {
    Acquired {
        source: CalendarSourceVersion,
        read_operation_id: Uuid,
        observed_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        batches: Vec<CalendarResourceOutcome>,
    },
    Unavailable {
        source: CalendarSourceVersion,
        reason: CalendarFailure,
        observed_at: DateTime<Utc>,
    },
}
impl CalendarSourceOutcome {
    pub fn source(&self) -> &CalendarSourceVersion {
        match self {
            Self::Acquired { source, .. } | Self::Unavailable { source, .. } => source,
        }
    }
    pub fn has_success(&self) -> bool {
        matches!(self, Self::Acquired { batches, .. } if batches.iter().any(|batch| matches!(batch, CalendarResourceOutcome::Complete { .. })))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarAcquisition {
    pub refresh_operation_id: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub range: CalendarRange,
    pub inventory: Vec<CalendarSourceVersion>,
    pub sources: Vec<CalendarSourceOutcome>,
    pub completed_at: DateTime<Utc>,
}
impl CalendarAcquisition {
    pub fn validate(
        &self,
        request: &CalendarRefreshRequest,
        now: DateTime<Utc>,
    ) -> Result<(), DayError> {
        request
            .actor
            .validate()
            .map_err(|_| DayError::validation("invalid acquisition actor"))?;
        self.validate_identity(
            request.refresh_operation_id,
            request.actor.person_id,
            &request.actor.device_id,
            &request.query.range()?,
            now,
        )
    }
    pub fn validate_record(
        &self,
        record: &RefreshRecord,
        now: DateTime<Utc>,
    ) -> Result<(), DayError> {
        record.validate()?;
        self.validate_identity(
            record.operation_id,
            record.person_id,
            &record.device_id,
            &record.query.range()?,
            now,
        )
    }
    fn validate_identity(
        &self,
        operation_id: Uuid,
        person_id: PersonId,
        device_id: &str,
        range: &CalendarRange,
        now: DateTime<Utc>,
    ) -> Result<(), DayError> {
        if self.refresh_operation_id != operation_id
            || self.person_id != person_id
            || self.device_id != device_id
            || &self.range != range
            || self.completed_at > now
            || self.inventory.len() > MAX_REFRESH_SOURCES
            || self.inventory.len() != self.sources.len()
            || self
                .inventory
                .windows(2)
                .any(|pair| pair[0].source.connection_id() >= pair[1].source.connection_id())
            || self
                .inventory
                .iter()
                .map(|source| source.calendars.len())
                .sum::<usize>()
                > MAX_REFRESH_CALENDARS
        {
            return Err(DayError::validation(
                "invalid calendar acquisition identity",
            ));
        }
        let mut records = 0usize;
        for (expected, result) in self.inventory.iter().zip(&self.sources) {
            expected.validate(self.person_id)?;
            if result.source() != expected {
                return Err(DayError::validation("calendar acquisition source mismatch"));
            }
            match result {
                CalendarSourceOutcome::Acquired {
                    read_operation_id,
                    observed_at,
                    expires_at,
                    batches,
                    ..
                } => {
                    if read_operation_id.is_nil()
                        || *observed_at > self.completed_at
                        || *expires_at <= now
                        || batches.len() != expected.calendars.len()
                        || batches
                            .iter()
                            .zip(&expected.calendars)
                            .any(|(batch, calendar)| batch.calendar_id() != calendar.calendar_id)
                    {
                        return Err(DayError::validation("invalid calendar batch coverage"));
                    }
                    for batch in batches {
                        if let CalendarResourceOutcome::Complete {
                            records: batch_records,
                            observed_at,
                            ..
                        } = batch
                        {
                            if *observed_at > self.completed_at {
                                return Err(DayError::validation(
                                    "invalid calendar observation time",
                                ));
                            }
                            records = records
                                .checked_add(batch_records.len())
                                .ok_or_else(|| DayError::validation("calendar record budget"))?;
                        }
                    }
                }
                CalendarSourceOutcome::Unavailable { observed_at, .. }
                    if *observed_at > self.completed_at =>
                {
                    return Err(DayError::validation("invalid calendar failure time"));
                }
                _ => {}
            }
        }
        if records > MAX_REFRESH_RECORDS
            || serde_json::to_vec(self)
                .map_err(|_| DayError::validation("invalid calendar acquisition"))?
                .len()
                > MAX_REFRESH_BYTES
        {
            return Err(DayError::validation("calendar acquisition budget"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DayRefreshFailure {
    SourceChanged,
    PermissionDenied,
    Unavailable,
    VaultLocked,
    BudgetExceeded,
    DeadlineExceeded,
    Cancelled,
    HostInterrupted,
    StorageUnavailable,
    InvalidAcquisition,
}
pub type CalendarRefreshError = DayRefreshFailure;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum DayRefreshState {
    Pending,
    Running,
    Completed { day: DaySnapshot },
    Failed { failure: DayRefreshFailure },
    Interrupted { failure: DayRefreshFailure },
}
impl DayRefreshState {
    pub fn terminal(&self) -> bool {
        !matches!(self, Self::Pending | Self::Running)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayRefreshSnapshot {
    pub operation_ref: Uuid,
    pub revision: Revision,
    #[serde(flatten)]
    pub state: DayRefreshState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshRecord {
    pub operation_id: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub command_id: Uuid,
    pub intent_digest: [u8; 32],
    pub query: DayQuery,
    pub expected_mirror_revision: MirrorExpectation,
    pub executor_generation: Uuid,
    pub revision: Revision,
    pub admitted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub state: DayRefreshState,
}
impl RefreshRecord {
    pub fn validate(&self) -> Result<(), DayError> {
        if self.operation_id.is_nil()
            || self.command_id.is_nil()
            || self.executor_generation.is_nil()
            || !self.person_id.is_valid()
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.revision.0 == 0
            || self.revision.0 > i64::MAX as u64
            || self.intent_digest
                != self.query.refresh_intent_digest(
                    self.person_id,
                    &self.device_id,
                    self.command_id,
                )?
            || self.updated_at < self.admitted_at
        {
            return Err(DayError::storage("invalid refresh record"));
        }
        self.expected_mirror_revision.next_revision()?;
        if let DayRefreshState::Completed { day } = &self.state {
            day.validate_bounds()?;
        }
        Ok(())
    }
    pub fn snapshot(&self) -> DayRefreshSnapshot {
        DayRefreshSnapshot {
            operation_ref: self.operation_id,
            revision: self.revision,
            state: self.state.clone(),
        }
    }
    pub fn transition(&self, state: DayRefreshState, now: DateTime<Utc>) -> Result<Self, DayError> {
        self.validate()?;
        if self.state.terminal()
            || !matches!(
                (&self.state, &state),
                (
                    DayRefreshState::Pending,
                    DayRefreshState::Running
                        | DayRefreshState::Failed { .. }
                        | DayRefreshState::Interrupted { .. }
                ) | (
                    DayRefreshState::Running,
                    DayRefreshState::Completed { .. }
                        | DayRefreshState::Failed { .. }
                        | DayRefreshState::Interrupted { .. }
                )
            )
        {
            return Err(DayError::conflict("refresh transition rejected"));
        }
        let mut next = self.clone();
        next.revision = Revision(
            self.revision
                .0
                .checked_add(1)
                .filter(|value| *value <= i64::MAX as u64)
                .ok_or_else(|| DayError::conflict("refresh revision exhausted"))?,
        );
        next.state = state;
        next.updated_at = now;
        next.validate()?;
        Ok(next)
    }
}

#[derive(Clone, Debug)]
pub struct RefreshAdmission {
    pub operation_id: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub command_id: Uuid,
    pub intent_digest: [u8; 32],
    pub query: DayQuery,
    pub executor_generation: Uuid,
    pub admitted_at: DateTime<Utc>,
}
impl RefreshAdmission {
    pub fn record(
        &self,
        expected_mirror_revision: MirrorExpectation,
    ) -> Result<RefreshRecord, DayError> {
        let record = RefreshRecord {
            operation_id: self.operation_id,
            person_id: self.person_id,
            device_id: self.device_id.clone(),
            command_id: self.command_id,
            intent_digest: self.intent_digest,
            query: self.query.clone(),
            expected_mirror_revision,
            executor_generation: self.executor_generation,
            revision: Revision(1),
            admitted_at: self.admitted_at,
            updated_at: self.admitted_at,
            state: DayRefreshState::Pending,
        };
        record.validate()?;
        Ok(record)
    }
}
#[derive(Clone, Debug)]
pub enum RefreshAdmissionResult {
    New(RefreshRecord),
    Existing(RefreshRecord),
}
#[derive(Clone, Debug)]
pub struct RefreshLookup {
    pub operation_id: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
}
#[derive(Clone, Debug)]
pub struct RefreshTransition {
    pub previous: RefreshRecord,
    pub next: RefreshRecord,
}
impl RefreshTransition {
    pub fn validate(&self) -> Result<(), DayError> {
        if matches!(&self.next.state, DayRefreshState::Completed { .. })
            || self
                .previous
                .transition(self.next.state.clone(), self.next.updated_at)?
                != self.next
        {
            return Err(DayError::conflict(
                "refresh transition cannot bypass mirror commit",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct RefreshCommit {
    pub previous: RefreshRecord,
    pub next: RefreshRecord,
    pub acquisition: CalendarAcquisition,
    pub mirror: CalendarMirror,
}
#[derive(Clone, Debug)]
pub struct RefreshExecutorReplacement {
    pub person_id: PersonId,
    pub device_id: String,
    pub executor_generation: Uuid,
    pub now: DateTime<Utc>,
}

pub fn digest(value: &impl Serialize) -> Result<[u8; 32], DayError> {
    serde_json::to_vec(value)
        .map(|bytes| Sha256::digest(bytes).into())
        .map_err(|_| DayError::validation("invalid canonical value"))
}
