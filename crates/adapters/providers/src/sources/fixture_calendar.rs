//! Deterministic synthetic Calendar facts for explicit Linux QA builds.
//!
//! This module is the fixture's external boundary. It never reads a host
//! calendar or mutates one; Connections, Access and Context still own source
//! setup, selection, grants and read admission.

use floe_context_contract::CalendarProvider;
use floe_kernel::AgentFailure;
use floe_native::{
    CalendarAcquisitionMode, CalendarAcquisitionRequest, CalendarAcquisitionResult,
    NativeCalendarBatch, NativeCalendarRecord, NativeEventSchedule, NativeResourceGroup,
    NativeSourceResource,
};
use sha2::{Digest, Sha256};

const CONNECTION: &str = "calendar.fixture.local";
const SUBJECT_DOMAIN: &[u8] = b"floe.qa.synthetic-calendar.subject.v1\0";
const MAX_READ_RANGE_MS: i64 = 32 * 24 * 60 * 60 * 1000;

const TEAM_CALENDAR: &str = "fixture.calendar.team";
const PRIVATE_CALENDAR: &str = "fixture.calendar.private";
const DENIED_CALENDAR: &str = "fixture.calendar.denied";
const TEAM_EVENT_ID: &str = "synthetic-event-team-1";
const TEAM_EVENT_REVISION: &str =
    "0000000000000000000000000000000000000000000000000000000000000001";
const TEAM_EVENT_START: &str = "2026-10-08T10:00:00Z";
const TEAM_EVENT_END: &str = "2026-10-08T10:30:00Z";
const PRIVATE_EVENT_ID: &str = "synthetic-event-private-1";
const PRIVATE_EVENT_REVISION: &str =
    "0000000000000000000000000000000000000000000000000000000000000001";
const PRIVATE_EVENT_START: &str = "2026-10-09T13:00:00Z";
const PRIVATE_EVENT_END: &str = "2026-10-09T13:30:00Z";

#[derive(Clone, Copy)]
struct FixtureCalendar {
    handle: &'static str,
    label: &'static str,
}

const CALENDARS: &[FixtureCalendar] = &[
    FixtureCalendar {
        handle: DENIED_CALENDAR,
        label: "Synthetic permission-denied calendar",
    },
    FixtureCalendar {
        handle: PRIVATE_CALENDAR,
        label: "Synthetic unselected calendar",
    },
    FixtureCalendar {
        handle: TEAM_CALENDAR,
        label: "Synthetic team calendar",
    },
];

/// Stable host-generation tag for this source adapter only. It is not an
/// Apple/native host epoch and cannot be used to dispatch native requests.
pub(super) fn host_epoch(device_id: &str) -> String {
    format!("fixture-host:{device_id}")
}

