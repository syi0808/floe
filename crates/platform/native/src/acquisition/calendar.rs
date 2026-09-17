//! Asking the bundled host to read the device's calendar.

use floe_context_contract::CalendarProvider;
use floe_kernel::{AgentFailure, PersonId};
use uuid::Uuid;

use super::{AcquisitionBroker, AcquisitionExchange, CompletionOutcome};
use crate::calendar_wire::NativeCalendarBatch;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalendarAcquisitionMode {
    InspectSubject,
    ReadEvents,
}

/// Why the host says it could not read the calendar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalendarSourceFailure {
    PermissionDenied,
    CalendarUnavailable,
    ProviderUnavailable,
}

/// How a source failure reaches the waiter.
pub fn calendar_failure(failure: CalendarSourceFailure) -> AgentFailure {
    match failure {
        CalendarSourceFailure::PermissionDenied => AgentFailure::CapabilityDenied,
        CalendarSourceFailure::CalendarUnavailable
        | CalendarSourceFailure::ProviderUnavailable => AgentFailure::CapabilityUnavailable,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalendarAcquisitionRequest {
    pub request_id: Uuid,
    pub host_epoch: String,
    pub person_id: PersonId,
    pub device_id: String,
    pub connection_id: String,
    pub connection_revision: u64,
    pub provider: CalendarProvider,
    pub mode: CalendarAcquisitionMode,
    pub calendar_ids: Vec<String>,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub deadline_unix_ms: i64,
    pub expected_native_subject_fingerprint: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalendarAcquisitionResult {
    pub request_id: Uuid,
    pub host_epoch: String,
    pub person_id: PersonId,
    pub device_id: String,
    pub connection_id: String,
    pub connection_revision: u64,
    pub provider: CalendarProvider,
    pub mode: CalendarAcquisitionMode,
    pub calendar_ids: Vec<String>,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub available_calendar_ids: Vec<String>,
    pub permission_class: String,
    pub batches: Vec<NativeCalendarBatch>,
}

pub struct CalendarExchange;

pub type CalendarBroker = AcquisitionBroker<CalendarExchange>;

impl AcquisitionExchange for CalendarExchange {
    type Request = CalendarAcquisitionRequest;
    type Response = CalendarAcquisitionResult;

    const MISSING_DEADLINE_EXPIRES: bool = false;
    const RESPONSE_CARRIES_HOST_EPOCH: bool = true;
    const RECHECK_AFTER_RECEIVE: bool = true;

    fn request_id(request: &Self::Request) -> Uuid {
        request.request_id
    }

    fn request_host_epoch(request: &Self::Request) -> &str {
        &request.host_epoch
    }

    fn request_person(request: &Self::Request) -> PersonId {
        request.person_id
    }

    fn request_deadline_unix_ms(request: &Self::Request) -> i64 {
        request.deadline_unix_ms
    }

    fn response_id(response: &Self::Response) -> Uuid {
        response.request_id
    }

    fn response_host_epoch(response: &Self::Response) -> &str {
        &response.host_epoch
    }

    fn admit(request: &Self::Request, response: &Self::Response) -> CompletionOutcome {
        if !identity_matches(request, response) {
            // The answer is not to this request; the request keeps waiting.
            return CompletionOutcome::Refuse(AgentFailure::StaleContext);
        }
        if response.mode == CalendarAcquisitionMode::ReadEvents
            && request.expected_native_subject_fingerprint.as_deref()
                != Some(response.native_subject_fingerprint_before.as_str())
        {
            // The subject the read stood on is not the one that was approved.
            return CompletionOutcome::RejectKeepingDeadline(AgentFailure::AccessReviewRequired);
        }
        if request
            .calendar_ids
            .iter()
            .any(|calendar_id| !response.available_calendar_ids.contains(calendar_id))
        {
            return CompletionOutcome::RejectKeepingDeadline(AgentFailure::StaleContext);
        }
        CompletionOutcome::Accept
    }
}

fn identity_matches(
    request: &CalendarAcquisitionRequest,
    response: &CalendarAcquisitionResult,
) -> bool {
    request.request_id == response.request_id
        && request.host_epoch == response.host_epoch
        && request.person_id == response.person_id
        && request.device_id == response.device_id
        && request.connection_id == response.connection_id
        && request.connection_revision == response.connection_revision
        && request.provider == response.provider
        && request.mode == response.mode
        && request.calendar_ids == response.calendar_ids
        && request.range_start_unix_ms == response.range_start_unix_ms
        && request.range_end_unix_ms == response.range_end_unix_ms
}
