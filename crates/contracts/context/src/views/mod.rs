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

#[cfg(test)]
mod freshness_tests {
    use super::*;
    use floe_kernel::{AGENT_VERSION, AgentFailure};

    #[test]
    fn signed_freshness_rejects_extreme_wire_timestamps_without_panicking() {
        for (observed, expires, now, expected) in [
            (i64::MIN, 1, 0, Err(AgentFailure::InvalidInput)),
            (-1, i64::MAX, 0, Err(AgentFailure::InvalidInput)),
            (i64::MIN, i64::MAX, 0, Err(AgentFailure::InvalidInput)),
            (0, 300_000, 0, Ok(())),
            (0, 300_001, 0, Err(AgentFailure::InvalidInput)),
            (1, 2, 0, Err(AgentFailure::InvalidInput)),
            (0, 1, 1, Err(AgentFailure::InvalidInput)),
            (2, 1, 0, Err(AgentFailure::InvalidInput)),
            (-100, 100, 0, Ok(())),
        ] {
            let people = PeopleView {
                schema_version: AGENT_VERSION,
                view_id: PEOPLE_VIEW_ID.into(),
                source_handle: "test-source".into(),
                observed_at_unix_ms: observed,
                expires_at_unix_ms: expires,
                coverage_complete: true,
                identities: vec![],
            };
            assert_eq!(validate_people_view(&people, now), expected);
            let mail = CommunicationView {
                schema_version: AGENT_VERSION,
                view_id: COMMUNICATION_VIEW_ID.into(),
                source_handle: "test-source".into(),
                observed_at_unix_ms: observed,
                expires_at_unix_ms: expires,
                coverage_complete: true,
                next_cursor: None,
                items: vec![],
            };
            assert_eq!(
                validate_communication_view(&mail, now, 100, 65536),
                expected
            );
            let interactions = ConfirmedInteractionView {
                schema_version: AGENT_VERSION,
                view_id: CONFIRMED_INTERACTION_VIEW_ID.into(),
                source_handle: "test-source".into(),
                observed_at_unix_ms: observed,
                expires_at_unix_ms: expires,
                interactions: vec![],
            };
            assert_eq!(
                validate_confirmed_interaction_view(&interactions, &people, now),
                expected
            );
            let calendar = CalendarContextView {
                schema_version: AGENT_VERSION,
                view_id: CALENDAR_CONTEXT_VIEW_ID.into(),
                source_handle: "test-source".into(),
                observed_at_unix_ms: observed,
                expires_at_unix_ms: expires,
                range_start_unix_ms: 0,
                range_end_unix_ms: 1,
                coverage_complete: true,
                next_cursor: None,
                items: vec![],
            };
            assert_eq!(validate_calendar_context_view(&calendar, now), expected);
            let logistics = LogisticsView {
                schema_version: AGENT_VERSION,
                view_id: LOGISTICS_VIEW_ID.into(),
                source_handle: "test-source".into(),
                observed_at_unix_ms: observed,
                expires_at_unix_ms: expires,
                coverage_complete: true,
                items: vec![],
            };
            assert_eq!(validate_logistics_view(&logistics, now), expected);
        }
    }
}
