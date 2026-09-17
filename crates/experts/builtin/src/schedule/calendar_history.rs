//! What the Schedule Expert counts as calendar-derived history.
//!
//! Which capability results and which delegated answers carry a Person's
//! calendar is this Expert's own knowledge. What a turn may still be shown of
//! that history is the transcript owner's.

use floe_agent_contract::SourceHistoryBoundary;

use crate::BuiltinExpertKind;

/// The boundary a calendar read leaves in a transcript.
pub struct CalendarHistoryBoundary;

impl SourceHistoryBoundary for CalendarHistoryBoundary {
    fn capability_carries_source(&self, capability_id: &str) -> bool {
        capability_id.starts_with("calendar.") || capability_id.starts_with("schedule.")
    }

    fn delegation_carries_source(
        &self,
        agent_id: &str,
        completed: bool,
        has_artifacts: bool,
    ) -> bool {
        (agent_id == BuiltinExpertKind::Schedule.package_id() || agent_id == "schedule")
            && (completed || has_artifacts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settled_schedule_delegation_is_a_calendar_boundary_under_either_identity() {
        let boundary = CalendarHistoryBoundary;
        for agent_id in [BuiltinExpertKind::Schedule.package_id(), "schedule"] {
            assert!(boundary.delegation_carries_source(agent_id, true, false));
            assert!(boundary.delegation_carries_source(agent_id, false, true));
            assert!(!boundary.delegation_carries_source(agent_id, false, false));
        }
        assert!(!boundary.delegation_carries_source("mail", true, true));
        assert!(boundary.capability_carries_source("calendar.read"));
        assert!(boundary.capability_carries_source("schedule.find_free_windows"));
        assert!(!boundary.capability_carries_source("mail.search"));
    }
}
