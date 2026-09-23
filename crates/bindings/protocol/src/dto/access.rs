use super::AgentVaultFailureDto;
use super::connections::{
    validate_envelope, validate_identifier, validate_producer, validate_text, validate_uuid,
};
use floe_context_contract::{
    ConsumerPolicyAuthority, GrantAuthority, GrantId, SourceAuthority,
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
    CalendarGrantPreview {
        connector_id: String,
        connection_id: String,
        resource: String,
    },
    CalendarGrantReview {
        connector_id: String,
        connection_id: String,
        resource: String,
        expected_producer_fingerprint: String,
        expected_source_authority: SourceAuthority,
        expected_grant_id: Option<GrantId>,
        expected_grant_authority: Option<GrantAuthority>,
        expected_consumer_policy: Option<ConsumerPolicyAuthority>,
    },
    CalendarGrantStatus {
        grant_id: GrantId,
    },
    CalendarGrantPause {
        grant_id: GrantId,
        expected_authority: GrantAuthority,
    },
    ViewGrantPreview {
        view_id: String,
        connector_id: String,
        connection_id: String,
        resource: String,
        consumer: String,
    },
    ViewGrantReview {
        view_id: String,
        connector_id: String,
        connection_id: String,
        resource: String,
        consumer: String,
        expected_producer_fingerprint: String,
        expected_source_authority: SourceAuthority,
        expected_connection_revision: u64,
        expected_provider_identity: String,
        expected_recipient: String,
    },
    ViewGrantStatus {
        grant_id: GrantId,
    },
    ViewGrantPause {
        grant_id: GrantId,
        expected_authority: GrantAuthority,
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
            RemoteAccessOperationDto::CalendarGrantPreview {
                connector_id,
                connection_id,
                resource,
            } => validate_source(connector_id, connection_id, resource),
            RemoteAccessOperationDto::CalendarGrantReview {
                connector_id,
                connection_id,
                resource,
                expected_producer_fingerprint,
                expected_source_authority,
                expected_grant_id,
                expected_grant_authority,
                expected_consumer_policy,
            } => {
                validate_source(connector_id, connection_id, resource)?;
                validate_text(expected_producer_fingerprint, 64)?;
                if !expected_source_authority.is_valid() {
                    return Err("operation.expected_source_authority");
                }
                match (
                    expected_grant_id,
                    expected_grant_authority,
                    expected_consumer_policy,
                ) {
                    (None, None, None) => Ok(()),
                    (Some(id), Some(authority), Some(policy))
                        if id.is_valid()
                            && authority.is_valid()
                            && policy.is_valid() => Ok(()),
                    _ => Err("operation.expected_grant"),
                }
            }
            RemoteAccessOperationDto::ViewGrantPreview {
                view_id,
                connector_id,
                connection_id,
                resource,
                consumer,
            } => {
                validate_source(connector_id, connection_id, resource)?;
                validate_text(view_id, 128)?;
                validate_text(consumer, 128)
            }
            RemoteAccessOperationDto::ViewGrantReview {
                view_id,
                connector_id,
                connection_id,
                resource,
                consumer,
                expected_producer_fingerprint,
                expected_connection_revision,
                expected_provider_identity,
                expected_recipient,
                expected_source_authority,
            } => {
                validate_source(connector_id, connection_id, resource)?;
                validate_text(view_id, 128)?;
                validate_text(consumer, 128)?;
                validate_text(expected_producer_fingerprint, 64)?;
                validate_text(expected_provider_identity, 1024)?;
                validate_text(expected_recipient, 1024)?;
                if !expected_source_authority.is_valid() {
                    return Err("operation.expected_source_authority");
                }
                if *expected_connection_revision == 0 {
                    return Err("operation.expected_connection_revision");
                }
                Ok(())
            }
            RemoteAccessOperationDto::CalendarGrantStatus { grant_id }
            | RemoteAccessOperationDto::ViewGrantStatus { grant_id } => {
                validate_identifier(grant_id.as_uuid())
            }
            RemoteAccessOperationDto::CalendarGrantPause {
                grant_id,
                expected_authority,
            }
            | RemoteAccessOperationDto::ViewGrantPause {
                grant_id,
                expected_authority,
            } => {
                validate_identifier(grant_id.as_uuid())?;
                if !expected_authority.is_valid() {
                    return Err("operation.expected_authority");
                }
                Ok(())
            }
            RemoteAccessOperationDto::ReadResult { operation_id, .. } => {
                validate_identifier(*operation_id)
            }
        }
    }
}

fn validate_source(connector: &str, connection: &str, resource: &str) -> Result<(), &'static str> {
    validate_text(connector, 128)?;
    validate_uuid(connection)?;
    validate_text(resource, 2048)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteAccessResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub producer: Option<RemoteProducerIdentityDto>,
    pub owner: Option<RemoteOwnerPublicKeyDto>,
    pub enrollment: Option<RemoteAuthorityEnrollmentStatusDto>,
    pub calendar_grant: Option<RemoteCalendarGrantOverviewDto>,
    pub calendar_preview: Option<RemoteCalendarGrantPreviewDto>,
    pub view_grant: Option<RemoteViewGrantOverviewDto>,
    pub view_preview: Option<RemoteViewGrantPreviewDto>,
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
