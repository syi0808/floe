//! Asking the bundled host what the device can say about attention.

use floe_context_contract::{AttentionView, validate_attention_view};
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

/// An Attention result that has passed its mode, schema, freshness and size
/// checks while its exact broker request was still live.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedAttentionAcquisitionResult {
    response: AttentionAcquisitionResult,
    projection: Option<AttentionView>,
}

impl ValidatedAttentionAcquisitionResult {
    pub fn into_parts(self) -> (AttentionAcquisitionResult, Option<AttentionView>) {
        (self.response, self.projection)
    }
}

fn validate_response_payload(
    response: &AttentionAcquisitionResult,
) -> Result<Option<AttentionView>, AgentFailure> {
    match (response.mode, response.view.as_ref()) {
        (AttentionAcquisitionMode::ReadProjection, Some(value)) => {
            let view: AttentionView =
                serde_json::from_value(value.clone()).map_err(|_| AgentFailure::InvalidInput)?;
            validate_attention_view(&view, chrono::Utc::now().timestamp_millis())?;
            Ok(Some(view))
        }
        (AttentionAcquisitionMode::InspectSubject, None) => Ok(None),
        _ => Err(AgentFailure::InvalidInput),
    }
}

pub struct AttentionExchange;

pub type AttentionBroker = AcquisitionBroker<AttentionExchange>;

impl AcquisitionExchange for AttentionExchange {
    type Request = AttentionAcquisitionRequest;
    type Response = AttentionAcquisitionResult;
    type Accepted = ValidatedAttentionAcquisitionResult;

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

    fn admit(
        request: &Self::Request,
        response: &Self::Response,
    ) -> CompletionOutcome<Self::Accepted> {
        let projection = match validate_response_payload(response) {
            Ok(projection) => projection,
            Err(failure) => return CompletionOutcome::Refuse(failure),
        };
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
        CompletionOutcome::Accept(ValidatedAttentionAcquisitionResult {
            response: response.clone(),
            projection,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_context_contract::{ATTENTION_VIEW_ID, AttentionState, AttentionView};

    fn request() -> AttentionAcquisitionRequest {
        AttentionAcquisitionRequest {
            request_id: Uuid::new_v4(),
            host_epoch: "host-epoch".into(),
            person_id: PersonId::new(),
            device_id: "device".into(),
            mode: AttentionAcquisitionMode::ReadProjection,
            deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
            expected_native_subject_fingerprint: Some("subject".into()),
        }
    }

    fn response(request: &AttentionAcquisitionRequest) -> AttentionAcquisitionResult {
        let now = chrono::Utc::now().timestamp_millis();
        let view = AttentionView {
            schema_version: floe_kernel::AGENT_VERSION,
            view_id: ATTENTION_VIEW_ID.into(),
            source_handle: "native-attention".into(),
            observed_at_unix_ms: now - 1_000,
            expires_at_unix_ms: now + 10_000,
            state: AttentionState::Unknown,
            confidence_millis: 0,
            evidence_handles: Vec::new(),
        };
        AttentionAcquisitionResult {
            request_id: request.request_id,
            host_epoch: request.host_epoch.clone(),
            person_id: request.person_id,
            device_id: request.device_id.clone(),
            mode: request.mode,
            native_subject_fingerprint_before: "subject".into(),
            native_subject_fingerprint_after: "subject".into(),
            permission_class: "session_observation".into(),
            view: Some(serde_json::to_value(view).unwrap()),
        }
    }

    #[test]
    fn matching_projection_is_admitted_as_a_typed_view() {
        let request = request();
        let response = response(&request);
        let CompletionOutcome::Accept(accepted) = AttentionExchange::admit(&request, &response)
        else {
            panic!("valid Attention view must be admitted");
        };
        let (response, projection) = accepted.into_parts();
        assert_eq!(response.request_id, request.request_id);
        assert!(projection.is_some());
    }

    #[test]
    fn wrong_mode_subject_and_epoch_are_rejected() {
        let request = request();

        let mut wrong_mode = response(&request);
        wrong_mode.mode = AttentionAcquisitionMode::InspectSubject;
        wrong_mode.view = None;
        assert!(matches!(
            AttentionExchange::admit(&request, &wrong_mode),
            CompletionOutcome::Reject(AgentFailure::StaleContext)
        ));

        let mut wrong_subject = response(&request);
        wrong_subject.native_subject_fingerprint_before = "other-subject".into();
        wrong_subject.native_subject_fingerprint_after = "other-subject".into();
        assert!(matches!(
            AttentionExchange::admit(&request, &wrong_subject),
            CompletionOutcome::Reject(AgentFailure::StaleContext)
        ));

        let mut wrong_epoch = response(&request);
        wrong_epoch.host_epoch = "replaced-host".into();
        assert!(matches!(
            AttentionExchange::admit(&request, &wrong_epoch),
            CompletionOutcome::Reject(AgentFailure::StaleContext)
        ));
    }

    #[test]
    fn stale_attention_projection_is_refused_with_the_contract_error() {
        let request = request();
        let now = chrono::Utc::now().timestamp_millis();
        let mut response = response(&request);
        response.view = Some(serde_json::json!({
            "schema_version": floe_kernel::AGENT_VERSION,
            "view_id": ATTENTION_VIEW_ID,
            "source_handle": "native-attention",
            "observed_at_unix_ms": now - 180_000,
            "expires_at_unix_ms": now - 1,
            "state": "unknown",
            "confidence_millis": 0,
            "evidence_handles": []
        }));

        assert!(matches!(
            AttentionExchange::admit(&request, &response),
            CompletionOutcome::Refuse(AgentFailure::InvalidInput)
        ));
    }
}
