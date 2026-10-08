use floe_kernel::{CommandId, PersonId, TaskId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    AgentIdentity, AgentInstanceId, ConversationBranchId, ConversationFailure, ConversationId,
    ConversationReference, MessageId, TranscriptReference,
};

pub const MAX_CONVERSATION_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAX_CHECKPOINT_SUMMARY_BYTES: usize = 64 * 1024;

/// Provenance supplied by an owner that has already authenticated the sender.
/// A message's model role is not an origin or authority grant.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MessageOrigin {
    Person {
        person_id: PersonId,
    },
    Agent {
        agent_instance_id: AgentInstanceId,
    },
    Host,
    Tool {
        agent_instance_id: AgentInstanceId,
        tool_id: String,
    },
}

impl MessageOrigin {
    fn validate(&self) -> Result<(), ConversationFailure> {
        match self {
            Self::Person { person_id } if !person_id.is_valid() => {
                Err(ConversationFailure::InvalidInput)
            }
            Self::Agent { agent_instance_id } if !agent_instance_id.is_valid() => {
                Err(ConversationFailure::InvalidInput)
            }
            Self::Tool {
                agent_instance_id,
                tool_id,
            } if !agent_instance_id.is_valid()
                || tool_id.trim().is_empty()
                || tool_id.len() > 128
                || tool_id.chars().any(char::is_control) =>
            {
                Err(ConversationFailure::InvalidInput)
            }
            _ => Ok(()),
        }
    }
}

/// Message identity and content. `command_id` is independent from the
/// conversation-local `message_id` and the host-owned `task_id`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationMessage {
    pub message_id: MessageId,
    pub command_id: CommandId,
    pub origin: MessageOrigin,
    pub text: String,
    /// Content-addressed reference to separately stored host evidence, such
    /// as attachments. The reference carries no authority and is not a wire
    /// payload or storage schema.
    pub evidence: Option<MessageEvidenceReference>,
    pub task_id: Option<TaskId>,
}

/// A stable commitment to semantic evidence associated with a message.
/// Evidence bytes remain owned and stored by their host domain.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MessageEvidenceReference {
    digest: [u8; 32],
}

impl MessageEvidenceReference {
    /// Store a content digest computed over the host-defined semantic evidence
    /// encoding. Callers must use a deterministic, domain-separated digest.
    pub fn from_digest(digest: [u8; 32]) -> Self {
        Self { digest }
    }

    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

impl ConversationMessage {
    pub fn validate(&self) -> Result<(), ConversationFailure> {
        if !self.message_id.is_valid()
            || !self.command_id.is_valid()
            || self.text.trim().is_empty()
            || self.text.len() > MAX_CONVERSATION_MESSAGE_BYTES
            || self
                .text
                .chars()
                .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
            || self.task_id.is_some_and(|id| !id.is_valid())
        {
            return Err(ConversationFailure::InvalidInput);
        }
        self.origin.validate()
    }

    /// Digest text and the associated evidence reference as one semantic body.
    pub fn body_digest(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(b"floe-conversation-message-body-v1\0");
        digest.update((self.text.len() as u64).to_be_bytes());
        digest.update(self.text.as_bytes());
        if let Some(evidence) = &self.evidence {
            digest.update([1]);
            digest.update(evidence.digest());
        } else {
            digest.update([0]);
        }
        digest.finalize().into()
    }

    /// Compare the stable message identity, body, authenticated origin and
    /// host Task association for replay classification. Command identity is a
    /// separate deduplication key and is intentionally checked by the owner.
    pub fn same_delivery(&self, other: &Self) -> bool {
        self.message_id == other.message_id
            && self.body_digest() == other.body_digest()
            && self.origin == other.origin
            && self.task_id == other.task_id
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdmissionTarget {
    New {
        conversation_id: ConversationId,
        branch_id: ConversationBranchId,
        identity: AgentIdentity,
    },
    Continue {
        reference: ConversationReference,
    },
}

impl AdmissionTarget {
    pub fn validate(&self) -> Result<(), ConversationFailure> {
        match self {
            Self::New {
                identity,
                conversation_id,
                branch_id,
            } => {
                identity.validate()?;
                if !conversation_id.is_valid() || !branch_id.is_valid() {
                    return Err(ConversationFailure::InvalidInput);
                }
            }
            Self::Continue { reference } => reference.validate()?,
        }
        Ok(())
    }

    /// Continue only when the supplied reference belongs to the exact pinned
    /// agent identity. A changed definition revision (or another identity
    /// change) defaults to an isolated new conversation.
    pub fn default_for_identity(
        identity: AgentIdentity,
        prior: Option<&ConversationReference>,
        new_conversation_id: ConversationId,
        new_branch_id: ConversationBranchId,
    ) -> Result<Self, ConversationFailure> {
        identity.validate()?;
        if !new_conversation_id.is_valid() || !new_branch_id.is_valid() {
            return Err(ConversationFailure::InvalidInput);
        }
        if let Some(reference) = prior {
            reference.validate()?;
            if reference.identity == identity {
                return Ok(Self::Continue {
                    reference: reference.clone(),
                });
            }
        }
        Ok(Self::New {
            conversation_id: new_conversation_id,
            branch_id: new_branch_id,
            identity,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MessageAdmissionRequest {
    pub target: AdmissionTarget,
    pub message: ConversationMessage,
}

impl MessageAdmissionRequest {
    pub fn validate(&self) -> Result<(), ConversationFailure> {
        self.target.validate()?;
        self.message.validate()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionDisposition {
    Appended,
    Queued,
    Replayed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionReceipt {
    pub transcript: TranscriptReference,
    pub head_revision: u64,
    pub task_id: Option<TaskId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionResult {
    pub disposition: AdmissionDisposition,
    pub receipt: AdmissionReceipt,
}

/// A checkpoint is tied to one exact conversation branch and transcript
/// prefix. It is a conversation value, not a model authorization proof.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationCheckpoint {
    pub through: TranscriptReference,
    pub prefix_digest: [u8; 32],
    pub summary: String,
}

impl ConversationCheckpoint {
    pub fn validate(&self) -> Result<(), ConversationFailure> {
        self.through.validate()?;
        if self.summary.trim().is_empty()
            || self.summary.len() > MAX_CHECKPOINT_SUMMARY_BYTES
            || self
                .summary
                .chars()
                .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
        {
            return Err(ConversationFailure::InvalidInput);
        }
        Ok(())
    }
}
