//! Immutable host input and Core identity for one Expert Task Run segment.
//!
//! These values connect Task-owned execution to neutral Conversation Core
//! custody. They do not create a second Task or Run lifecycle.

use floe_agent_contract::{
    AgentFailure, DependencyCoverage, MAX_AGENT_MESSAGES, MAX_MODEL_CONVERSATION_BYTES,
    ModelConversation, ModelConversationEntry, PackageKind, PackageRef, TaskBlockage,
    TaskExecutionKey,
};
use floe_conversation_contract::{
    AgentIdentity, AgentInstanceId, AssignmentId, ConversationBranchId, ConversationId,
    MAX_CONVERSATION_MESSAGE_BYTES, MessageId, TranscriptReference,
};
use floe_kernel::{CommandId, PersonId, RunId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ExpertAdmissionIdentity;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertConversationKey {
    pub person_id: PersonId,
    pub registry_instance_id: Uuid,
    pub installation_id: Uuid,
    pub assignment_id: Uuid,
    pub package: PackageRef,
    pub definition_revision: u64,
}

impl ExpertConversationKey {
    pub fn from_admission(
        person_id: PersonId,
        admission: &ExpertAdmissionIdentity,
    ) -> Result<Self, AgentFailure> {
        let key = Self {
            person_id,
            registry_instance_id: admission.registry_instance_id,
            installation_id: admission.installation_id,
            assignment_id: admission.assignment_id,
            package: admission.package.clone(),
            definition_revision: admission.definition_revision,
        };
        key.validate()?;
        Ok(key)
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        let valid_name = |value: &str| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        };
        if !self.person_id.is_valid()
            || self.registry_instance_id.is_nil()
            || self.installation_id.is_nil()
            || self.assignment_id.is_nil()
            || self.package.kind != PackageKind::Expert
            || !valid_name(&self.package.id)
            || !valid_name(&self.package.version)
            || self.definition_revision == 0
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    /// Core's role-neutral identity uses the actual installed Expert instance
    /// and assignment. The definition identifier binds both package ID and
    /// package version; the revision pins the admitted definition policy.
    pub fn core_identity(&self) -> Result<AgentIdentity, AgentFailure> {
        self.validate()?;
        let identity = AgentIdentity {
            person_id: self.person_id,
            agent_instance_id: AgentInstanceId::from_uuid(self.installation_id)
                .ok_or(AgentFailure::InvalidInput)?,
            assignment_id: AssignmentId::from_uuid(self.assignment_id)
                .ok_or(AgentFailure::InvalidInput)?,
            definition_id: format!("expert:{}@{}", self.package.id, self.package.version),
            definition_revision: self.definition_revision,
        };
        identity
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        Ok(identity)
    }

    pub fn matches_admission(&self, admission: &ExpertAdmissionIdentity) -> bool {
        self.registry_instance_id == admission.registry_instance_id
            && self.installation_id == admission.installation_id
            && self.assignment_id == admission.assignment_id
            && self.package == admission.package
            && self.definition_revision == admission.definition_revision
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertTaskConversationDraft {
    pub key: ExpertConversationKey,
    pub run_id: RunId,
    pub input_message_id: MessageId,
    pub input_command_id: CommandId,
    pub delegated_message: String,
    pub input_coverage: DependencyCoverage,
}

impl ExpertTaskConversationDraft {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.key.validate()?;
        if !self.run_id.is_valid()
            || !self.input_message_id.is_valid()
            || !self.input_command_id.is_valid()
            || self.delegated_message.len() > MAX_CONVERSATION_MESSAGE_BYTES
            || self
                .delegated_message
                .chars()
                .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
            || self.input_coverage.validate().is_err()
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// Exact transcript boundary pinned by the Task admission transaction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertHistoryPin {
    pub head_revision: u64,
    pub head_reference: Option<TranscriptReference>,
    pub prefix_digest: [u8; 32],
}

impl ExpertHistoryPin {
    pub fn validate(
        &self,
        conversation_id: ConversationId,
        branch_id: ConversationBranchId,
    ) -> Result<(), AgentFailure> {
        match (self.head_revision, self.head_reference) {
            (0, None) if self.prefix_digest == [0; 32] => Ok(()),
            (revision, Some(reference))
                if revision > 0
                    && reference.conversation_id == conversation_id
                    && reference.branch_id == branch_id
                    && reference.sequence == revision
                    && reference.validate().is_ok()
                    && self.prefix_digest != [0; 32] =>
            {
                Ok(())
            }
            _ => Err(AgentFailure::StorageUnavailable),
        }
    }
}

/// Canonical host Task input retained separately from the external delegation
/// digest. Vault creates the stable assignment binding and fills its exact pin
/// in the same transaction that admits the Task.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertTaskConversationInput {
    pub execution: TaskExecutionKey,
    pub request_digest: [u8; 32],
    pub key: ExpertConversationKey,
    pub identity: AgentIdentity,
    pub conversation_id: ConversationId,
    pub branch_id: ConversationBranchId,
    pub run_id: RunId,
    pub input_message_id: MessageId,
    pub input_command_id: CommandId,
    pub delegated_message: String,
    pub input_coverage: DependencyCoverage,
    /// Filled atomically with Submitted -> Working and Core input append.
    pub input_reference: Option<TranscriptReference>,
    pub history_pin: ExpertHistoryPin,
}

impl ExpertTaskConversationInput {
    /// Stable owner commitment for the complete local Expert input. This is
    /// deliberately independent of the external delegation request digest.
    pub fn commitment(&self) -> Result<[u8; 32], AgentFailure> {
        let canonical = serde_json::to_vec(&(
            &self.execution,
            self.request_digest,
            &self.key,
            &self.identity,
            self.conversation_id,
            self.branch_id,
            self.run_id,
            self.input_message_id,
            self.input_command_id,
            &self.delegated_message,
            &self.input_coverage,
            self.history_pin,
        ))
        .map_err(|_| AgentFailure::StorageUnavailable)?;
        let mut digest = sha2::Sha256::new();
        use sha2::Digest;
        digest.update(b"floe-expert-task-host-input-v1\0");
        digest.update((canonical.len() as u64).to_be_bytes());
        digest.update(canonical);
        Ok(digest.finalize().into())
    }

    /// Issue the receipt only while constructing the atomic owner admission.
    /// Replay paths load the persisted receipt and compare this input against it.
    pub fn issue_admission_reference(&self) -> Result<ExpertTaskAdmissionReference, AgentFailure> {
        self.validate()?;
        Ok(ExpertTaskAdmissionReference {
            execution: self.execution,
            request_digest: self.request_digest,
            run_id: self.run_id,
            conversation_id: self.conversation_id,
            branch_id: self.branch_id,
            history_pin: self.history_pin,
            host_input_commitment: self.commitment()?,
        })
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.execution.validate()?;
        self.key.validate()?;
        self.identity
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        self.history_pin
            .validate(self.conversation_id, self.branch_id)?;
        if self.identity != self.key.core_identity()?
            || !self.conversation_id.is_valid()
            || !self.branch_id.is_valid()
            || !self.run_id.is_valid()
            || !self.input_message_id.is_valid()
            || !self.input_command_id.is_valid()
            || self.delegated_message.len() > MAX_CONVERSATION_MESSAGE_BYTES
            || self
                .delegated_message
                .chars()
                .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
            || self.input_coverage.validate().is_err()
            || self.input_reference.is_some_and(|reference| {
                reference.conversation_id != self.conversation_id
                    || reference.branch_id != self.branch_id
                    || reference.message_id != self.input_message_id
                    || reference.sequence != self.history_pin.head_revision.saturating_add(1)
                    || reference.validate().is_err()
            })
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

/// Owner-issued, immutable admission reference required by the Submitted to
/// Working CAS. It binds the local Task input and exact pinned Expert prefix.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertTaskAdmissionReference {
    pub execution: TaskExecutionKey,
    pub request_digest: [u8; 32],
    pub run_id: RunId,
    pub conversation_id: ConversationId,
    pub branch_id: ConversationBranchId,
    pub history_pin: ExpertHistoryPin,
    pub host_input_commitment: [u8; 32],
}

impl ExpertTaskAdmissionReference {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.execution.validate()?;
        if !self.run_id.is_valid()
            || !self.conversation_id.is_valid()
            || !self.branch_id.is_valid()
            || self.host_input_commitment == [0; 32]
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        self.history_pin
            .validate(self.conversation_id, self.branch_id)
    }
}

/// The only typed content Vault stores for an Expert Core output. Blocked
/// evidence is retained for owner audit but is never projected into model
/// history as if it were an answer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpertConversationEvidence {
    ModelEntry {
        entry: ModelConversationEntry,
        coverage: DependencyCoverage,
    },
    Blocked {
        blockage: TaskBlockage,
        coverage: DependencyCoverage,
    },
}

impl ExpertConversationEvidence {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        match self {
            Self::ModelEntry { entry, coverage } => {
                entry.validate()?;
                coverage
                    .validate()
                    .map_err(|_| AgentFailure::PolicyDenied)?;
                if matches!(
                    entry,
                    ModelConversationEntry::User { .. }
                        | ModelConversationEntry::Preamble { .. }
                        | ModelConversationEntry::DelegationExchange { .. }
                ) {
                    return Err(AgentFailure::PolicyDenied);
                }
                Ok(())
            }
            Self::Blocked { blockage, coverage } => {
                blockage.validate()?;
                coverage.validate().map_err(|_| AgentFailure::PolicyDenied)
            }
        }
    }
}

/// A pinned Expert history snapshot ready for the Engine. Coverage is aligned
/// one-to-one with `conversation.history` and must be reauthorized by Context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpertTaskConversation {
    pub conversation: ModelConversation,
    pub history_coverage: Vec<DependencyCoverage>,
}

impl ExpertTaskConversation {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.conversation.validate()?;
        if self.history_coverage.len() != self.conversation.history.len()
            || self.conversation.history.len() > MAX_AGENT_MESSAGES
            || serde_json::to_vec(&self.conversation)
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .len()
                > MAX_MODEL_CONVERSATION_BYTES
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        for coverage in &self.history_coverage {
            coverage
                .validate()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
        }
        Ok(())
    }
}
