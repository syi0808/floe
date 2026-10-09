//! Lossless, owner-typed evidence for one retained Session message.
//!
//! This contract is separate from the neutral Core transcript. Core receives
//! only the content digest through `MessageEvidenceReference`; it never needs
//! to know the owner payload or this codec.

use floe_agent_contract::{AgentFailure, MAX_OUTPUT_BYTES};
use floe_kernel::PersonId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::session::{AgentMessage, MAX_SESSION_BYTES};

pub const TYPED_AGENT_MESSAGE_OWNER_NAMESPACE: &str = "floe.conversation.session";
pub const TYPED_AGENT_MESSAGE_SCHEMA_ID: &str = "floe.conversation.agent-message";
pub const TYPED_AGENT_MESSAGE_SCHEMA_VERSION: u32 = 2;

/// Absolute ceiling on serialized AgentMessage payload bytes. The admissible
/// payload is smaller when the measured serialized reference consumes part of
/// the shared per-record limit.
pub const MAX_TYPED_AGENT_MESSAGE_PAYLOAD_BYTES: usize = MAX_SESSION_BYTES;
/// Per-record limit for payload bytes plus exact serialized reference bytes.
pub const MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES: usize = MAX_SESSION_BYTES;

const TYPED_AGENT_MESSAGE_DIGEST_DOMAIN: &[u8] =
    b"floe-conversation-typed-agent-message-envelope-v2\0";

/// Provenance class for an owner-authored Manager contribution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TypedAgentMessageProvenance {
    OwnerRecorded,
}

impl TypedAgentMessageProvenance {
    pub fn as_storage_value(self) -> &'static str {
        match self {
            Self::OwnerRecorded => "owner_recorded",
        }
    }

    pub fn from_storage_value(value: &str) -> Result<Self, AgentFailure> {
        match value {
            "owner_recorded" => Ok(Self::OwnerRecorded),
            _ => Err(AgentFailure::UnsupportedVersion),
        }
    }
}

/// Neutral, scoped link to one exact owner-encoded AgentMessage.
///
/// The digest commits to these fields and the full payload. It is safe to pass
/// to role-neutral Core as an opaque content-addressed evidence digest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TypedAgentMessageReference {
    owner_namespace: String,
    schema_id: String,
    schema_version: u32,
    person_id: PersonId,
    session_id: Uuid,
    entry_id: Uuid,
    turn_id: Uuid,
    provenance: TypedAgentMessageProvenance,
    encoded_byte_length: u64,
    digest: [u8; 32],
}

impl TypedAgentMessageReference {
    pub fn owner_namespace(&self) -> &str {
        &self.owner_namespace
    }

