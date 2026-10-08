use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

// These are provisional framing/resource guards for the in-process contract.
// They are not measured product context, prompt, or user-content defaults.
pub const A2A_EXCHANGE_CONTRACT_VERSION: u32 = 1;
pub const MAX_A2A_ID_BYTES: usize = 256;
pub const MAX_A2A_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAX_A2A_ARTIFACT_BYTES: usize = 256 * 1024;
pub const MAX_A2A_ARTIFACTS: usize = 16;
pub const MAX_A2A_ARTIFACT_PARTS: usize = 64;
pub const MAX_A2A_TOTAL_ARTIFACT_BYTES: usize = 1024 * 1024;
pub const MAX_A2A_ENVELOPE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_A2A_EXTENSIONS: usize = 32;

macro_rules! external_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn try_new(value: impl Into<String>) -> Result<Self, A2aFailure> {
                let value = value.into();
                if !bounded_identifier(&value, MAX_A2A_ID_BYTES) {
                    return Err(A2aFailure::InvalidEnvelope);
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub(crate) fn validate(&self) -> Result<(), A2aFailure> {
                if bounded_identifier(&self.0, MAX_A2A_ID_BYTES) {
                    Ok(())
                } else {
                    Err(A2aFailure::InvalidEnvelope)
                }
            }
        }
    };
}

external_id!(A2aPeerId);
external_id!(A2aPeerAgentId);
external_id!(A2aPeerContextId);
external_id!(A2aPeerMessageId);
external_id!(A2aPeerTaskId);

/// Floe's internal exchange contract version. It is not a claim of remote A2A
/// standard conformance; a future external binding owns its pinned mapping.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct A2aEnvelope {
    pub contract_version: u32,
    pub extensions: Vec<String>,
    pub message: A2aMessage,
}

