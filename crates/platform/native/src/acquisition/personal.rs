//! Asking the bundled host for one of the Person's own device sources.

use floe_context_contract::{
    PeopleView, WellbeingView, validate_people_view, validate_wellbeing_view,
};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<NativeResourceGroup>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeResourceGroup {
    pub handle: String,
    pub label: String,
}
impl NativeSourceResource {
    pub(crate) fn valid_metadata(&self) -> bool {
        let text = |value: &str, max: usize| {
            !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
        };
        text(&self.handle, 512)
            && text(&self.label, 256)
            && self
                .group
                .as_ref()
                .is_none_or(|group| text(&group.handle, 512) && text(&group.label, 256))
    }
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

/// A native Personal result whose source-specific payload was validated while
/// its exact broker request was still live.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedPersonalAcquisitionResult {
    response: PersonalAcquisitionResult,
    projection: Option<ValidatedPersonalProjection>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ValidatedPersonalProjection {
    People(PeopleView),
    Wellbeing(WellbeingView),
}

impl ValidatedPersonalAcquisitionResult {
    pub fn into_parts(
        self,
    ) -> (
        PersonalAcquisitionResult,
        Option<ValidatedPersonalProjection>,
    ) {
        (self.response, self.projection)
    }
}

fn validate_response_payload(
    response: &PersonalAcquisitionResult,
) -> Result<Option<ValidatedPersonalProjection>, AgentFailure> {
    if response.mode != PersonalAcquisitionMode::ReadProjection
        && (response.view.is_some() || response.transform_operation_id.is_some())
    {
        return Err(AgentFailure::PolicyDenied);
    }
    if response.mode != PersonalAcquisitionMode::InspectCatalog
        && (!response.resources.is_empty() || response.catalog_complete)
    {
        return Err(AgentFailure::InvalidInput);
    }
    if response.mode == PersonalAcquisitionMode::ReadProjection && response.view.is_none() {
        return Err(AgentFailure::InvalidInput);
    }
    match (&response.view, response.domain) {
        (Some(value), PersonalDomain::People) => {
            if response.transform_operation_id.is_some() {
                return Err(AgentFailure::PolicyDenied);
            }
            let view: PeopleView =
                serde_json::from_value(value.clone()).map_err(|_| AgentFailure::InvalidInput)?;
            validate_people_view(&view, chrono::Utc::now().timestamp_millis())?;
            Ok(Some(ValidatedPersonalProjection::People(view)))
        }
        (Some(value), PersonalDomain::Wellbeing) => {
            if response
                .transform_operation_id
                .is_none_or(|operation_id| operation_id.is_nil())
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let view: WellbeingView =
                serde_json::from_value(value.clone()).map_err(|_| AgentFailure::InvalidInput)?;
            validate_wellbeing_view(&view, chrono::Utc::now().timestamp_millis())?;
            Ok(Some(ValidatedPersonalProjection::Wellbeing(view)))
        }
        (None, _) if response.transform_operation_id.is_some() => Err(AgentFailure::PolicyDenied),
        (None, _) => Ok(None),
    }
}

pub struct PersonalExchange;

pub type PersonalBroker = AcquisitionBroker<PersonalExchange>;

impl AcquisitionExchange for PersonalExchange {
    type Request = PersonalAcquisitionRequest;
    type Response = PersonalAcquisitionResult;
    type Accepted = ValidatedPersonalAcquisitionResult;

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

