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
    },
    ReadResult {
        operation_id: Uuid,
        release: bool,
    },
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
                ..
            } => {
                validate_text(connector_id, 128)?;
                validate_uuid(connection_id)?;
                if let Some(resource) = resource {
                    validate_text(resource, 2048)?;
                }
                Ok(())
            }
            RemoteAccessOperationDto::ReadResult { operation_id, .. } => {
                validate_identifier(*operation_id)
            }
        }
    }
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
pub struct RemoteCalendarGrantPreviewDto {
    pub schema_version: u32,
    pub person_id: String,
    pub connector_id: String,
    pub connection_id: String,
    pub resource: String,
    pub source_authority: SourceAuthority,
    pub provider_identity: String,
    pub execution_owner: String,
    pub producer: RemoteProducerIdentityDto,
    pub consumers: Vec<String>,
    pub purpose: String,
    pub recipient: String,
    pub grant_id: Option<GrantId>,
    pub grant_authority: Option<GrantAuthority>,
    pub consumer_policy: Option<ConsumerPolicyAuthority>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteCalendarGrantOverviewDto {
    pub schema_version: u32,
    pub person_id: String,
    pub grant_id: GrantId,
    pub grant_authority: GrantAuthority,
    pub connector_id: String,
    pub connection_id: String,
    pub resource: String,
    pub source_authority: SourceAuthority,
    pub execution_owner: String,
    pub state: String,
    pub review_required: bool,
    pub consumers: Vec<String>,
    pub purpose: String,
    pub recipient: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteViewGrantPreviewDto {
    pub schema_version: u32,
    pub person_id: String,
    pub view_id: String,
    pub connector_id: String,
    pub connection_id: String,
    pub connection_revision: u64,
    pub resource: String,
    pub source_authority: SourceAuthority,
    pub provider_identity: String,
    pub execution_owner: String,
    pub producer: RemoteProducerIdentityDto,
    pub consumer: String,
    pub purpose: String,
    pub recipient: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteViewGrantOverviewDto {
    pub schema_version: u32,
    pub person_id: String,
    pub grant_id: GrantId,
    pub grant_authority: GrantAuthority,
    pub view_id: String,
    pub connector_id: String,
    pub connection_id: String,
    pub connection_revision: Option<u64>,
    pub resource: String,
    pub source_authority: SourceAuthority,
    pub execution_owner: String,
    pub state: String,
    pub review_required: bool,
    pub consumer: String,
    pub purpose: String,
    pub recipient: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteOwnerPublicKeyDto {
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}
