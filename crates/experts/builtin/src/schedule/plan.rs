//! Request-only planning for the Schedule Expert.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

use floe_agent_contract::InferencePolicyDecision;
use floe_agent_contract::{AgentFailure, DataClass, ModelPlacement};
use floe_day::CalendarRange;

/// The explicit user shortcut that asks for a protected focus window today.
pub const FOCUS_REQUEST: &str = "/focus";

/// The shortest focus window the Expert will propose from now.
const FOCUS_LEAD: Duration = Duration::minutes(1);

/// The requested window and intent, independent of available sources.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleRequestPlan {
    pub range: CalendarRange,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub propose_focus: bool,
}

/// Decide which bounded interval this request needs before reading a source.
pub fn plan_request(
    assignment: &str,
    local_now: DateTime<chrono::Local>,
    now: DateTime<Utc>,
) -> Result<ScheduleRequestPlan, AgentFailure> {
    let range = requested_range(assignment, local_now)?;
    let (mut starts_at, ends_at) = day_bounds(&range)?;
    let propose_focus = assignment.trim() == FOCUS_REQUEST;
    if propose_focus {
        starts_at = starts_at.max(now + FOCUS_LEAD);
        if starts_at >= ends_at {
            return Err(AgentFailure::CapabilityUnavailable);
        }
    }
    Ok(ScheduleRequestPlan {
        range,
        starts_at,
        ends_at,
        propose_focus,
    })
}

/// Resolve the bounded calendar interval named by an assignment. An explicit
/// date pair is inclusive to the Person and exclusive at the source boundary.
pub fn requested_range(
    assignment: &str,
    local_now: DateTime<chrono::Local>,
) -> Result<CalendarRange, AgentFailure> {
    let today = local_now.date_naive();
    let dates = assignment
        .split_whitespace()
        .filter_map(|token| {
            let token = token
                .trim_matches(|character: char| !character.is_ascii_digit() && character != '-');
            (token.len() == 10
                && token.as_bytes().get(4) == Some(&b'-')
                && token.as_bytes().get(7) == Some(&b'-'))
            .then(|| NaiveDate::parse_from_str(token, "%Y-%m-%d"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let (start_date, end_date_exclusive) = match dates.as_slice() {
        [] if assignment.to_ascii_lowercase().contains("this week") => {
            let monday = today - Duration::days(today.weekday().num_days_from_monday().into());
            (monday, monday + Duration::days(7))
        }
        [] => (today, today + Duration::days(1)),
        [date] => (
            *date,
            date.checked_add_signed(Duration::days(1))
                .ok_or(AgentFailure::InvalidInput)?,
        ),
        [start, end] => (
            *start,
            end.checked_add_signed(Duration::days(1))
                .ok_or(AgentFailure::InvalidInput)?,
        ),
        _ => return Err(AgentFailure::InvalidInput),
    };
    let range = CalendarRange {
        start_date,
        end_date_exclusive,
        timezone_offset_seconds: local_now.offset().local_minus_utc(),
        end_timezone_offset_seconds: None,
    };
    if !range.is_valid() || (end_date_exclusive - start_date).num_days() > 31 {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(range)
}

/// The Context/source policy this run's model call carries.
///
/// This carries Context/source semantics only (data classes, freshness,
/// bounds): it no longer selects a provider placement and never authorizes
/// model transfer. Canonical Inference maps the run's intent to an execution
/// constraint, and Access fences the dispatch.
pub fn run_policy(data_class: DataClass) -> InferencePolicyDecision {
    InferencePolicyDecision {
        purpose: "everyday_assistance".into(),
        data_classes: vec![data_class],
        allowed_placements: vec![ModelPlacement::DeviceLocal, ModelPlacement::Remote],
        performance_class: "interactive".into(),
        projection_version: 1,
        external_transfer_consent: floe_agent_contract::TransferConsent::NotGranted,
        bounded_sensitive_projection: false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduleExecutionIntent {
    requirement: floe_agent_contract::ExpertModelRequirement,
}

impl ScheduleExecutionIntent {
    pub fn new(requirement: floe_agent_contract::ExpertModelRequirement) -> Self {
        Self { requirement }
    }

    pub fn requirement(self) -> floe_agent_contract::ExpertModelRequirement {
        self.requirement
    }
}

/// The instants one calendar day covers, in the offsets the range declares.
pub fn day_bounds(range: &CalendarRange) -> Result<(DateTime<Utc>, DateTime<Utc>), AgentFailure> {
    if !range.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let start = range
        .start_date
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        - Duration::seconds(i64::from(range.timezone_offset_seconds));
    let end = range
        .end_date_exclusive
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        - Duration::seconds(i64::from(
            range
                .end_timezone_offset_seconds
                .unwrap_or(range.timezone_offset_seconds),
        ));
    Ok((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn requested_intervals_are_not_reduced_to_today() {
        let local = Utc
            .with_ymd_and_hms(2026, 9, 23, 12, 0, 0)
            .unwrap()
            .with_timezone(&chrono::Local);
        let today = local.date_naive();
        let week_start = today - Duration::days(today.weekday().num_days_from_monday().into());
        let today_range = requested_range("today", local).unwrap();
        assert_eq!(today_range.start_date, today);
        assert_eq!(today_range.end_date_exclusive, today + Duration::days(1));

        let week_range = requested_range("What is this week like?", local).unwrap();
        assert_eq!(week_range.start_date, week_start);
        assert_eq!(
            week_range.end_date_exclusive,
            week_start + Duration::days(7)
        );

        let historical = requested_range("Review 2026-08-03 to 2026-08-09", local).unwrap();
        assert_eq!(historical.start_date.to_string(), "2026-08-03");
        assert_eq!(historical.end_date_exclusive.to_string(), "2026-08-10");

        let future = requested_range("Look at 2026-10-12", local).unwrap();
        assert_eq!(future.start_date.to_string(), "2026-10-12");
        assert_eq!(future.end_date_exclusive.to_string(), "2026-10-13");
    }

    #[test]
    fn invalid_explicit_intervals_are_rejected() {
        let local = chrono::Local::now();
        assert_eq!(
            requested_range("2026-09-10 to 2026-09-01", local),
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(
            requested_range("2026-01-01 to 2026-02-02", local),
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(
            requested_range("2026-01-01 2026-01-02 2026-01-03", local),
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(
            requested_range("Review 2026-02-30", local),
            Err(AgentFailure::InvalidInput)
        );
    }

    #[test]
    fn a_focus_request_needs_a_window_that_has_not_passed() {
        let local = chrono::Local::now();
        let now = Utc::now();
        let plan = plan_request("/focus", local, now).unwrap();
        assert!(plan.propose_focus);
        assert!(plan.starts_at >= now);
        assert!(
            !plan_request("what is today like?", local, now)
                .unwrap()
                .propose_focus
        );
        assert!(plan_request("/focus", local, now + Duration::days(2)).is_err());
    }
}