impl A2aEnvelope {
    pub fn validate(&self, policy: &A2aProtocolPolicy) -> Result<(), A2aFailure> {
        if self.contract_version != A2A_EXCHANGE_CONTRACT_VERSION {
            return Err(A2aFailure::UnsupportedVersion);
        }
        if self.extensions.len() > MAX_A2A_EXTENSIONS {
            return Err(A2aFailure::InvalidEnvelope);
        }
        let mut seen = BTreeSet::new();
        for extension in &self.extensions {
            if !bounded_identifier(extension, MAX_A2A_ID_BYTES)
                || !seen.insert(extension)
                || !policy.allowed_extensions.contains(extension)
            {
                return Err(A2aFailure::UnsupportedExtension);
            }
        }
        self.message.validate()?;
        if serde_json::to_vec(self)
            .map_err(|_| A2aFailure::InvalidEnvelope)?
            .len()
            > MAX_A2A_ENVELOPE_BYTES
        {
            return Err(A2aFailure::InvalidEnvelope);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct A2aProtocolPolicy {
    allowed_extensions: BTreeSet<String>,
}

impl A2aProtocolPolicy {
    pub fn new(allowed_extensions: impl IntoIterator<Item = String>) -> Result<Self, A2aFailure> {
        let mut allowed = BTreeSet::new();
        for extension in allowed_extensions {
            if !bounded_identifier(&extension, MAX_A2A_ID_BYTES) || !allowed.insert(extension) {
                return Err(A2aFailure::UnsupportedExtension);
            }
        }
        if allowed.len() > MAX_A2A_EXTENSIONS {
            return Err(A2aFailure::UnsupportedExtension);
        }
        Ok(Self {
            allowed_extensions: allowed,
        })
    }

    pub fn allows(&self, extension: &str) -> bool {
        self.allowed_extensions.contains(extension)
    }
}

/// A transport-neutral request. Remote IDs remain separate from local Floe
/// IDs and require an explicit mapping before admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct A2aMessage {
    pub peer_id: A2aPeerId,
    pub sender_agent_id: A2aPeerAgentId,
    pub context_id: A2aPeerContextId,
    pub message_id: A2aPeerMessageId,
    pub task_id: Option<A2aPeerTaskId>,
    pub content: String,
    pub artifacts: Vec<A2aArtifact>,
}

impl A2aMessage {
    fn validate(&self) -> Result<(), A2aFailure> {
        self.peer_id.validate()?;
        self.sender_agent_id.validate()?;
        self.context_id.validate()?;
        self.message_id.validate()?;
        if let Some(task_id) = &self.task_id {
            task_id.validate()?;
        }
        if !bounded_text(&self.content, MAX_A2A_MESSAGE_BYTES) {
            return Err(A2aFailure::InvalidEnvelope);
        }
        validate_artifacts(&self.artifacts)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct A2aArtifact {
    pub artifact_id: String,
    pub name: String,
    pub parts: Vec<A2aArtifactPart>,
}

impl A2aArtifact {
    fn validate(&self) -> Result<(), A2aFailure> {
        if !bounded_identifier(&self.artifact_id, MAX_A2A_ID_BYTES)
            || !bounded_identifier(&self.name, MAX_A2A_ID_BYTES)
            || self.parts.is_empty()
            || self.parts.len() > MAX_A2A_ARTIFACT_PARTS
        {
            return Err(A2aFailure::InvalidEnvelope);
        }
        for part in &self.parts {
            match part {
                A2aArtifactPart::Text { text } if !bounded_text(text, MAX_A2A_ARTIFACT_BYTES) => {
                    return Err(A2aFailure::InvalidEnvelope);
                }
                A2aArtifactPart::Data { media_type, data }
                    if !bounded_identifier(media_type, MAX_A2A_ID_BYTES)
                        || !bounded_text(data, MAX_A2A_ARTIFACT_BYTES) =>
                {
                    return Err(A2aFailure::InvalidEnvelope);
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum A2aArtifactPart {
    Text { text: String },
    Data { media_type: String, data: String },
}

/// A peer's Task status is an external observation. It is not a local Task
/// snapshot, execution receipt, or evidence that a local effect completed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct A2aTaskObservation {
    pub peer_id: A2aPeerId,
    pub context_id: A2aPeerContextId,
    pub task_id: A2aPeerTaskId,
    pub status_code: String,
    pub artifacts: Vec<A2aArtifact>,
}

impl A2aTaskObservation {
    pub fn validate(&self) -> Result<(), A2aFailure> {
        self.peer_id.validate()?;
        self.context_id.validate()?;
        self.task_id.validate()?;
        if !bounded_identifier(&self.status_code, 64) {
            return Err(A2aFailure::InvalidEnvelope);
        }
        validate_artifacts(&self.artifacts)?;
        if serde_json::to_vec(self)
            .map_err(|_| A2aFailure::InvalidEnvelope)?
            .len()
            > MAX_A2A_ENVELOPE_BYTES
        {
            return Err(A2aFailure::InvalidEnvelope);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum A2aFailure {
    InvalidEnvelope,
    UnsupportedVersion,
    UnsupportedExtension,
    InvalidPeerBinding,
    MappingMismatch,
    TransportFailure,
    RemoteTaskFailure,
    HostTaskFailure,
}

fn bounded_identifier(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

fn bounded_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= maximum
        && !value
            .chars()
            .any(|character| character.is_control() && character != '\n' && character != '\t')
}

pub(crate) fn validate_artifacts(artifacts: &[A2aArtifact]) -> Result<(), A2aFailure> {
    if artifacts.len() > MAX_A2A_ARTIFACTS {
        return Err(A2aFailure::InvalidEnvelope);
    }
    let mut identities = BTreeSet::new();
    let mut total_bytes = 0usize;
    for artifact in artifacts {
        artifact.validate()?;
        if !identities.insert(artifact.artifact_id.as_str()) {
            return Err(A2aFailure::InvalidEnvelope);
        }
        for metadata in [&artifact.artifact_id, &artifact.name] {
            total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or(A2aFailure::InvalidEnvelope)?;
            if total_bytes > MAX_A2A_TOTAL_ARTIFACT_BYTES {
                return Err(A2aFailure::InvalidEnvelope);
            }
        }
        for part in &artifact.parts {
            let bytes = match part {
                A2aArtifactPart::Text { text } => text.len(),
                A2aArtifactPart::Data { media_type, data } => media_type
                    .len()
                    .checked_add(data.len())
                    .ok_or(A2aFailure::InvalidEnvelope)?,
            };
            total_bytes = total_bytes
                .checked_add(bytes)
                .ok_or(A2aFailure::InvalidEnvelope)?;
            if total_bytes > MAX_A2A_TOTAL_ARTIFACT_BYTES {
                return Err(A2aFailure::InvalidEnvelope);
            }
        }
    }
    Ok(())
}
