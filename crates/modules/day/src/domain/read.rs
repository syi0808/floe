//! Bounded owner selections over stored Day records. History outside the
//! requested selection is neither materialized nor counted against its budget.
use crate::{
    CalendarRange, DayError, DayQuery, DayTimelineItem, EventSchedule, MAX_DAY_SNAPSHOT_BYTES,
    MAX_DAY_SNAPSHOT_ITEMS, TimelineItem,
};
use chrono::{DateTime, Utc};
use floe_kernel::PersonId;

#[derive(Clone, Debug)]
pub enum DayReadSelection {
    DisplayDay {
        range: CalendarRange,
    },
    ActionWindow {
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    },
    OpenTasks,
    CurrentNotes,
}
#[derive(Clone, Debug)]
pub struct DayReadQuery {
    pub person_id: PersonId,
    pub selection: DayReadSelection,
    pub max_items: usize,
    /// Maximum serialized DayTimelineItem projection bytes returned by the read.
    pub max_projected_item_bytes: usize,
    /// Optional bounds for evidence reads that must stop before JSON decoding.
    pub acquisition: Option<DayReadAcquisitionBudget>,
}

/// Pre-deserialization bounds for exact Task/Note evidence selections.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DayReadAcquisitionBudget {
    max_candidate_items: usize,
    max_serialized_payload_bytes: usize,
}

impl DayReadAcquisitionBudget {
    pub fn try_new(
        max_candidate_items: usize,
        max_serialized_payload_bytes: usize,
    ) -> Result<Self, DayError> {
        if max_candidate_items == 0
            || max_candidate_items > MAX_DAY_SNAPSHOT_ITEMS
            || max_serialized_payload_bytes == 0
            || max_serialized_payload_bytes > MAX_DAY_SNAPSHOT_BYTES
        {
            return Err(DayError::validation("invalid Day acquisition budget"));
        }
        Ok(Self {
            max_candidate_items,
            max_serialized_payload_bytes,
        })
    }

    pub fn max_candidate_items(self) -> usize {
        self.max_candidate_items
    }

    pub fn max_serialized_payload_bytes(self) -> usize {
        self.max_serialized_payload_bytes
    }
}

impl DayReadQuery {
    pub fn display(person_id: PersonId, query: &DayQuery) -> Result<Self, DayError> {
        Ok(Self {
            person_id,
            selection: DayReadSelection::DisplayDay {
                range: query.range()?,
            },
            max_items: MAX_DAY_SNAPSHOT_ITEMS,
            max_projected_item_bytes: MAX_DAY_SNAPSHOT_BYTES,
            acquisition: None,
        })
    }

    pub fn context_evidence(
        person_id: PersonId,
        selection: DayReadSelection,
        acquisition: DayReadAcquisitionBudget,
        max_projected_item_bytes: usize,
    ) -> Result<Self, DayError> {
        let query = Self {
            person_id,
            selection,
            max_items: acquisition.max_candidate_items(),
            max_projected_item_bytes,
            acquisition: Some(acquisition),
        };
        query.validate()?;
        Ok(query)
    }

