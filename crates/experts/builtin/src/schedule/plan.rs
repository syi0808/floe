//! What the Schedule Expert decides before it is given anything to read.
//!
//! Which of the Person's calendar setups this device may use, how far the day
//! it is about to look, whether the request is the focus shortcut, and whether
//! the calendar has to be acquired from the paired server rather than mirrored
//! locally — these are the Expert's own judgments. The composition root reads
//! the records and builds the readers; it does not decide any of this.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

use floe_agent_contract::InferencePolicyDecision;
use floe_agent_contract::{AgentFailure, DataClass, ExpertModelRequirement, ModelPlacement};
use floe_context_contract::CalendarProvider;
use floe_day::CalendarRange;

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

/// Where this run's own reasoning happens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleReasoning {
    /// The calendar is being pulled from the paired server for this very turn,
    /// so reasoning over what comes back stays on this device's own model. The
    /// Expert will not hand a freshly acquired remote calendar to a second
    /// remote recipient.
    OnDevice,
    /// Nothing is acquired remotely, so the turn reasons wherever the
    /// conversation's own route already put it.
    ConversationRoute,
}

/// The window, the intent, the acquisition and the reasoning this run needs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleRunPlan {
    pub range: CalendarRange,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// The turn asked for a protected focus window rather than a review.
    pub propose_focus: bool,
    /// The calendar must be read through the paired server for this provider.
    pub acquire_remotely: bool,
    /// Which model this run may reason on, given that acquisition.
    pub reasoning: ScheduleReasoning,
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
    remote_source_available: bool,
    local_now: DateTime<chrono::Local>,
    now: DateTime<Utc>,
) -> Result<ScheduleRunPlan, AgentFailure> {
    let range = requested_range(assignment, local_now)?;
    let (mut starts_at, ends_at) = day_bounds(&range)?;
    let propose_focus = assignment.trim() == FOCUS_REQUEST;
    if propose_focus {
        starts_at = starts_at.max(now + FOCUS_LEAD);
        if starts_at >= ends_at || calendar_count != 1 {
            return Err(AgentFailure::CapabilityUnavailable);
        }
    }
    let acquire_remotely = remote_source_available
        && matches!(
            provider,
            CalendarProvider::Google | CalendarProvider::Microsoft
        );
    Ok(ScheduleRunPlan {
        range,
        starts_at,
        ends_at,
        expires_at: now + GRANT_LIFETIME,
        propose_focus,
        acquire_remotely,
        reasoning: if acquire_remotely {
            ScheduleReasoning::OnDevice
        } else {
            ScheduleReasoning::ConversationRoute
        },
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

/// What execution class this Schedule run requires.
///
/// The single Schedule-owned statement of where its reasoning may happen.
/// The Expert states the class; Inference selects the profile. This is the
/// only mapping from `ScheduleReasoning` to execution: no caller consults a
/// model, placement, or route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduleExecutionIntent {
    requirement: ExpertModelRequirement,
}

impl ScheduleExecutionIntent {
    /// Map the run's reasoning to its execution class: a freshly acquired
    /// remote calendar stays on this device's own model, while anything else
    /// reasons wherever the conversation's own route already put it.
    pub fn from_reasoning(reasoning: ScheduleReasoning) -> Self {
        Self {
            requirement: match reasoning {
                ScheduleReasoning::OnDevice => ExpertModelRequirement::DeviceOnly,
                ScheduleReasoning::ConversationRoute => ExpertModelRequirement::Any,
            },
        }
    }

    pub fn requirement(self) -> ExpertModelRequirement {
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
            !plan_run(
                "what is today like?",
                CalendarProvider::Fixture,
                2,
                false,
                local,
                now
            )
            .unwrap()
            .propose_focus
        );
    }

    #[test]
    fn a_remotely_acquired_calendar_is_reasoned_over_on_this_device() {
        let local = chrono::Local::now();
        let now = Utc::now();
        assert_eq!(
            plan_run("review", CalendarProvider::Google, 1, true, local, now)
                .unwrap()
                .reasoning,
            ScheduleReasoning::OnDevice
        );
        for (provider, remote_source) in [
            (CalendarProvider::Google, false),
            (CalendarProvider::EventKit, true),
            (CalendarProvider::Fixture, true),
        ] {
            assert_eq!(
                plan_run("review", provider, 1, remote_source, local, now)
                    .unwrap()
                    .reasoning,
                ScheduleReasoning::ConversationRoute
            );
        }
    }

    #[test]
    fn execution_intent_maps_reasoning_to_requirement() {
        use floe_agent_contract::ExpertModelRequirement;

        assert_eq!(
            ScheduleExecutionIntent::from_reasoning(ScheduleReasoning::OnDevice).requirement(),
            ExpertModelRequirement::DeviceOnly
        );
        assert_eq!(
            ScheduleExecutionIntent::from_reasoning(ScheduleReasoning::ConversationRoute)
                .requirement(),
            ExpertModelRequirement::Any
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
