use std::collections::BTreeMap;

use thiserror::Error;

use crate::api::{
    DataRecipient, ExecutionLocation, ModelProfile, PlannedRoute, RecipientConstraint, RouteRequest,
};

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RoutePlanError {
    #[error("no model profile is configured")]
    NotConfigured,
    #[error("external transfer requires consent")]
    ConsentRequired,
    #[error("the requested model is unavailable")]
    Unavailable,
    #[error("the requested inference route is denied")]
    Denied,
}

#[derive(Clone, Debug, Default)]
pub struct InferenceRouter {
    profiles: BTreeMap<String, ModelProfile>,
}

impl InferenceRouter {
    pub fn new(profiles: impl IntoIterator<Item = ModelProfile>) -> Result<Self, RoutePlanError> {
        let mut indexed = BTreeMap::new();
        for profile in profiles {
            if !valid_identifier(&profile.id)
                || !valid_identifier(profile.purpose.as_str())
                || !valid_identifier(profile.consumer.as_str())
                || !is_valid_recipient(&profile.data_recipient)
                || profile
                    .capabilities
                    .0
                    .iter()
                    .any(|value| !valid_identifier(value))
            {
                return Err(RoutePlanError::Denied);
            }
            if indexed.insert(profile.id.clone(), profile).is_some() {
                return Err(RoutePlanError::Denied);
            }
        }
        Ok(Self { profiles: indexed })
    }

