pub(crate) mod action_collection;
mod calendar;
mod capture;
mod entity;
mod product;
mod projection;
mod refresh;

pub use action_collection::{
    ActionCollectionSource, CalendarActionCollection, DayCollectionCommit, DayCollectionReceipt,
};
pub use calendar::{
    CalendarBatch, CalendarExternalRevision, CalendarFailure, CalendarMirror,
    CalendarMirrorSourceState, CalendarMirrorState, CalendarRange, CalendarRecord,
    CalendarSelection, CalendarSource, CalendarSyncStatus, MAX_CALENDAR_NAME_BYTES,
};
pub use capture::{Capture, CaptureProcessing, CaptureSource, DomainRef};
pub use entity::{
    AllDaySchedule, DomainError, Event, EventSchedule, Note, Priority, SourceRef, Task,
    TimedSchedule,
};
pub use floe_kernel::{CaptureId, EventId, NoteId, PersonId, Revision, TaskId};
pub use product::*;
pub use projection::{
    DaySnapshot, MAX_DAY_SNAPSHOT_BYTES, MAX_DAY_SNAPSHOT_ITEMS, TimelineItem, project_day,
    project_day_with_end_offset,
};
pub use refresh::*;

mod read;
pub use read::{DayReadAcquisitionBudget, DayReadQuery, DayReadSelection};

mod manual_calendar_operation;
mod mutation;
pub use manual_calendar_operation::{
    ManualCalendarDestination, ManualCalendarOperation, ManualCalendarOperationPage,
    ManualCalendarOperationReceipt, ManualCalendarOperationStatus,
};
pub use mutation::{
    Classification, DayMutation, DayMutationApplied, DayMutationCommand, DayMutationPrior,
    DayMutationRequest, DayMutationResult, DayMutationTarget, MAX_DAY_COMMAND_RECEIPTS,
    MAX_DAY_MUTATION_BYTES, MAX_DAY_MUTATION_RECEIPT_BYTES,
};

mod cache_status;
pub use cache_status::{CalendarCacheInspection, CalendarCacheSourceStatus};

mod write_fence;
pub use write_fence::DayWriteFence;