    pub fn schema_id(&self) -> &str {
        &self.schema_id
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn person_id(&self) -> PersonId {
        self.person_id
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub fn entry_id(&self) -> Uuid {
        self.entry_id
    }

    pub fn turn_id(&self) -> Uuid {
        self.turn_id
    }

    pub fn provenance(&self) -> TypedAgentMessageProvenance {
        self.provenance
    }

    /// Exact UTF-8 byte length of the owner codec's serialized AgentMessage.
    pub fn encoded_byte_length(&self) -> u64 {
        self.encoded_byte_length
    }

    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }

    pub fn validate_for_storage(&self) -> Result<(), AgentFailure> {
        if self.owner_namespace != TYPED_AGENT_MESSAGE_OWNER_NAMESPACE {
            return Err(AgentFailure::PolicyDenied);
        }
        if self.schema_id != TYPED_AGENT_MESSAGE_SCHEMA_ID
            || self.schema_version != TYPED_AGENT_MESSAGE_SCHEMA_VERSION
        {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if !self.person_id.is_valid()
            || self.session_id.is_nil()
            || self.entry_id.is_nil()
            || self.turn_id.is_nil()
            || self.encoded_byte_length == 0
            || self.digest == [0; 32]
        {
            return Err(AgentFailure::InvalidInput);
        }
        let payload_bytes =
            usize::try_from(self.encoded_byte_length).map_err(|_| AgentFailure::BudgetExceeded)?;
        if payload_bytes > MAX_TYPED_AGENT_MESSAGE_PAYLOAD_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(())
    }

    pub fn encoded_reference_bytes(&self) -> Result<usize, AgentFailure> {
        serde_json::to_vec(self)
            .map(|bytes| bytes.len())
            .map_err(|_| AgentFailure::InvalidInput)
    }

    pub fn from_stored_fields(
        owner_namespace: String,
        schema_id: String,
        schema_version: u32,
        person_id: PersonId,
        session_id: Uuid,
        entry_id: Uuid,
        turn_id: Uuid,
        provenance: TypedAgentMessageProvenance,
        encoded_byte_length: u64,
        digest: [u8; 32],
    ) -> Self {
        Self {
            owner_namespace,
            schema_id,
            schema_version,
            person_id,
            session_id,
            entry_id,
            turn_id,
            provenance,
            encoded_byte_length,
            digest,
        }
    }
}

/// Owner codec output: the exact typed payload bytes and their neutral link.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedAgentMessageEvidence {
    reference: TypedAgentMessageReference,
    payload: Vec<u8>,
}

impl TypedAgentMessageEvidence {
    pub fn encode(
        person_id: PersonId,
        session_id: Uuid,
        entry_id: Uuid,
        provenance: TypedAgentMessageProvenance,
        message: &AgentMessage,
    ) -> Result<Self, AgentFailure> {
        if !person_id.is_valid() || session_id.is_nil() || entry_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        validate_agent_message(message)?;
        let payload = serde_json::to_vec(message).map_err(|_| AgentFailure::InvalidInput)?;
        if payload.is_empty() || payload.len() > MAX_TYPED_AGENT_MESSAGE_PAYLOAD_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }

        let mut reference = TypedAgentMessageReference {
            owner_namespace: TYPED_AGENT_MESSAGE_OWNER_NAMESPACE.to_owned(),
            schema_id: TYPED_AGENT_MESSAGE_SCHEMA_ID.to_owned(),
            schema_version: TYPED_AGENT_MESSAGE_SCHEMA_VERSION,
            person_id,
            session_id,
            entry_id,
            turn_id: message.turn_id(),
            provenance,
            encoded_byte_length: u64::try_from(payload.len())
                .map_err(|_| AgentFailure::BudgetExceeded)?,
            digest: [0; 32],
        };
        reference.digest = evidence_digest(&reference, &payload)?;
        reference.validate_for_storage()?;
        let evidence = Self { reference, payload };
        if evidence.encoded_byte_length()? > MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(evidence)
    }

    pub fn from_stored(
        reference: TypedAgentMessageReference,
        payload: Vec<u8>,
    ) -> Result<Self, AgentFailure> {
        reference.validate_for_storage()?;
        if payload.is_empty()
            || u64::try_from(payload.len()).ok() != Some(reference.encoded_byte_length)
            || payload.len() > MAX_TYPED_AGENT_MESSAGE_PAYLOAD_BYTES
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        if evidence_digest(&reference, &payload)? != reference.digest {
            return Err(AgentFailure::VaultUnavailable);
        }
        let message: AgentMessage =
            serde_json::from_slice(&payload).map_err(|_| AgentFailure::VaultUnavailable)?;
        validate_agent_message(&message).map_err(|_| AgentFailure::VaultUnavailable)?;
        if message.turn_id() != reference.turn_id
            || serde_json::to_vec(&message).map_err(|_| AgentFailure::VaultUnavailable)? != payload
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let evidence = Self { reference, payload };
        if evidence.encoded_byte_length()? > MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(evidence)
    }

    pub fn reference(&self) -> &TypedAgentMessageReference {
        &self.reference
    }

    pub fn payload_bytes(&self) -> &[u8] {
        &self.payload
    }

