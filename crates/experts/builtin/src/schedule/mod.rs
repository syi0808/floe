//! The Schedule Expert: what it is, what it decides before it runs, and the
//! calendar-aware history its own turn is bounded to.

mod calendar_history;
mod model_history;
pub mod definition;
mod plan;
mod host;

pub use calendar_history::{has_calendar_history, project_calendar_history};
pub use model_history::bounded_model_history_start;
pub use definition::{SCHEDULE_DEFINITION_REVISION, schedule_definition};
pub use plan::{
    FOCUS_REQUEST, ScheduleRunPlan, ScheduleSetupCandidate, ScheduleSetupSelection, day_bounds,
    plan_run, run_policy, select_active_setup,
};
pub use host::*;
