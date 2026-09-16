//! What the Schedule Expert decides before it is given anything to read.
//!
//! Which of the Person's calendar setups this device may use, how far the day
//! it is about to look, whether the request is the focus shortcut, and whether
//! the calendar has to be acquired from the paired server rather than mirrored
//! locally — these are the Expert's own judgments. The composition root reads
//! the records and builds the readers; it does not decide any of this.

use chrono::{DateTime, Duration, Utc};

use floe_agent_contract::{AgentFailure, DataClass, ModelPlacement, TransferConsent};
use floe_agent_contract::InferencePolicyDecision;
use floe_day::{CalendarProvider, CalendarRange};

/// The explicit user shortcut that asks for a protected focus window today.
pub const FOCUS_REQUEST: &str = "/focus";

/// The shortest focus window the Expert will propose from now.
const FOCUS_LEAD: Duration = Duration::minutes(1);

/// How long the calendar grant this run reads under stays valid.
const GRANT_LIFETIME: Duration = Duration::minutes(2);

/// One calendar setup this Person has, as the registry recorded it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleSetupCandidate {
    pub device_id: String,
    /// Whether the view binding, both installations and both assignments are
    /// all enabled. A setup that is only partly enabled is not a candidate.
    pub active: bool,
}

/// Which setup this run uses, and whether the choice was unambiguous.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduleSetupSelection {
    pub index: usize,
    /// More than one setup — or none — is bound to the calling device, so the
    /// Person has to say which one before this Expert acts on it.
    pub ambiguous: bool,
}

/// Choose the setup this device should read under.
///
/// A setup bound to the calling device wins. Falling back to another device's
/// setup is possible but never unambiguous, so the caller can offer a reviewable
/// answer instead of silently reading the wrong calendar.
pub fn select_active_setup(
    candidates: &[ScheduleSetupCandidate],
    request_device_id: &str,
) -> Result<ScheduleSetupSelection, AgentFailure> {
    if request_device_id.trim().is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    let active: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.active)
        .map(|(index, _)| index)
        .collect();
    let bound: Vec<usize> = active
        .iter()
        .copied()
        .filter(|index| candidates[*index].device_id == request_device_id)
        .collect();
    let index = bound
        .first()
        .or(active.first())
        .copied()
        .ok_or(AgentFailure::CapabilityDenied)?;
    Ok(ScheduleSetupSelection {
        index,
        ambiguous: bound.len() != 1,
    })
}

/// The window, the intent and the acquisition this run needs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleRunPlan {
    pub range: CalendarRange,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// The turn asked for a protected focus window rather than a review.
    pub propose_focus: bool,
    /// The calendar must be read through the paired server for this provider,
    /// so the turn runs on the device model and the acquisition is remote.
    pub acquire_remotely: bool,
}

/// Decide what this run looks at, and how.
///
/// A focus request needs a window that has not already passed and exactly one
/// calendar to place it in; anything else is refused rather than answered
/// against the wrong day.
pub fn plan_run(
    assignment: &str,
    provider: CalendarProvider,
    calendar_count: usize,
    remote_route_available: bool,
    local_now: DateTime<chrono::Local>,
    now: DateTime<Utc>,
) -> Result<ScheduleRunPlan, AgentFailure> {
    let range = CalendarRange {
        start_date: local_now.date_naive(),
        end_date_exclusive: local_now.date_naive() + Duration::days(1),
        timezone_offset_seconds: local_now.offset().local_minus_utc(),
        end_timezone_offset_seconds: None,
    };
    let (mut starts_at, ends_at) = day_bounds(&range)?;
    let propose_focus = assignment.trim() == FOCUS_REQUEST;
    if propose_focus {
        starts_at = starts_at.max(now + FOCUS_LEAD);
        if starts_at >= ends_at || calendar_count != 1 {
            return Err(AgentFailure::CapabilityUnavailable);
        }
    }
    Ok(ScheduleRunPlan {
        range,
        starts_at,
        ends_at,
        expires_at: now + GRANT_LIFETIME,
        propose_focus,
        acquire_remotely: remote_route_available
            && matches!(
                provider,
                CalendarProvider::Google | CalendarProvider::Microsoft
            ),
    })
}

/// The policy this run's model call is authorized under.
pub fn run_policy(
    placement: ModelPlacement,
    data_class: DataClass,
    external_transfer_consent: TransferConsent,
) -> InferencePolicyDecision {
    InferencePolicyDecision {
        purpose: "everyday-assistance".into(),
        data_classes: vec![data_class],
        allowed_placements: vec![placement],
        performance_class: "interactive".into(),
        projection_version: 1,
        external_transfer_consent,
        bounded_sensitive_projection: false,
    }
}

/// The instants one calendar day covers, in the offsets the range declares.
pub fn day_bounds(
    range: &CalendarRange,
) -> Result<(DateTime<Utc>, DateTime<Utc>), AgentFailure> {
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

    fn candidate(device_id: &str, active: bool) -> ScheduleSetupCandidate {
        ScheduleSetupCandidate {
            device_id: device_id.into(),
            active,
        }
    }

    #[test]
    fn the_calling_device_wins_and_another_device_is_never_unambiguous() {
        let candidates = [
            candidate("other", true),
            candidate("this", true),
            candidate("this", false),
        ];
        assert_eq!(
            select_active_setup(&candidates, "this").unwrap(),
            ScheduleSetupSelection {
                index: 1,
                ambiguous: false
            }
        );
        assert_eq!(
            select_active_setup(&candidates, "absent").unwrap(),
            ScheduleSetupSelection {
                index: 0,
                ambiguous: true
            }
        );
        assert_eq!(
            select_active_setup(&[candidate("this", false)], "this"),
            Err(AgentFailure::CapabilityDenied)
        );
        assert_eq!(
            select_active_setup(&candidates, "  "),
            Err(AgentFailure::InvalidInput)
        );
    }

    #[test]
    fn two_setups_on_the_same_device_stay_ambiguous() {
        let candidates = [candidate("this", true), candidate("this", true)];
        assert!(select_active_setup(&candidates, "this").unwrap().ambiguous);
    }

    #[test]
    fn a_focus_request_needs_one_calendar_and_a_window_that_has_not_passed() {
        let local = chrono::Local::now();
        let now = Utc::now();
        let plan = plan_run("/focus", CalendarProvider::Fixture, 1, false, local, now).unwrap();
        assert!(plan.propose_focus);
        assert!(plan.starts_at >= now);
        assert!(!plan.acquire_remotely);
        assert_eq!(
            plan_run("/focus", CalendarProvider::Fixture, 2, false, local, now),
            Err(AgentFailure::CapabilityUnavailable)
        );
        assert!(
            !plan_run("what is today like?", CalendarProvider::Fixture, 2, false, local, now)
                .unwrap()
                .propose_focus
        );
    }

    #[test]
    fn only_a_connector_calendar_is_acquired_through_the_paired_server() {
        let local = chrono::Local::now();
        let now = Utc::now();
        for (provider, remote) in [
            (CalendarProvider::Google, true),
            (CalendarProvider::Microsoft, true),
            (CalendarProvider::EventKit, false),
            (CalendarProvider::Fixture, false),
        ] {
            assert_eq!(
                plan_run("review", provider, 1, true, local, now)
                    .unwrap()
                    .acquire_remotely,
                remote
            );
            assert!(
                !plan_run("review", provider, 1, false, local, now)
                    .unwrap()
                    .acquire_remotely
            );
        }
    }
}
