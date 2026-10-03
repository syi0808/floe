//! Context view shapes and their validation.
//!
//! A view is a bounded projection of a source. Context owns what a view may
//! contain and how fresh it must be; an Expert only reads it.

pub mod calendar;
pub mod communication;
pub mod interactions;
pub mod native;
pub mod personal;
pub mod portfolio;

pub use calendar::*;
pub use communication::*;
pub use interactions::*;
pub use native::*;
pub use personal::*;
pub use portfolio::*;

/// The same declaration classifies a View's evidence and its permission review.
/// Unknown Views have no inferred sensitivity.
pub fn source_view_data_class(view_id: &str) -> Option<crate::DataClass> {
    match view_id {
        CALENDAR_CONTEXT_VIEW_ID => Some(CalendarContextView::DATA_CLASS),
        COMMUNICATION_VIEW_ID => Some(CommunicationView::DATA_CLASS),
        PEOPLE_VIEW_ID => Some(PeopleView::DATA_CLASS),
        ATTENTION_VIEW_ID => Some(AttentionView::DATA_CLASS),
        WELLBEING_VIEW_ID => Some(WellbeingView::DATA_CLASS),
        WORK_CONTEXT_VIEW_ID => Some(WorkContextView::DATA_CLASS),
        LOGISTICS_VIEW_ID => Some(LogisticsView::DATA_CLASS),
        _ => None,
    }
}