    pub fn encoded_byte_length(&self) -> Result<usize, AgentFailure> {
        self.payload
            .len()
            .checked_add(self.reference.encoded_reference_bytes()?)
            .ok_or(AgentFailure::BudgetExceeded)
    }

    pub fn decode_message(&self) -> Result<AgentMessage, AgentFailure> {
        let checked = Self::from_stored(self.reference.clone(), self.payload.clone())?;
        serde_json::from_slice(&checked.payload).map_err(|_| AgentFailure::VaultUnavailable)
    }
}

fn evidence_digest(
    reference: &TypedAgentMessageReference,
    payload: &[u8],
) -> Result<[u8; 32], AgentFailure> {
    let payload_length = u64::try_from(payload.len()).map_err(|_| AgentFailure::BudgetExceeded)?;
    let mut digest = Sha256::new();
    digest.update(TYPED_AGENT_MESSAGE_DIGEST_DOMAIN);
    digest_field(&mut digest, reference.owner_namespace.as_bytes())?;
    digest_field(&mut digest, reference.schema_id.as_bytes())?;
    digest.update(reference.schema_version.to_be_bytes());
    digest.update(reference.person_id.0.as_bytes());
    digest.update(reference.session_id.as_bytes());
    digest.update(reference.entry_id.as_bytes());
    digest.update(reference.turn_id.as_bytes());
    digest_field(
        &mut digest,
        reference.provenance.as_storage_value().as_bytes(),
    )?;
    digest.update(payload_length.to_be_bytes());
    digest.update(payload);
    Ok(digest.finalize().into())
}

fn digest_field(digest: &mut Sha256, value: &[u8]) -> Result<(), AgentFailure> {
    let length = u64::try_from(value.len()).map_err(|_| AgentFailure::BudgetExceeded)?;
    digest.update(length.to_be_bytes());
    digest.update(value);
    Ok(())
}

pub(super) fn validate_agent_message(message: &AgentMessage) -> Result<(), AgentFailure> {
    if message.turn_id().is_nil() {
        return Err(AgentFailure::InvalidInput);
    }
    match message {
        AgentMessage::Compaction {
            summary, recovery, ..
        } => {
            if summary.len() > crate::MAX_COMPACTION_SUMMARY_BYTES
                || recovery.archive_id.is_nil()
                || recovery.through_turn_id.is_nil()
                || recovery.archived_message_count == 0
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        AgentMessage::Preamble { text, .. } => {
            if text.len() > MAX_OUTPUT_BYTES {
                return Err(AgentFailure::InvalidInput);
            }
        }
        AgentMessage::User {
            message_id, text, ..
        } => {
            if message_id.is_nil() || text.len() > crate::MAX_TURN_TEXT_BYTES {
                return Err(AgentFailure::InvalidInput);
            }
        }
        AgentMessage::Assistant { text, .. } => {
            if text.len() > MAX_OUTPUT_BYTES {
                return Err(AgentFailure::InvalidInput);
            }
        }
        AgentMessage::Capability {
            call_id,
            capability_id,
            input,
            result,
            ..
        } => {
            if call_id.is_nil()
                || capability_id.is_empty()
                || capability_id.len() > 256
                || input.len() > MAX_OUTPUT_BYTES
                || result
                    .as_ref()
                    .is_ok_and(|output| output.len() > MAX_OUTPUT_BYTES)
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        AgentMessage::Delegation {
            task,
            execution_receipt,
            ..
        } => {
            task.validate(MAX_OUTPUT_BYTES)?;
            if let Some(reference) = execution_receipt {
                reference.validate()?;
                if reference.execution.task_id != task.task_id {
                    return Err(AgentFailure::InvalidInput);
                }
            }
        }
        AgentMessage::Interaction { interaction_id, .. } => {
            if interaction_id.is_nil() {
                return Err(AgentFailure::InvalidInput);
            }
        }
    }
    Ok(())
}
