//! Idempotent projection of an already proved external Calendar effect.
use chrono::{DateTime, Utc};
use floe_context_contract::{CalendarProvider, ConnectionId, SourceAuthority};
use floe_kernel::{PersonId, Revision};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::{CalendarExternalRevision, CalendarMirror, CalendarMirrorSourceState, CalendarMirrorState, CalendarRecord, CalendarSource, CalendarSourceVersion, DayError, Event, MirrorExpectation, SourceRef};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionCollectionSource { pub connection_id: ConnectionId, pub connection_revision: u64, pub source_authority: SourceAuthority, pub provider: CalendarProvider, pub calendar_id: String, pub calendar_name: String }
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CalendarActionCollection {
    Created { source: ActionCollectionSource, record: CalendarRecord },
    Updated { source: ActionCollectionSource, expected_external_id: String, expected_external_revision: CalendarExternalRevision, record: CalendarRecord },
    Deleted { source: ActionCollectionSource, external_id: String, expected_external_revision: CalendarExternalRevision },
}
impl CalendarActionCollection {
    pub fn source(&self) -> &ActionCollectionSource { match self { Self::Created { source, .. } | Self::Updated { source, .. } | Self::Deleted { source, .. } => source } }
    pub fn validate(&self) -> Result<(), DayError> {
        let source = self.source();
        if source.connection_revision == 0 || source.connection_revision > i64::MAX as u64 || !source.source_authority.is_valid() || source.calendar_id.is_empty() || source.calendar_name.is_empty() || source.calendar_name.len() > 256 || source.calendar_name.chars().any(char::is_control) { return Err(DayError::validation("invalid Calendar collection source")); }
        match self {
            Self::Created { record, .. } | Self::Updated { record, .. } => { if record.calendar_id != source.calendar_id || record.external_id.is_empty() || record.external_id.len() > 512 || !record.external_revision.is_valid() || record.title.len() > 4096 { return Err(DayError::validation("invalid collected Calendar record")); } }
            Self::Deleted { external_id, expected_external_revision, .. } => { if external_id.is_empty() || external_id.len() > 512 || !expected_external_revision.is_valid() { return Err(DayError::validation("invalid Calendar deletion evidence")); } }
        }
        if let Self::Updated { expected_external_id, expected_external_revision, record, .. } = self { if expected_external_id != &record.external_id || !expected_external_revision.is_valid() { return Err(DayError::validation("updated Calendar identity changed")); } }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayCollectionReceipt { pub execution_id: Uuid, pub receipt_digest: [u8; 32], pub day_projection_ref: String }
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayCollectionCommit { pub person_id: PersonId, pub device_id: String, pub executor_generation: Uuid, pub execution_id: Uuid, pub receipt_digest: [u8; 32], pub intent_digest: [u8; 32], pub collection: CalendarActionCollection, pub collected_at: DateTime<Utc> }
impl DayCollectionCommit {
    pub fn validate(&self) -> Result<(), DayError> {
        self.collection.validate()?;
        if !self.person_id.is_valid() || self.device_id.is_empty() || self.device_id.len() > 256 || self.executor_generation.is_nil() || self.execution_id.is_nil() || self.receipt_digest == [0; 32] || self.intent_digest != crate::digest(&(self.person_id, &self.device_id, self.execution_id, self.receipt_digest, &self.collection))? { return Err(DayError::validation("invalid Calendar collection intent")); } Ok(())
    }
    /// Called by the repository with current complete inventory and mirror in
    /// its short local transaction. No external effect or permission is made.
    pub fn apply(&self, previous: Option<&CalendarMirror>, inventory: &[CalendarSourceVersion]) -> Result<(CalendarMirror, DayCollectionReceipt), DayError> {
        self.validate()?;
        if inventory.len() > crate::MAX_REFRESH_SOURCES || inventory.windows(2).any(|pair| pair[0].source.connection_id() >= pair[1].source.connection_id()) || inventory.iter().map(|source| source.calendars.len()).sum::<usize>() > crate::MAX_REFRESH_CALENDARS { return Err(DayError::validation("invalid collection inventory")); }
        for source in inventory { source.validate(self.person_id)?; }
        let next_mirror_revision = MirrorExpectation::of(previous)?.next_revision()?;
        let expected = self.collection.source();
        let source = inventory.iter().find(|source| source.source.connection_id() == expected.connection_id).ok_or_else(|| DayError::conflict("collection source is unavailable"))?;
        if source.revision.0 != expected.connection_revision || source.authority != expected.source_authority || source.provider != expected.provider || !source.calendars.iter().any(|calendar| calendar.calendar_id == expected.calendar_id) { return Err(DayError::conflict("collection source changed")); }
        let mut events = previous.map(|mirror| mirror.events.clone()).unwrap_or_default();
        events.retain(|event| matches!(&event.source, SourceRef::Calendar(origin) if inventory.iter().any(|current| current.source.connection_id() == origin.connection_id && current.provider == origin.provider && current.calendars.iter().any(|calendar| calendar.calendar_id == origin.calendar_id) && previous.and_then(|mirror| mirror.state.source_state(origin.connection_id.as_str())).is_some_and(|state| state.source == *current))));
        let external_id = match &self.collection { CalendarActionCollection::Created { record, .. } | CalendarActionCollection::Updated { record, .. } => &record.external_id, CalendarActionCollection::Deleted { external_id, .. } => external_id };
        let existing = events.iter().position(|event| matches!(&event.source, SourceRef::Calendar(origin) if origin.connection_id == expected.connection_id && origin.provider == expected.provider && origin.calendar_id == expected.calendar_id && &origin.external_id == external_id));
        if let Some(index) = existing {
            let SourceRef::Calendar(origin) = &events[index].source else { return Err(DayError::storage("invalid Calendar mirror event")); };
            match &self.collection {
                CalendarActionCollection::Created { record, .. } if origin.external_revision != record.external_revision => return Err(DayError::conflict("created event already has newer observation")),
                CalendarActionCollection::Updated { expected_external_revision, record, .. } if &origin.external_revision != expected_external_revision && origin.external_revision != record.external_revision => return Err(DayError::conflict("updated event has newer observation")),
                CalendarActionCollection::Deleted { expected_external_revision, .. } if &origin.external_revision != expected_external_revision => return Err(DayError::conflict("deleted event has newer observation")),
                _ => {}
            }
        }
        match &self.collection {
            CalendarActionCollection::Deleted { .. } => { if let Some(index) = existing { events.remove(index); } },
            CalendarActionCollection::Created { record, .. } | CalendarActionCollection::Updated { record, .. } => {
                let origin = CalendarSource { can_modify: record.can_modify, connection_id: expected.connection_id.clone(), provider: expected.provider, calendar_id: expected.calendar_id.clone(), calendar_name: expected.calendar_name.clone(), external_id: record.external_id.clone(), external_revision: record.external_revision.clone() };
                let mut event = Event::observed_calendar(self.person_id, record.title.clone(), record.schedule.clone(), origin, self.collected_at)?;
                event.id = calendar_event_id(source, &record.calendar_id, &record.external_id)?;
                event.revision = Revision(next_mirror_revision);
                if let Some(index) = existing {
                    let old = &events[index]; event.id = old.id; event.created_at = old.created_at; event.revision = old.revision;
                    if old.title == event.title && old.schedule == event.schedule && old.source == event.source { event.updated_at = old.updated_at; } else { event.revision = Revision(old.revision.0.checked_add(1).filter(|revision| *revision <= i64::MAX as u64).ok_or_else(|| DayError::conflict("event revision exhausted"))?); }
                    events[index] = event;
                } else { events.push(event); }
            }
        }
        let sources = inventory.iter().map(|source| previous.and_then(|mirror| mirror.state.source_state(source.source.connection_id().as_str())).filter(|state| state.source == *source).cloned().unwrap_or_else(|| CalendarMirrorSourceState { source: source.clone(), last_success_at: None, last_range: None, error: None, error_at: None, calendar_statuses: Default::default() })).collect();
        // A single causal event is not complete Calendar interval coverage.
        let mirror = CalendarMirror { mirror_revision: next_mirror_revision, state: CalendarMirrorState { sources }, events };
        let receipt = DayCollectionReceipt { execution_id: self.execution_id, receipt_digest: self.receipt_digest, day_projection_ref: format!("day.collection:{}", self.execution_id) };
        Ok((mirror, receipt))
    }
}
pub(crate) fn calendar_event_id(source: &CalendarSourceVersion, calendar_id: &str, external_id: &str) -> Result<floe_kernel::EventId, DayError> {
    let digest = crate::digest(&("floe.day.calendar.event", source.source.person_id(), &source.source, source.authority, calendar_id, external_id))?;
    let mut bytes = [0; 16]; bytes.copy_from_slice(&digest[..16]); bytes[6] = (bytes[6] & 15) | 80; bytes[8] = (bytes[8] & 63) | 128; Ok(floe_kernel::EventId(Uuid::from_bytes(bytes)))
}
