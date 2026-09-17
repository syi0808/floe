//! Asking the bundled host what the device can say about attention.

use floe_kernel::{AgentFailure, PersonId};
use uuid::Uuid;

use super::{AcquisitionBroker, AcquisitionExchange, CompletionOutcome};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttentionAcquisitionMode {
    InspectSubject,
    ReadProjection,
}

/// How the host's reported failure reaches the waiter.
///
/// An unrecognised code is not translated into a permissive result; the caller
/// rejects the report instead.
pub fn attention_failure(failure: &str) -> Option<AgentFailure> {
    match failure {
        "permission_denied" => Some(AgentFailure::PolicyDenied),
        "attention_unavailable" | "provider_unavailable" => {
            Some(AgentFailure::CapabilityUnavailable)
        }
        "cancelled" => Some(AgentFailure::Cancelled),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AttentionAcquisitionRequest {
    pub request_id: Uuid,
    pub host_epoch: String,
    pub person_id: PersonId,
    pub device_id: String,
    pub mode: AttentionAcquisitionMode,
    pub deadline_unix_ms: i64,
    pub expected_native_subject_fingerprint: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AttentionAcquisitionResult {
    pub request_id: Uuid,
    pub host_epoch: String,
    pub person_id: PersonId,
    pub device_id: String,
    pub mode: AttentionAcquisitionMode,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub permission_class: String,
    pub view: Option<serde_json::Value>,
}

pub struct AttentionExchange;

pub type AttentionBroker = AcquisitionBroker<AttentionExchange>;

impl AcquisitionExchange for AttentionExchange {
    type Request = AttentionAcquisitionRequest;
    type Response = AttentionAcquisitionResult;

    const MISSING_DEADLINE_EXPIRES: bool = false;
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
        if request.person_id != response.person_id
            || request.device_id != response.device_id
            || request.host_epoch != response.host_epoch
            || request.mode != response.mode
            || (request.mode == AttentionAcquisitionMode::ReadProjection
                && request.expected_native_subject_fingerprint.as_deref()
                    != Some(response.native_subject_fingerprint_before.as_str()))
        {
            return CompletionOutcome::Reject(AgentFailure::StaleContext);
        }
        CompletionOutcome::Accept
    }
}
