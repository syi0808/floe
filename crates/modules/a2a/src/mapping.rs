use floe_conversation_contract::{
    AdmissionTarget, AgentIdentity, AgentInstanceId, ConversationMessage, MessageAdmissionRequest,
    MessageId, MessageOrigin,
};
use floe_kernel::{CommandId, TaskId};

use crate::{
    A2aArtifact, A2aEnvelope, A2aFailure, A2aPeerAgentId, A2aPeerContextId, A2aPeerId,
    A2aPeerMessageId, A2aPeerTaskId, A2aProtocolPolicy,
};

/// Identity returned by the host's authenticated peer-binding boundary.
/// Neither the peer's role label nor its model output constructs this value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedPeerAgent {
    pub peer_id: A2aPeerId,
    pub agent_id: A2aPeerAgentId,
    pub agent_instance_id: AgentInstanceId,
}

impl AuthenticatedPeerAgent {
    /// Call only after the binding has authenticated this peer and resolved its
    /// stable agent instance. This value grants no source or product authority.
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

/// Validated remote payload mapped to a host Conversation admission, with
/// artifacts preserved for the host Task owner to interpret explicitly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct A2aInboundAdmission {
    pub message: MessageAdmissionRequest,
    pub artifacts: Vec<A2aArtifact>,
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
        let identity = target_identity(&self.local_target);
        identity
            .validate()
            .map_err(|_| A2aFailure::MappingMismatch)?;
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
    Ok(A2aInboundAdmission {
        message: MessageAdmissionRequest {
            target: mapping.local_target.clone(),
            message: ConversationMessage {
                message_id: mapping.local_message_id,
                command_id: mapping.local_command_id,
                origin: MessageOrigin::Agent {
                    agent_instance_id: peer.agent_instance_id,
                },
                text: message.content.clone(),
                task_id: mapping.local_task_id,
            },
        },
        artifacts: message.artifacts.clone(),
    })
}

fn target_identity(target: &AdmissionTarget) -> &AgentIdentity {
    match target {
        AdmissionTarget::New { identity, .. } => identity,
        AdmissionTarget::Continue { reference } => &reference.identity,
    }
}
