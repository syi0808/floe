use super::AgentVaultFailureDto;
use super::connections::{
    validate_envelope, validate_identifier, validate_producer, validate_uuid,
};
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
