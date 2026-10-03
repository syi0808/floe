//! Fresh metadata facts for a cached Day projection. No event read authority.
use chrono::{DateTime, Utc};
use floe_kernel::OwnerActor;
use crate::{CalendarFailure, CalendarMirrorSourceState, CalendarMirrorState, CalendarSourceVersion, DayError, MAX_REFRESH_CALENDARS, MAX_REFRESH_SOURCES};

#[derive(Clone, Debug)]
pub enum CalendarCacheSourceStatus {
    Current { source: CalendarSourceVersion, observed_at: DateTime<Utc> },
    Unavailable { source: CalendarSourceVersion, failure: CalendarFailure, observed_at: DateTime<Utc> },
}
impl CalendarCacheSourceStatus {
    pub fn source(&self) -> &CalendarSourceVersion { match self { Self::Current { source, .. } | Self::Unavailable { source, .. } => source } }
    pub fn observed_at(&self) -> DateTime<Utc> { match self { Self::Current { observed_at, .. } | Self::Unavailable { observed_at, .. } => *observed_at } }
}
#[derive(Clone, Debug)]
pub struct CalendarCacheInspection { pub actor: OwnerActor, pub sources: Vec<CalendarCacheSourceStatus>, pub observed_at: DateTime<Utc> }
impl CalendarCacheInspection {
    pub fn validate(&self, actor: &OwnerActor, now: DateTime<Utc>) -> Result<(), DayError> {
        actor.validate().map_err(|_| DayError::validation("invalid Day cache actor"))?;
        if &self.actor != actor || self.observed_at > now || now.signed_duration_since(self.observed_at) > chrono::Duration::seconds(30) || self.sources.len() > MAX_REFRESH_SOURCES || self.sources.windows(2).any(|pair| pair[0].source().source.connection_id() >= pair[1].source().source.connection_id()) || self.sources.iter().map(|source| source.source().calendars.len()).sum::<usize>() > MAX_REFRESH_CALENDARS { return Err(DayError::validation("invalid Calendar cache inspection")); }
        for source in &self.sources { source.source().validate(actor.person_id)?; if source.observed_at() > self.observed_at || now.signed_duration_since(source.observed_at()) > chrono::Duration::seconds(30) { return Err(DayError::validation("stale Calendar cache inspection")); } }
        Ok(())
    }
    /// Decorate an in-memory projection only. Cached events and all persisted
    /// command snapshots remain immutable under this read-only operation.
    pub fn project_state(&self, cached: Option<&CalendarMirrorState>) -> CalendarMirrorState {
        let mut sources = cached.map(|state| state.sources.clone()).unwrap_or_default();
        for source in &mut sources {
            let current = self.sources.iter().find(|current| current.source().source.connection_id() == source.source.source.connection_id());
            let failure = match current {
                Some(CalendarCacheSourceStatus::Current { source: current, .. }) if current == &source.source => None,
                Some(CalendarCacheSourceStatus::Unavailable { source: current, failure, .. }) if current == &source.source => Some(*failure),
                _ => Some(CalendarFailure::SourceChanged),
            };
            if let Some(failure) = failure {
                source.error = Some(failure); source.error_at = Some(self.observed_at);
                for status in source.calendar_statuses.values_mut() { status.error = Some(failure); status.error_at = Some(self.observed_at); }
            }
        }
        for current in &self.sources {
            if sources.iter().any(|source| source.source.source.connection_id() == current.source().source.connection_id()) { continue; }
            let failure = match current { CalendarCacheSourceStatus::Current { .. } => None, CalendarCacheSourceStatus::Unavailable { failure, .. } => Some(*failure) };
            sources.push(CalendarMirrorSourceState { source: current.source().clone(), last_success_at: None, last_range: None, error: failure, error_at: failure.map(|_| current.observed_at()), calendar_statuses: Default::default() });
        }
        sources.sort_by(|left, right| left.source.source.connection_id().cmp(&right.source.source.connection_id()));
        CalendarMirrorState { sources }
    }
}
