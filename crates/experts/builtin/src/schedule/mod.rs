//! The Schedule Expert: what it is, what it decides before it runs, and the
//! calendar-aware history its own turn is bounded to.

mod calendar_history;
pub mod definition;
mod host;
mod plan;

pub use calendar_history::CalendarHistoryBoundary;
pub use definition::{SCHEDULE_DEFINITION_REVISION, schedule_definition};
pub use host::*;
pub use plan::{
    FOCUS_REQUEST, ScheduleExecutionIntent, ScheduleRequestPlan, day_bounds, plan_request,
    requested_range, run_policy,
};
