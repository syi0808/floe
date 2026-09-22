use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionsResultDto {
    pub operation_id: uuid::Uuid,
    pub done: bool,
    pub state: Option<super::AgentVaultStateDto>,
    pub connections: Option<Vec<super::ConnectorSnapshotDto>>,
    pub failure: Option<super::AgentVaultFailureDto>,
}
use uuid::Uuid;

use super::{
    APP_WIRE_VERSION, AgentVaultFailureDto, RemoteOwnerPublicKeyDto, RemoteProducerIdentityDto,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairingTargetDto {
    pub base_url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemotePairingRequestDto {
    pub schema_version: u32,
    pub request_id: Uuid,
    pub operation: RemotePairingOperationDto,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RemotePairingOperationDto {
    Prepare {},
    Confirm {
        target: PairingTargetDto,
        challenge: RemotePairingChallengeDto,
        polling_proof: String,
    },
    Status {
        target: PairingTargetDto,
        pairing_id: Uuid,
        polling_proof: String,
    },
    Finalize {
        target: PairingTargetDto,
        pairing_id: Uuid,
        polling_proof: String,
        challenge: RemotePairingChallengeDto,
    },
    ReadResult {
        operation_id: Uuid,
        release: bool,
    },
}

impl std::fmt::Debug for RemotePairingOperationDto {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RemotePairingOperationDto { evidence: [REDACTED] }")
    }
}

impl RemotePairingRequestDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_envelope(self.schema_version, self.request_id)?;
        match &self.operation {
            RemotePairingOperationDto::Prepare {} => Ok(()),
            RemotePairingOperationDto::Confirm {
                target,
                challenge,
                polling_proof,
            } => {
                validate_setup(target, polling_proof)?;
                validate_challenge(challenge)
            }
            RemotePairingOperationDto::Status {
                target,
                pairing_id,
                polling_proof,
            } => {
                validate_identifier(*pairing_id)?;
                validate_setup(target, polling_proof)
            }
            RemotePairingOperationDto::Finalize {
                target,
                pairing_id,
                polling_proof,
                challenge,
            } => {
                validate_identifier(*pairing_id)?;
                validate_setup(target, polling_proof)?;
                validate_challenge(challenge)
            }
            RemotePairingOperationDto::ReadResult { operation_id, .. } => {
                validate_identifier(*operation_id)
            }
        }
    }
}

pub(super) fn validate_envelope(version: u32, request_id: Uuid) -> Result<(), &'static str> {
    if version != APP_WIRE_VERSION {
        return Err("schema_version");
    }
    if request_id.is_nil() {
        return Err("request_id");
    }
    Ok(())
}

pub(super) fn validate_identifier(identifier: Uuid) -> Result<(), &'static str> {
    if identifier.is_nil() {
        Err("operation.id")
    } else {
        Ok(())
    }
}

pub(super) fn validate_text(value: &str, maximum: usize) -> Result<(), &'static str> {
    if value.is_empty()
        || value.len() > maximum
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err("operation.text")
    } else {
        Ok(())
    }
}

pub(super) fn validate_uuid(value: &str) -> Result<(), &'static str> {
    let identifier = Uuid::parse_str(value).map_err(|_| "operation.id")?;
    validate_identifier(identifier)
}

fn validate_setup(target: &PairingTargetDto, proof: &str) -> Result<(), &'static str> {
    validate_text(&target.base_url, 2048)?;
    validate_text(proof, 256)
}

fn validate_challenge(challenge: &RemotePairingChallengeDto) -> Result<(), &'static str> {
    if challenge.schema_version != 1 {
        return Err("operation.challenge.schema_version");
    }
    validate_uuid(&challenge.pairing_id)?;
    validate_uuid(&challenge.challenge_id)?;
    validate_uuid(&challenge.issuer.key_id)?;
    validate_text(&challenge.issuer.public_key, 128)?;
    validate_text(&challenge.issuer.fingerprint, 64)?;
    validate_text(&challenge.challenge_b64url, 16_384)?;
    validate_text(&challenge.producer_signature, 128)?;
    validate_producer(&challenge.producer)
}

pub(super) fn validate_producer(producer: &RemoteProducerIdentityDto) -> Result<(), &'static str> {
    if producer.schema_version != 1 {
        return Err("operation.producer.schema_version");
    }
    validate_uuid(&producer.instance_id)?;
    validate_uuid(&producer.execution_owner)?;
    validate_uuid(&producer.key_id)?;
    validate_text(&producer.audience, 256)?;
    validate_text(&producer.public_key, 128)?;
    validate_text(&producer.fingerprint, 64)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemotePairingResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub owner: Option<RemoteOwnerPublicKeyDto>,
    pub pairing: Option<PairingReportDto>,
    pub failure: Option<AgentVaultFailureDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairingReportDto {
    pub pairing_id: String,
    pub person_id: String,
    pub device_id: String,
    pub producer: Option<RemoteProducerIdentityDto>,
    pub issuer: Option<RemoteOwnerPublicKeyDto>,
    pub issuer_fingerprint: Option<String>,
    pub outcome: PairingOutcomeDto,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum PairingOutcomeDto {
    Pending {},
    LocalConfirmed {},
    Approved { client_id: String, token: String },
    Rejected {},
    Expired {},
    RepairRequired {},
}

impl std::fmt::Debug for PairingOutcomeDto {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Pending {} => "Pending",
            Self::LocalConfirmed {} => "LocalConfirmed",
            Self::Approved { .. } => "Approved { token: [REDACTED], .. }",
            Self::Rejected {} => "Rejected",
            Self::Expired {} => "Expired",
            Self::RepairRequired {} => "RepairRequired",
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemotePairingChallengeDto {
    pub schema_version: u32,
    pub pairing_id: String,
    pub challenge_id: String,
    pub challenge_b64url: String,
    pub producer_signature: String,
    pub producer: RemoteProducerIdentityDto,
    pub issuer: RemoteOwnerPublicKeyDto,
    pub expires_at_unix_ms: i64,
}
