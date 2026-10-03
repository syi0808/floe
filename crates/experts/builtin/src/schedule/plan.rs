//! Request-only planning for the Schedule Expert.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

use floe_agent_contract::AgentFailure;
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
