use super::AgentVaultFailureDto;
use super::connections::{
    validate_envelope, validate_identifier, validate_producer, validate_text, validate_uuid,
};
use floe_context_contract::{ConsumerPolicyAuthority, GrantAuthority, GrantId, SourceAuthority};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteAccessRequestDto {
    pub schema_version: u32,
    pub request_id: Uuid,
    pub operation: RemoteAccessOperationDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RemoteAccessOperationDto {
    InspectProducer {},
    ReviewAndEnroll {
        producer: RemoteProducerIdentityDto,
    },
    EnrollmentStatus {
        enrollment_id: String,
    },
    ConnectionObserve {
        connector_id: String,
        connection_id: String,
        resource: Option<String>,
        enabled: Option<bool>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        disconnecting: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected: Option<ConnectionObserveExpectationDto>,
    },
    ConnectionObserveReview {
        connector_id: String,
        connection_id: String,
        resource: Option<String>,
    },
    ReadResult {
        operation_id: Uuid,
        release: bool,
    },
}

/// One reviewed grant of a remote Observe bundle, echoed back on enable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionObserveMemberDto {
    pub view_id: String,
    pub resource: String,
    pub producer_fingerprint: String,
    pub source_authority: SourceAuthority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_revision: Option<u64>,
    pub provider_identity: String,
    pub recipient: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_grant_id: Option<GrantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_grant_authority: Option<GrantAuthority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_policy: Option<ConsumerPolicyAuthority>,
}

/// The whole reviewed bundle a remote Observe enable binds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionObserveExpectationDto {
    pub members: Vec<ConnectionObserveMemberDto>,
}

impl RemoteAccessRequestDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_envelope(self.schema_version, self.request_id)?;
        match &self.operation {
            RemoteAccessOperationDto::InspectProducer {} => Ok(()),
            RemoteAccessOperationDto::ReviewAndEnroll { producer } => validate_producer(producer),
            RemoteAccessOperationDto::EnrollmentStatus { enrollment_id } => {
                validate_uuid(enrollment_id)
            }
            RemoteAccessOperationDto::ConnectionObserve {
                connector_id,
                connection_id,
                resource,
                expected,
                ..
            } => {
                validate_observe_identity(connector_id, connection_id, resource)?;
                if let Some(expected) = expected {
                    validate_observe_expectation(expected)?;
                }
                Ok(())
            }
            RemoteAccessOperationDto::ConnectionObserveReview {
                connector_id,
                connection_id,
                resource,
            } => validate_observe_identity(connector_id, connection_id, resource),
            RemoteAccessOperationDto::ReadResult { operation_id, .. } => {
                validate_identifier(*operation_id)
            }
        }
    }
}

fn validate_observe_identity(
    connector_id: &str,
    connection_id: &str,
    resource: &Option<String>,
) -> Result<(), &'static str> {
    validate_text(connector_id, 128)?;
    validate_uuid(connection_id)?;
    if let Some(resource) = resource {
        validate_text(resource, 2048)?;
    }
    Ok(())
}

fn validate_observe_expectation(
    expected: &ConnectionObserveExpectationDto,
) -> Result<(), &'static str> {
    if expected.members.is_empty() || expected.members.len() > 8 {
        return Err("operation.expected.members");
    }
    let mut previous: Option<&str> = None;
    for member in &expected.members {
        for value in [
            member.view_id.as_str(),
            member.resource.as_str(),
            member.producer_fingerprint.as_str(),
            member.provider_identity.as_str(),
            member.recipient.as_str(),
        ] {
            validate_text(value, 256)?;
        }
        if !member.source_authority.is_valid() || member.connection_revision == Some(0) {
            return Err("operation.expected.member");
        }
        match (
            &member.expected_grant_id,
            &member.expected_grant_authority,
            &member.expected_policy,
        ) {
            (None, None, None) => {}
            (Some(id), Some(authority), Some(policy))
                if id.is_valid() && authority.is_valid() && policy.is_valid() => {}
            _ => return Err("operation.expected.member.grant"),
        }
        if previous.is_some_and(|previous| previous >= member.view_id.as_str()) {
            return Err("operation.expected.members");
        }
        previous = Some(member.view_id.as_str());
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteAccessResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub producer: Option<RemoteProducerIdentityDto>,
    pub owner: Option<RemoteOwnerPublicKeyDto>,
    pub enrollment: Option<RemoteAuthorityEnrollmentStatusDto>,
    pub connection_observe_status: Option<String>,
    pub reviewed_bundle: Option<ConnectionObserveExpectationDto>,
    pub failure: Option<AgentVaultFailureDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteProducerIdentityDto {
    pub schema_version: u32,
    pub instance_id: String,
    pub execution_owner: String,
    pub audience: String,
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteAuthorityEnrollmentStatusDto {
    pub enrollment_id: String,
    pub key_id: String,
    pub fingerprint: String,
    pub local_confirmed: bool,
    pub admin_approved: bool,
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteOwnerPublicKeyDto {
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}
