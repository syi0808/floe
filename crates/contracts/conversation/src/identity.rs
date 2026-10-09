use floe_kernel::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ConversationFailure;

macro_rules! uuid_identity {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn from_uuid(value: Uuid) -> Option<Self> {
                (!value.is_nil()).then_some(Self(value))
            }

            pub fn as_uuid(self) -> Uuid {
                self.0
            }

            pub fn is_valid(self) -> bool {
                !self.0.is_nil()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

uuid_identity!(AgentInstanceId);
uuid_identity!(AssignmentId);
uuid_identity!(ConversationId);
uuid_identity!(ConversationBranchId);
uuid_identity!(MessageId);
uuid_identity!(LogicalContributionId);

/// Stable owner and assignment identity, separate from a role name or model
/// role. The definition revision is pinned separately from the instance ID.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentIdentity {
    pub person_id: PersonId,
    pub agent_instance_id: AgentInstanceId,
    pub assignment_id: AssignmentId,
    pub definition_id: String,
    pub definition_revision: u64,
}

impl AgentIdentity {
    pub fn validate(&self) -> Result<(), ConversationFailure> {
        if !self.person_id.is_valid()
            || !self.agent_instance_id.is_valid()
            || !self.assignment_id.is_valid()
            || self.definition_id.trim().is_empty()
            || self.definition_id.len() > 512
            || self.definition_id.chars().any(char::is_control)
            || self.definition_revision == 0
        {
            return Err(ConversationFailure::InvalidInput);
        }
        Ok(())
    }
}

/// Explicit local reference to one agent-owned conversation head.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationReference {
    pub conversation_id: ConversationId,
    pub branch_id: ConversationBranchId,
    pub identity: AgentIdentity,
    pub head_revision: u64,
}

impl ConversationReference {
    pub fn validate(&self) -> Result<(), ConversationFailure> {
        self.identity.validate()?;
        if !self.conversation_id.is_valid() || !self.branch_id.is_valid() || self.head_revision == 0
        {
            return Err(ConversationFailure::InvalidInput);
        }
        Ok(())
    }
}

/// A stable pointer into a transcript prefix, never an authorization grant.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptReference {
    pub conversation_id: ConversationId,
    pub branch_id: ConversationBranchId,
    pub message_id: MessageId,
    pub sequence: u64,
}

impl TranscriptReference {
    pub fn validate(self) -> Result<(), ConversationFailure> {
        if !self.conversation_id.is_valid()
            || !self.branch_id.is_valid()
            || !self.message_id.is_valid()
            || self.sequence == 0
        {
            return Err(ConversationFailure::InvalidInput);
        }
        Ok(())
    }
}
