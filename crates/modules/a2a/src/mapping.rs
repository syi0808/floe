use floe_conversation_contract::{
    AdmissionTarget, AgentInstanceId, ConversationMessage, MessageAdmissionRequest,
    MessageEvidenceReference, MessageId, MessageOrigin,
};
use floe_kernel::{CommandId, TaskId};
use sha2::{Digest, Sha256};

use crate::{
    A2aArtifact, A2aEnvelope, A2aFailure, A2aPeerAgentId, A2aPeerContextId, A2aPeerId,
    A2aPeerMessageId, A2aPeerTaskId, A2aProtocolPolicy,
};

/// Identity returned by the host's authenticated peer-binding boundary.
/// Neither the peer's role label nor its model output constructs this value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedPeerAgent {
    peer_id: A2aPeerId,
    agent_id: A2aPeerAgentId,
    agent_instance_id: AgentInstanceId,
}

impl AuthenticatedPeerAgent {
    /// Call only after the binding has authenticated this peer and resolved its
    /// stable agent instance. This constructor validates and packages that
    /// result; it does not authenticate a transport or grant source/product
    /// authority.
    pub fn from_verified_binding(
        peer_id: A2aPeerId,
        agent_id: A2aPeerAgentId,
        agent_instance_id: AgentInstanceId,
    ) -> Result<Self, A2aFailure> {
        peer_id.validate()?;
        agent_id.validate()?;
        if !agent_instance_id.is_valid() {
            return Err(A2aFailure::InvalidPeerBinding);
        }
        Ok(Self {
            peer_id,
            agent_id,
            agent_instance_id,
        })
    }

    pub fn peer_id(&self) -> &A2aPeerId {
        &self.peer_id
    }

    pub fn agent_id(&self) -> &A2aPeerAgentId {
        &self.agent_id
    }

    pub fn agent_instance_id(&self) -> AgentInstanceId {
        self.agent_instance_id
    }

    fn validate(&self) -> Result<(), A2aFailure> {
        self.peer_id.validate()?;
        self.agent_id.validate()?;
        if !self.agent_instance_id.is_valid() {
            return Err(A2aFailure::InvalidPeerBinding);
        }
        Ok(())
    }
}

/// Mapping selected by the A2A identity owner. Remote IDs never parse directly
/// into local Conversation, Message, Command, or Task IDs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct A2aInboundMapping {
    pub peer_id: A2aPeerId,
    pub remote_context_id: A2aPeerContextId,
    pub remote_message_id: A2aPeerMessageId,
    pub remote_task_id: Option<A2aPeerTaskId>,
    pub host_agent_instance_id: AgentInstanceId,
    pub local_target: AdmissionTarget,
    pub local_message_id: MessageId,
    pub local_command_id: CommandId,
    pub local_task_id: Option<TaskId>,
}

/// Validated remote payload mapped to a host Conversation admission. The
/// message's evidence reference commits to the artifacts, which remain
/// separate for the host Task owner to persist and interpret explicitly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct A2aInboundAdmission {
    pub message: MessageAdmissionRequest,
    pub artifacts: Vec<A2aArtifact>,
}

impl A2aInboundAdmission {
    /// Recheck the message and artifact commitment before the host persists or
    /// forwards this mapped payload.
    pub fn validate(&self) -> Result<(), A2aFailure> {
        validate_artifact_commitment(&self.message, &self.artifacts)
    }
}

impl A2aInboundMapping {
    pub fn validate(&self) -> Result<(), A2aFailure> {
        self.peer_id.validate()?;
        self.remote_context_id.validate()?;
        self.remote_message_id.validate()?;
        if let Some(task_id) = &self.remote_task_id {
            task_id.validate()?;
        }
        if !self.host_agent_instance_id.is_valid()
            || !self.local_message_id.is_valid()
            || !self.local_command_id.is_valid()
            || self.local_task_id.is_some_and(|id| !id.is_valid())
        {
            return Err(A2aFailure::MappingMismatch);
        }
        self.local_target
            .validate()
            .map_err(|_| A2aFailure::MappingMismatch)?;
        let identity = target_identity(&self.local_target);
        if identity.agent_instance_id != self.host_agent_instance_id {
            return Err(A2aFailure::MappingMismatch);
        }
        Ok(())
    }
}

pub fn map_inbound_message(
    envelope: &A2aEnvelope,
    policy: &A2aProtocolPolicy,
    peer: &AuthenticatedPeerAgent,
    mapping: &A2aInboundMapping,
) -> Result<A2aInboundAdmission, A2aFailure> {
    envelope.validate(policy)?;
    mapping.validate()?;
    peer.validate()?;
    let message = &envelope.message;
    if peer.peer_id != message.peer_id || peer.agent_id != message.sender_agent_id {
        return Err(A2aFailure::InvalidPeerBinding);
    }
    if mapping.peer_id != message.peer_id
        || mapping.remote_context_id != message.context_id
        || mapping.remote_message_id != message.message_id
        || mapping.remote_task_id != message.task_id
        || mapping.remote_task_id.is_some() != mapping.local_task_id.is_some()
    {
        return Err(A2aFailure::MappingMismatch);
    }
    let evidence = artifact_evidence_reference(&message.artifacts);
    let admission = MessageAdmissionRequest {
        target: mapping.local_target.clone(),
        message: ConversationMessage {
            message_id: mapping.local_message_id,
            command_id: mapping.local_command_id,
            origin: MessageOrigin::Agent {
                agent_instance_id: peer.agent_instance_id,
            },
            text: message.content.clone(),
            evidence,
            task_id: mapping.local_task_id,
        },
    };
    admission
        .validate()
        .map_err(|_| A2aFailure::MappingMismatch)?;
    let admission = A2aInboundAdmission {
        message: admission,
        artifacts: message.artifacts.clone(),
    };
    admission.validate()?;
    Ok(admission)
}

fn target_identity(target: &AdmissionTarget) -> &floe_conversation_contract::AgentIdentity {
    match target {
        AdmissionTarget::New { identity, .. } => identity,
        AdmissionTarget::AppendToExisting { reference } => &reference.identity,
    }
}

pub(crate) fn validate_artifact_commitment(
    message: &MessageAdmissionRequest,
    artifacts: &[A2aArtifact],
) -> Result<(), A2aFailure> {
    message
        .validate()
        .map_err(|_| A2aFailure::MappingMismatch)?;
    crate::exchange::validate_artifacts(artifacts)?;
    if message.message.evidence != artifact_evidence_reference(artifacts) {
        return Err(A2aFailure::MappingMismatch);
    }
    Ok(())
}

fn artifact_evidence_reference(artifacts: &[A2aArtifact]) -> Option<MessageEvidenceReference> {
    if artifacts.is_empty() {
        return None;
    }
    let mut digest = Sha256::new();
    digest.update(b"floe-a2a-semantic-artifacts-v1\0");
    digest.update((artifacts.len() as u64).to_be_bytes());
    for artifact in artifacts {
        digest_string(&mut digest, &artifact.artifact_id);
        digest_string(&mut digest, &artifact.name);
        digest.update((artifact.parts.len() as u64).to_be_bytes());
        for part in &artifact.parts {
            match part {
                crate::A2aArtifactPart::Text { text } => {
                    digest.update([0]);
                    digest_string(&mut digest, text);
                }
                crate::A2aArtifactPart::Data { media_type, data } => {
                    digest.update([1]);
                    digest_string(&mut digest, media_type);
                    digest_string(&mut digest, data);
                }
            }
        }
    }
    Some(MessageEvidenceReference::from_digest(
        digest.finalize().into(),
    ))
}

fn digest_string(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}
