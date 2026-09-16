//! The Schedule Expert: what it is, what it decides before it runs, and the
//! calendar-aware history its own turn is bounded to.

mod calendar_history;
mod model_history;
mod definition;
mod host;

pub use calendar_history::{has_calendar_history, project_calendar_history};
pub use model_history::bounded_model_history_start;
pub use definition::{SCHEDULE_DEFINITION_REVISION, schedule_definition};
pub use host::*;
