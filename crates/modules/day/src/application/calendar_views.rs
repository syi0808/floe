use chrono::{DateTime, Utc};
use floe_kernel::AgentFailure;
use crate::CalendarRange;

/// The UTC interval a calendar range covers, honouring its timezone offsets.
pub fn range_bounds(range: &CalendarRange) -> Result<(DateTime<Utc>, DateTime<Utc>), AgentFailure> {
    if !range.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let start = range
        .start_date
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        .checked_sub_signed(chrono::Duration::seconds(i64::from(
            range.timezone_offset_seconds,
        )))
        .ok_or(AgentFailure::InvalidInput)?;
    let end = range
        .end_date_exclusive
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        .checked_sub_signed(chrono::Duration::seconds(i64::from(
            range
                .end_timezone_offset_seconds
                .unwrap_or(range.timezone_offset_seconds),
        )))
        .ok_or(AgentFailure::InvalidInput)?;
    Ok((start, end))
}
