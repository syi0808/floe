//! Asking the bundled host for one of the Person's own device sources.

use floe_kernel::{AgentFailure, PersonId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AcquisitionBroker, AcquisitionExchange, CompletionOutcome};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersonalDomain {
    People,
    Wellbeing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonalAcquisitionMode {
    ReadProjection,
    InspectSubject,
    InspectCatalog,
    RequestPermission,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSourceResource {
    pub handle: String,
    pub label: String,
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
    pub mode: PersonalAcquisitionMode,
    pub selected_handles: Vec<String>,
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
    pub mode: PersonalAcquisitionMode,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub permission_class: String,
    pub provider: String,
    pub view: Option<serde_json::Value>,
    pub transform_operation_id: Option<Uuid>,
    pub resources: Vec<NativeSourceResource>,
    pub catalog_complete: bool,
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
            || request.mode != response.mode
            // The subject must not move underneath the read.
            || (request.mode != PersonalAcquisitionMode::RequestPermission && response.native_subject_fingerprint_before
                != response.native_subject_fingerprint_after)
            || request
                .expected_native_subject_fingerprint
                .as_deref()
                .is_some_and(|expected| expected != response.native_subject_fingerprint_before)
        {
            return CompletionOutcome::Reject(AgentFailure::AccessReviewRequired);
        }
        if (request.mode == PersonalAcquisitionMode::ReadProjection
            && request.domain == PersonalDomain::Wellbeing
            && response.view.is_some())
            != response.transform_operation_id.is_some()
        {
            return CompletionOutcome::Reject(AgentFailure::PolicyDenied);
        }
        if request.mode != PersonalAcquisitionMode::ReadProjection
            && (response.view.is_some() || response.transform_operation_id.is_some())
        {
            return CompletionOutcome::Reject(AgentFailure::PolicyDenied);
        }
        if request.mode == PersonalAcquisitionMode::InspectCatalog {
            if response.resources.len() > 256
                || response.resources.iter().any(|resource| {
                    resource.handle.is_empty()
                        || resource.handle.len() > 512
                        || resource.handle.chars().any(char::is_control)
                        || resource.label.is_empty()
                        || resource.label.len() > 256
                        || resource.label.chars().any(char::is_control)
                })
                || response
                    .resources
                    .iter()
                    .map(|resource| &resource.handle)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != response.resources.len()
            {
                return CompletionOutcome::Reject(AgentFailure::PolicyDenied);
            }
        } else if !response.resources.is_empty() || response.catalog_complete {
            return CompletionOutcome::Reject(AgentFailure::PolicyDenied);
        }
        CompletionOutcome::Accept
    }
}