pub(super) fn respond(
    request: &CalendarAcquisitionRequest,
) -> Result<CalendarAcquisitionResult, AgentFailure> {
    validate_request(request)?;
    let subject = subject_fingerprint(request);
    let inventory = match request.mode {
        CalendarAcquisitionMode::RequestPermission => Vec::new(),
        _ => inventory(),
    };
    let ids = inventory
        .iter()
        .map(|resource| resource.handle.clone())
        .collect::<Vec<_>>();

    let permission_class = match request.mode {
        CalendarAcquisitionMode::RequestPermission => "request_completed",
        CalendarAcquisitionMode::ReadEvents
            if request.calendar_ids.iter().any(|id| id == DENIED_CALENDAR) =>
        {
            "denied"
        }
        _ => "authorized",
    };

    let batches = if request.mode == CalendarAcquisitionMode::ReadEvents
        && permission_class == "authorized"
    {
        request
            .calendar_ids
            .iter()
            .map(|calendar_id| batch(calendar_id, request))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    Ok(CalendarAcquisitionResult {
        request_id: request.request_id,
        host_epoch: request.host_epoch.clone(),
        person_id: request.person_id,
        device_id: request.device_id.clone(),
        connection_id: request.connection_id.clone(),
        connection_revision: request.connection_revision,
        provider: request.provider,
        mode: request.mode,
        calendar_ids: request.calendar_ids.clone(),
        range_start_unix_ms: request.range_start_unix_ms,
        range_end_unix_ms: request.range_end_unix_ms,
        native_subject_fingerprint_before: subject.clone(),
        native_subject_fingerprint_after: subject,
        available_calendar_ids: ids,
        available_calendars: inventory,
        permission_class: permission_class.to_owned(),
        batches,
    })
}

fn validate_request(request: &CalendarAcquisitionRequest) -> Result<(), AgentFailure> {
    if request.request_id.is_nil()
        || request.person_id.0.is_nil()
        || request.provider != CalendarProvider::Fixture
        || request.connection_id != CONNECTION
        || request.connection_revision == 0
        || request.device_id.trim().is_empty()
        || request.device_id.trim() != request.device_id
        || request.device_id.len() > 128
        || request.device_id.chars().any(char::is_control)
        || request.host_epoch != host_epoch(&request.device_id)
        || request.range_start_unix_ms < 0
        || request.range_end_unix_ms <= request.range_start_unix_ms
        || request.deadline_unix_ms <= chrono::Utc::now().timestamp_millis()
        || request.calendar_ids.len() > CALENDARS.len()
        || request
            .calendar_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || request.calendar_ids.iter().any(|id| !known_calendar(id))
    {
        return Err(AgentFailure::PolicyDenied);
    }

    match request.mode {
        CalendarAcquisitionMode::RequestPermission
            if !request.calendar_ids.is_empty()
                || request.expected_native_subject_fingerprint.is_some() =>
        {
            Err(AgentFailure::InvalidInput)
        }
        CalendarAcquisitionMode::InspectCatalog
            if !request.calendar_ids.is_empty()
                || request.expected_native_subject_fingerprint.is_some() =>
        {
            Err(AgentFailure::InvalidInput)
        }
        CalendarAcquisitionMode::InspectSubject if request.calendar_ids.is_empty() => {
            Err(AgentFailure::InvalidInput)
        }
        CalendarAcquisitionMode::ReadEvents
            if request.calendar_ids.is_empty()
                || request.expected_native_subject_fingerprint.as_deref()
                    != Some(subject_fingerprint(request).as_str())
                || request.range_end_unix_ms - request.range_start_unix_ms > MAX_READ_RANGE_MS =>
        {
            Err(AgentFailure::AccessReviewRequired)
        }
        _ => Ok(()),
    }
}

fn inventory() -> Vec<NativeSourceResource> {
    let group = NativeResourceGroup {
        handle: "fixture.qa".into(),
        label: "Synthetic QA calendars".into(),
    };
    CALENDARS
        .iter()
        .map(|calendar| NativeSourceResource {
            handle: calendar.handle.to_owned(),
            label: calendar.label.to_owned(),
            group: Some(group.clone()),
        })
        .collect()
}

fn known_calendar(handle: &str) -> bool {
    CALENDARS.iter().any(|calendar| calendar.handle == handle)
}

fn subject_fingerprint(request: &CalendarAcquisitionRequest) -> String {
    let mut hash = Sha256::new();
    hash.update(SUBJECT_DOMAIN);
    hash.update(request.person_id.to_string().as_bytes());
    hash.update([0]);
    hash.update(request.device_id.as_bytes());
    hash.update([0]);
    hash.update(request.connection_id.as_bytes());
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn batch(
    calendar_id: &str,
    request: &CalendarAcquisitionRequest,
) -> Result<NativeCalendarBatch, AgentFailure> {
    let (event_id, event_revision, title, starts_at, ends_at) = match calendar_id {
        TEAM_CALENDAR => (
            TEAM_EVENT_ID,
            TEAM_EVENT_REVISION,
            "Synthetic planning event",
            TEAM_EVENT_START,
            TEAM_EVENT_END,
        ),
        PRIVATE_CALENDAR => (
            PRIVATE_EVENT_ID,
            PRIVATE_EVENT_REVISION,
            "Unselected calendar sentinel event",
            PRIVATE_EVENT_START,
            PRIVATE_EVENT_END,
        ),
        DENIED_CALENDAR => return Err(AgentFailure::CapabilityDenied),
        _ => return Err(AgentFailure::InvalidInput),
    };
    let event_start = chrono::DateTime::parse_from_rfc3339(starts_at)
        .map_err(|_| AgentFailure::InvalidInput)?
        .timestamp_millis();
    let event_end = chrono::DateTime::parse_from_rfc3339(ends_at)
        .map_err(|_| AgentFailure::InvalidInput)?
        .timestamp_millis();
    let overlaps =
        event_start < request.range_end_unix_ms && event_end > request.range_start_unix_ms;

    Ok(NativeCalendarBatch {
        calendar_id: calendar_id.to_owned(),
        records: if overlaps {
            vec![NativeCalendarRecord {
                can_modify: false,
                calendar_id: calendar_id.to_owned(),
                external_id: event_id.to_owned(),
                external_revision: event_revision.to_owned(),
                title: title.to_owned(),
                schedule: NativeEventSchedule::Timed {
                    starts_at: starts_at.to_owned(),
                    ends_at: ends_at.to_owned(),
                    timezone: "UTC".into(),
                },
            }]
        } else {
            Vec::new()
        },
        failure: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_kernel::PersonId;
    use uuid::Uuid;

    fn request(
        mode: CalendarAcquisitionMode,
        calendar_ids: Vec<String>,
    ) -> CalendarAcquisitionRequest {
        request_in_range(
            mode,
            calendar_ids,
            "2026-10-01T00:00:00Z",
            "2026-11-01T00:00:00Z",
        )
    }

    fn request_in_range(
        mode: CalendarAcquisitionMode,
        calendar_ids: Vec<String>,
        start: &str,
        end: &str,
    ) -> CalendarAcquisitionRequest {
        let mut request = CalendarAcquisitionRequest {
            request_id: Uuid::new_v4(),
            host_epoch: host_epoch("qa-device"),
            person_id: PersonId(Uuid::new_v4()),
            device_id: "qa-device".into(),
            connection_id: CONNECTION.into(),
            connection_revision: 1,
            provider: CalendarProvider::Fixture,
            mode,
            calendar_ids,
            range_start_unix_ms: chrono::DateTime::parse_from_rfc3339(start)
                .unwrap()
                .timestamp_millis(),
            range_end_unix_ms: chrono::DateTime::parse_from_rfc3339(end)
                .unwrap()
                .timestamp_millis(),
            deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 30_000,
            expected_native_subject_fingerprint: None,
        };
        if mode == CalendarAcquisitionMode::ReadEvents {
            request.expected_native_subject_fingerprint = Some(subject_fingerprint(&request));
        }
        request
    }

    #[test]
    fn selected_reads_exclude_unselected_calendar_content() {
        let result = respond(&request(
            CalendarAcquisitionMode::ReadEvents,
            vec![TEAM_CALENDAR.into()],
        ))
        .unwrap();
        assert_eq!(result.batches.len(), 1);
        assert_eq!(result.batches[0].calendar_id, TEAM_CALENDAR);
        assert_eq!(
            result.batches[0].records[0].title,
            "Synthetic planning event"
        );
        assert!(
            !result
                .batches
                .iter()
                .any(|batch| batch.calendar_id == PRIVATE_CALENDAR)
        );
        assert!(
            result
                .batches
                .iter()
                .flat_map(|batch| &batch.records)
                .all(|record| {
                    !record.can_modify && record.title != "Unselected calendar sentinel event"
                })
        );
    }

    #[test]
    fn overlapping_ranges_keep_a_stable_synthetic_event_identity_time_and_revision() {
        let first = respond(&request(
            CalendarAcquisitionMode::ReadEvents,
            vec![TEAM_CALENDAR.into()],
        ))
        .unwrap();
        let second = respond(&request_in_range(
            CalendarAcquisitionMode::ReadEvents,
            vec![TEAM_CALENDAR.into()],
            "2026-10-07T00:00:00Z",
            "2026-10-10T00:00:00Z",
        ))
        .unwrap();
        assert_eq!(first.batches, second.batches);
        assert_eq!(first.batches[0].records[0].external_id, TEAM_EVENT_ID);
        assert_eq!(
            first.batches[0].records[0].external_revision,
            TEAM_EVENT_REVISION
        );

        let outside = respond(&request_in_range(
            CalendarAcquisitionMode::ReadEvents,
            vec![TEAM_CALENDAR.into()],
            "2026-11-02T00:00:00Z",
            "2026-11-03T00:00:00Z",
        ))
        .unwrap();
        assert!(outside.batches[0].records.is_empty());
    }

    #[test]
    fn denied_source_returns_no_calendar_payload() {
        let result = respond(&request(
            CalendarAcquisitionMode::ReadEvents,
            vec![DENIED_CALENDAR.into()],
        ))
        .unwrap();
        assert_eq!(result.permission_class, "denied");
        assert!(result.batches.is_empty());
    }
}