    pub fn plan(&self, request: &RouteRequest) -> Result<PlannedRoute, RoutePlanError> {
        if !valid_identifier(request.purpose.as_str())
            || !valid_identifier(request.consumer.as_str())
            || request
                .requested_capabilities
                .0
                .iter()
                .any(|value| !valid_identifier(value))
        {
            return Err(RoutePlanError::Denied);
        }
        let profile = if let Some(id) = request.preferred_profile_id.as_deref() {
            self.profiles.get(id).ok_or(RoutePlanError::NotConfigured)?
        } else {
            let mut matching = self.profiles.values().filter(|profile| {
                profile.purpose == request.purpose && profile.consumer == request.consumer
            });
            let selected = matching.next().ok_or(RoutePlanError::NotConfigured)?;
            if matching.next().is_some() {
                return Err(RoutePlanError::Denied);
            }
            selected
        };

        if !profile.available {
            return Err(RoutePlanError::Unavailable);
        }
        if profile.purpose != request.purpose || profile.consumer != request.consumer {
            return Err(RoutePlanError::Denied);
        }
        if !is_valid_recipient(&profile.data_recipient)
            || (profile.execution_location == ExecutionLocation::Remote
                && profile.data_recipient == DataRecipient::Device)
        {
            return Err(RoutePlanError::Denied);
        }
        if request
            .requested_capabilities
            .0
            .iter()
            .any(|capability| !profile.capabilities.contains(capability))
        {
            return Err(RoutePlanError::Unavailable);
        }

        let recipient = match &request.recipient {
            RecipientConstraint::DeviceOnly if profile.data_recipient != DataRecipient::Device => {
                return Err(RoutePlanError::Denied);
            }
            RecipientConstraint::DeviceOnly => DataRecipient::Device,
            RecipientConstraint::External { recipient, consent } => {
                if recipient.trim().is_empty() || recipient.len() > 256 {
                    return Err(RoutePlanError::Denied);
                }
                if !*consent {
                    return Err(RoutePlanError::ConsentRequired);
                }
                match &profile.data_recipient {
                    DataRecipient::External(actual) if actual == recipient => {
                        DataRecipient::External(actual.clone())
                    }
                    DataRecipient::External(_) => return Err(RoutePlanError::Denied),
                    DataRecipient::Device => return Err(RoutePlanError::Denied),
                }
            }
        };

        if profile.data_recipient.is_external() && !profile.external_transfer_consent {
            return Err(RoutePlanError::ConsentRequired);
        }

        Ok(PlannedRoute {
            profile_id: profile.id.clone(),
            purpose: profile.purpose.clone(),
            consumer: profile.consumer.clone(),
            execution_location: profile.execution_location,
            data_recipient: recipient,
        })
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn is_valid_recipient(recipient: &DataRecipient) -> bool {
    match recipient {
        DataRecipient::Device => true,
        DataRecipient::External(value) => {
            !value.is_empty()
                && value.trim() == value
                && value.len() <= 256
                && !value.chars().any(char::is_control)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{ExecutionLocation, ModelCapabilities, ModelConsumer, ModelPurpose};

    fn profile(recipient: DataRecipient, consent: bool) -> ModelProfile {
        ModelProfile {
            id: "profile".into(),
            purpose: ModelPurpose::new("everyday_assistance").unwrap(),
            consumer: ModelConsumer::new("conversation.root").unwrap(),
            execution_location: ExecutionLocation::Gateway,
            data_recipient: recipient,
            capabilities: ModelCapabilities(vec!["chat".into()]),
            available: true,
            external_transfer_consent: consent,
        }
    }

    fn request(recipient: RecipientConstraint) -> RouteRequest {
        RouteRequest {
            purpose: ModelPurpose::new("everyday_assistance").unwrap(),
            requested_capabilities: ModelCapabilities(vec!["chat".into()]),
            consumer: ModelConsumer::new("conversation.root").unwrap(),
            recipient,
            preferred_profile_id: None,
        }
    }

    #[test]
    fn recipient_and_execution_are_independent() {
        let router = InferenceRouter::new([profile(DataRecipient::Device, false)]).unwrap();
        let route = router
            .plan(&request(RecipientConstraint::DeviceOnly))
            .unwrap();
        assert_eq!(route.execution_location, ExecutionLocation::Gateway);
        assert_eq!(route.data_recipient, DataRecipient::Device);
    }

    #[test]
    fn external_recipient_requires_exact_consent_and_identity() {
        let router = InferenceRouter::new([profile(
            DataRecipient::external("fixture-recipient").unwrap(),
            true,
        )])
        .unwrap();
        assert_eq!(
            router.plan(&request(RecipientConstraint::External {
                recipient: "other-recipient".into(),
                consent: true,
            })),
            Err(RoutePlanError::Denied)
        );
        assert_eq!(
            router.plan(&request(RecipientConstraint::External {
                recipient: "fixture-recipient".into(),
                consent: false,
            })),
            Err(RoutePlanError::ConsentRequired)
        );
    }

    #[test]
    fn explicit_missing_profile_does_not_fall_back_and_duplicate_ids_are_rejected() {
        assert_eq!(
            InferenceRouter::new([
                profile(DataRecipient::Device, false),
                profile(DataRecipient::Device, false),
            ])
            .unwrap_err(),
            RoutePlanError::Denied
        );
        let router = InferenceRouter::new([profile(DataRecipient::Device, false)]).unwrap();
        let mut request = request(RecipientConstraint::DeviceOnly);
        request.preferred_profile_id = Some("missing".into());
        assert_eq!(router.plan(&request), Err(RoutePlanError::NotConfigured));
    }

    #[test]
    fn ambiguous_or_deserialized_invalid_profiles_are_denied() {
        let first = profile(DataRecipient::Device, false);
        let mut second = first.clone();
        second.id = "second".into();
        let router = InferenceRouter::new([first.clone(), second]).unwrap();
        assert_eq!(
            router.plan(&request(RecipientConstraint::DeviceOnly)),
            Err(RoutePlanError::Denied)
        );
        let mut invalid = first;
        invalid.purpose = serde_json::from_str("\"\"").unwrap();
        assert_eq!(
            InferenceRouter::new([invalid]).unwrap_err(),
            RoutePlanError::Denied
        );
    }

    #[test]
    fn scope_capability_and_remote_device_claims_are_checked() {
        let base = profile(DataRecipient::Device, false);
        let router = InferenceRouter::new([base.clone()]).unwrap();
        let mut query = request(RecipientConstraint::DeviceOnly);
        query.preferred_profile_id = Some(base.id.clone());
        query.consumer = ModelConsumer::new("other-consumer").unwrap();
        assert_eq!(router.plan(&query), Err(RoutePlanError::Denied));
        query.consumer = base.consumer.clone();
        query.requested_capabilities.0.push("unsupported".into());
        assert_eq!(router.plan(&query), Err(RoutePlanError::Unavailable));
        let mut remote = base;
        remote.execution_location = ExecutionLocation::Remote;
        let router = InferenceRouter::new([remote]).unwrap();
        assert_eq!(
            router.plan(&request(RecipientConstraint::DeviceOnly)),
            Err(RoutePlanError::Denied)
        );
    }
}