    pub fn validate(&self) -> Result<(), DayError> {
        if !self.person_id.is_valid()
            || self.max_items == 0
            || self.max_items > MAX_DAY_SNAPSHOT_ITEMS
            || self.max_projected_item_bytes == 0
            || self.max_projected_item_bytes > MAX_DAY_SNAPSHOT_BYTES
            || self
                .acquisition
                .is_some_and(|budget| budget.max_candidate_items != self.max_items)
            || (self.acquisition.is_some()
                && !matches!(
                    &self.selection,
                    DayReadSelection::OpenTasks | DayReadSelection::CurrentNotes
                ))
        {
            return Err(DayError::validation("invalid Day read budget"));
        }
        match &self.selection {
            DayReadSelection::DisplayDay { range } => {
                crate::range_bounds(range)
                    .map_err(|_| DayError::validation("invalid Day range"))?;
            }
            DayReadSelection::ActionWindow { starts_at, ends_at }
                if starts_at >= ends_at
                    || ends_at.signed_duration_since(*starts_at) > chrono::Duration::days(32) =>
            {
                return Err(DayError::validation("invalid Action event window"));
            }
            _ => {}
        }
        Ok(())
    }
    /// Storage may preselect a conservative physical superset, then calls this
    /// exact owner predicate before accumulating a selected result.
    pub fn selects(&self, item: &TimelineItem) -> Result<bool, DayError> {
        self.validate()?;
        let (person, id, revision, deleted) = match item {
            TimelineItem::Event(value) => (
                value.person_id,
                value.id.0,
                value.revision,
                value.deleted_at,
            ),
            TimelineItem::Task(value) => (
                value.person_id,
                value.id.0,
                value.revision,
                value.deleted_at,
            ),
            TimelineItem::Note(value) => (
                value.person_id,
                value.id.0,
                value.revision,
                value.deleted_at,
            ),
        };
        if person != self.person_id
            || id.is_nil()
            || revision.0 == 0
            || revision.0 > i64::MAX as u64
        {
            return Err(DayError::storage("invalid Day record identity"));
        }
        if deleted.is_some() {
            return Ok(false);
        }
        Ok(match (&self.selection, item) {
            (DayReadSelection::DisplayDay { range }, TimelineItem::Event(value)) => {
                range.contains(&value.schedule)
            }
            (DayReadSelection::DisplayDay { range }, TimelineItem::Task(value)) => {
                let (start, end) = crate::range_bounds(range)
                    .map_err(|_| DayError::validation("invalid Day range"))?;
                value.created_at < end
                    && value
                        .completed_at
                        .is_none_or(|completed| completed >= start)
            }
            (DayReadSelection::DisplayDay { range }, TimelineItem::Note(value)) => {
                let (start, end) = crate::range_bounds(range)
                    .map_err(|_| DayError::validation("invalid Day range"))?;
                value.created_at >= start && value.created_at < end
            }
            (DayReadSelection::ActionWindow { starts_at, ends_at }, TimelineItem::Event(value)) => {
                match &value.schedule {
                    EventSchedule::Timed(value) => {
                        value.starts_at < *ends_at && value.ends_at > *starts_at
                    }
                    EventSchedule::AllDay(value) => value
                        .start_date
                        .and_hms_opt(0, 0, 0)
                        .and_then(|value| {
                            value
                                .and_utc()
                                .checked_sub_signed(chrono::Duration::hours(14))
                        })
                        .zip(
                            value
                                .end_date_exclusive
                                .and_hms_opt(0, 0, 0)
                                .and_then(|value| {
                                    value
                                        .and_utc()
                                        .checked_add_signed(chrono::Duration::hours(14))
                                }),
                        )
                        .is_none_or(|(start, end)| start < *ends_at && end > *starts_at),
                }
            }
            (DayReadSelection::OpenTasks, TimelineItem::Task(value)) => {
                value.completed_at.is_none()
            }
            (DayReadSelection::CurrentNotes, TimelineItem::Note(_)) => true,
            _ => false,
        })
    }
    pub fn item_bytes(&self, item: &TimelineItem) -> Result<usize, DayError> {
        let value = match item {
            TimelineItem::Event(value) => DayTimelineItem::Event(crate::project_event(value)),
            TimelineItem::Task(value) => DayTimelineItem::Task(crate::project_task(value)),
            TimelineItem::Note(value) => DayTimelineItem::Note(crate::project_note(value)),
        };
        struct Counter {
            bytes: usize,
            maximum: usize,
        }
        impl std::io::Write for Counter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.bytes = self
                    .bytes
                    .checked_add(bytes.len())
                    .filter(|count| *count <= self.maximum)
                    .ok_or_else(|| std::io::Error::other("Day read byte budget"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Counter {
            bytes: 0,
            maximum: self.max_projected_item_bytes,
        };
        serde_json::to_writer(&mut count, &value)
            .map_err(|_| DayError::budget("Day read byte budget"))?;
        Ok(count.bytes)
    }
}