    fn admit(
        request: &Self::Request,
        response: &Self::Response,
    ) -> CompletionOutcome<Self::Accepted> {
        let projection = match validate_response_payload(response) {
            Ok(projection) => projection,
            Err(failure) => return CompletionOutcome::Refuse(failure),
        };
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
        if request.mode == PersonalAcquisitionMode::InspectCatalog {
            if response.resources.len() > 256
                || response
                    .resources
                    .iter()
                    .any(|resource| !resource.valid_metadata())
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
        CompletionOutcome::Accept(ValidatedPersonalAcquisitionResult {
            response: response.clone(),
            projection,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_context_contract::{
        CapacityState, PEOPLE_VIEW_ID, PeopleIdentity, PeopleView, RecoveryState,
        WELLBEING_VIEW_ID, WellbeingView,
    };

    fn request(
        domain: PersonalDomain,
        mode: PersonalAcquisitionMode,
    ) -> PersonalAcquisitionRequest {
        PersonalAcquisitionRequest {
            request_id: Uuid::new_v4(),
            host_epoch: "host-epoch".into(),
            person_id: PersonId::new(),
            device_id: "device".into(),
            domain,
            mode,
            selected_handles: Vec::new(),
            deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
            expected_native_subject_fingerprint: Some("subject".into()),
        }
    }

    fn people_view(now: i64) -> PeopleView {
        PeopleView {
            schema_version: floe_kernel::AGENT_VERSION,
            view_id: PEOPLE_VIEW_ID.into(),
            source_handle: "native-contacts".into(),
            observed_at_unix_ms: now - 1_000,
            expires_at_unix_ms: now + 10_000,
            coverage_complete: true,
            identities: Vec::new(),
        }
    }

    fn wellbeing_view(now: i64) -> WellbeingView {
        WellbeingView {
            schema_version: floe_kernel::AGENT_VERSION,
            view_id: WELLBEING_VIEW_ID.into(),
            source_handle: "native-health".into(),
            observed_at_unix_ms: now - 1_000,
            expires_at_unix_ms: now + 10_000,
            capacity: CapacityState::Typical,
            recovery: RecoveryState::Typical,
            confidence_millis: 500,
            evidence_handles: vec!["health-evidence".into()],
        }
    }

    fn response(request: &PersonalAcquisitionRequest) -> PersonalAcquisitionResult {
        let now = chrono::Utc::now().timestamp_millis();
        let (view, transform_operation_id) = match (request.domain, request.mode) {
            (PersonalDomain::People, PersonalAcquisitionMode::ReadProjection) => {
                (Some(serde_json::to_value(people_view(now)).unwrap()), None)
            }
            (PersonalDomain::Wellbeing, PersonalAcquisitionMode::ReadProjection) => (
                Some(serde_json::to_value(wellbeing_view(now)).unwrap()),
                Some(Uuid::new_v4()),
            ),
            _ => (None, None),
        };
        PersonalAcquisitionResult {
            request_id: request.request_id,
            host_epoch: request.host_epoch.clone(),
            person_id: request.person_id,
            device_id: request.device_id.clone(),
            domain: request.domain,
            mode: request.mode,
            native_subject_fingerprint_before: "subject".into(),
            native_subject_fingerprint_after: "subject".into(),
            permission_class: "authorized".into(),
            provider: "apple".into(),
            view,
            transform_operation_id,
            resources: Vec::new(),
            catalog_complete: false,
        }
    }

    #[test]
    fn people_and_wellbeing_reads_are_admitted_with_typed_views() {
        let people_request = request(
            PersonalDomain::People,
            PersonalAcquisitionMode::ReadProjection,
        );
        let people_response = response(&people_request);
        let CompletionOutcome::Accept(people) =
            PersonalExchange::admit(&people_request, &people_response)
        else {
            panic!("valid People projection must be admitted");
        };
        assert!(matches!(
            people.into_parts().1,
            Some(ValidatedPersonalProjection::People(_))
        ));

        let wellbeing_request = request(
            PersonalDomain::Wellbeing,
            PersonalAcquisitionMode::ReadProjection,
        );
        let wellbeing_response = response(&wellbeing_request);
        let CompletionOutcome::Accept(wellbeing) =
            PersonalExchange::admit(&wellbeing_request, &wellbeing_response)
        else {
            panic!("valid Wellbeing projection must be admitted");
        };
        assert!(matches!(
            wellbeing.into_parts().1,
            Some(ValidatedPersonalProjection::Wellbeing(_))
        ));
    }

    #[test]
    fn wrong_mode_subject_and_epoch_are_rejected() {
        let request = request(
            PersonalDomain::People,
            PersonalAcquisitionMode::ReadProjection,
        );

        let mut wrong_mode = response(&request);
        wrong_mode.mode = PersonalAcquisitionMode::InspectSubject;
        wrong_mode.view = None;
        assert!(matches!(
            PersonalExchange::admit(&request, &wrong_mode),
            CompletionOutcome::Reject(AgentFailure::AccessReviewRequired)
        ));

        let mut wrong_subject = response(&request);
        wrong_subject.native_subject_fingerprint_before = "other-subject".into();
        wrong_subject.native_subject_fingerprint_after = "other-subject".into();
        assert!(matches!(
            PersonalExchange::admit(&request, &wrong_subject),
            CompletionOutcome::Reject(AgentFailure::AccessReviewRequired)
        ));

        let mut wrong_epoch = response(&request);
        wrong_epoch.host_epoch = "replaced-host".into();
        assert!(matches!(
            PersonalExchange::admit(&request, &wrong_epoch),
            CompletionOutcome::Reject(AgentFailure::AccessReviewRequired)
        ));
    }

    #[test]
    fn missing_health_transform_operation_is_denied() {
        let request = request(
            PersonalDomain::Wellbeing,
            PersonalAcquisitionMode::ReadProjection,
        );
        let mut response = response(&request);
        response.transform_operation_id = None;
        assert!(matches!(
            PersonalExchange::admit(&request, &response),
            CompletionOutcome::Refuse(AgentFailure::PolicyDenied)
        ));
    }

    #[test]
    fn oversized_people_view_is_refused_before_becoming_an_accepted_result() {
        let request = request(
            PersonalDomain::People,
            PersonalAcquisitionMode::ReadProjection,
        );
        let now = chrono::Utc::now().timestamp_millis();
        let mut view = people_view(now);
        view.identities = (0..64)
            .map(|index| PeopleIdentity {
                identity_handle: format!("identity-{index}"),
                display_name: "n".repeat(256),
                aliases: vec!["a".repeat(256); 8],
                confidence_millis: 500,
                evidence_handles: vec!["e".repeat(128); 16],
            })
            .collect();
        let mut response = response(&request);
        response.view = Some(serde_json::to_value(view).unwrap());
        assert!(matches!(
            PersonalExchange::admit(&request, &response),
            CompletionOutcome::Refuse(AgentFailure::BudgetExceeded)
        ));
    }

    #[test]
    fn stale_wellbeing_view_is_refused_with_the_contract_error() {
        let request = request(
            PersonalDomain::Wellbeing,
            PersonalAcquisitionMode::ReadProjection,
        );
        let now = chrono::Utc::now().timestamp_millis();
        let mut view = wellbeing_view(now);
        view.observed_at_unix_ms = now - 2_000_000;
        view.expires_at_unix_ms = now - 1;
        let mut response = response(&request);
        response.view = Some(serde_json::to_value(view).unwrap());
        assert!(matches!(
            PersonalExchange::admit(&request, &response),
            CompletionOutcome::Refuse(AgentFailure::InvalidInput)
        ));
    }

    #[test]
    fn catalog_shape_rules_are_enforced_by_personal_owner() {
        let request = request(
            PersonalDomain::People,
            PersonalAcquisitionMode::InspectCatalog,
        );
        let mut response = response(&request);
        response.resources = vec![
            NativeSourceResource {
                handle: "same".into(),
                label: "first".into(),
                group: None,
            },
            NativeSourceResource {
                handle: "same".into(),
                label: "second".into(),
                group: None,
            },
        ];
        assert!(matches!(
            PersonalExchange::admit(&request, &response),
            CompletionOutcome::Reject(AgentFailure::PolicyDenied)
        ));
    }
}
