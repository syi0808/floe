//! Asking the bundled host for one of the Person's own device sources.

use floe_kernel::{AgentFailure, PersonId};
use uuid::Uuid;

use super::{AcquisitionBroker, AcquisitionExchange, CompletionOutcome};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersonalDomain {
    People,
    Wellbeing,
    Feasibility,
}

/// How the host's reported failure reaches the waiter.
pub fn personal_failure(failure: &str) -> AgentFailure {
    match failure {
        "permission_denied" => AgentFailure::CapabilityDenied,
        "cancelled" => AgentFailure::Cancelled,
        _ => AgentFailure::CapabilityUnavailable,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PersonalAcquisitionRequest {
    pub request_id: Uuid,
    pub host_epoch: String,
    pub person_id: PersonId,
    pub device_id: String,
    pub domain: PersonalDomain,
    pub selected_handles: Vec<String>,
    pub event_handle: Option<String>,
    pub evidence_handles: Vec<String>,
    pub destination_latitude: Option<f64>,
    pub destination_longitude: Option<f64>,
    pub event_start_unix_ms: Option<i64>,
    pub event_end_unix_ms: Option<i64>,
    pub travel_mode: Option<String>,
    pub deadline_unix_ms: i64,
    pub expected_native_subject_fingerprint: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PersonalAcquisitionResult {
    pub request_id: Uuid,
    pub host_epoch: String,
    pub person_id: PersonId,
    pub device_id: String,
    pub domain: PersonalDomain,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub permission_class: String,
    pub provider: String,
    pub view: Option<serde_json::Value>,
}

pub struct PersonalExchange;

pub type PersonalBroker = AcquisitionBroker<PersonalExchange>;

impl AcquisitionExchange for PersonalExchange {
    type Request = PersonalAcquisitionRequest;
    type Response = PersonalAcquisitionResult;

    /// A personal read with no recorded deadline is already too late.
    const MISSING_DEADLINE_EXPIRES: bool = true;

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
        if request.host_epoch != response.host_epoch
            || request.person_id != response.person_id
            || request.device_id != response.device_id
            || request.domain != response.domain
            // The subject must not move underneath the read.
            || response.native_subject_fingerprint_before
                != response.native_subject_fingerprint_after
            || request
                .expected_native_subject_fingerprint
                .as_deref()
                .is_some_and(|expected| expected != response.native_subject_fingerprint_before)
        {
            return CompletionOutcome::Reject(AgentFailure::AccessReviewRequired);
        }
        CompletionOutcome::Accept
    }
}
