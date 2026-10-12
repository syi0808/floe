//! Explicit, exact-target forgetting. Knowledge owns the deletion and replay contract.
use chrono::{DateTime, Utc};
use floe_kernel::{AgentFailure, CommandId, PersonId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryForgetRequest {
    pub command_id: CommandId,
    pub target_id: Uuid,
    pub expected_revision: u64,
}

impl MemoryForgetRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.command_id.is_valid()
            || self.target_id.is_nil()
            || self.expected_revision == 0
            || self.expected_revision > i64::MAX as u64
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryForgetAcknowledgement {
    pub command_id: CommandId,
    pub target_id: Uuid,
    pub forgotten_revision: u64,
    pub committed_at: DateTime<Utc>,
}

/// Validate retained identity/provenance before removing a memory revision.
/// The exclusion records contain identities only, never forgotten text.
pub fn memory_forget_evidence<'a>(
    revision: &'a crate::KnowledgeRevision,
    person_id: PersonId,
    target_id: Uuid,
) -> Result<&'a [crate::LearningEvidenceRef], AgentFailure> {
    if revision.schema_version != crate::KNOWLEDGE_VERSION
        || revision.person_id != person_id
        || revision.target_id != target_id
        || target_id.is_nil()
        || revision.revision == 0
        || revision.kind != crate::KnowledgeKind::Memory
        || !matches!(revision.payload, crate::KnowledgePayload::Memory { .. })
        || revision.source_refs.is_empty()
        || revision.source_refs.len() > 32
        || revision
            .source_refs
            .iter()
            .any(|r| r.session_id.is_nil() || r.turn_id.is_nil())
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(&revision.source_refs)
}

#[cfg(test)]
mod tests {
    #[test]
    fn forget_requests_are_not_background_learning_proposals() {
        for text in [
            "Forget my preference for tea",
            "Please forget this memory",
            "이 내용은 기억에서 지워",
            "그건 잊어줘",
        ] {
            assert_eq!(crate::explicit_learning_signal(text), None);
        }
        assert_eq!(
            crate::explicit_learning_signal("Please remember that I prefer tea"),
            Some(crate::LearningObservationKind::ExplicitRemember)
        );
    }
}
