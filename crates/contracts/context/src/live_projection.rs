//! A live context's exact data/provenance binding, not dispatch authority.
use crate::{DataClass, DependencyCoverage, MAX_CONTEXT_MEMORIES, MemoryRevisionReference};
use floe_conversation_contract::{MessageId, SurfaceAnchor};
use floe_kernel::{AgentFailure, OwnerActor, PersonId};
use floe_model_contract::{LiveContextPayload, LiveModelPlan, ModelBindingDigest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fmt};
use uuid::Uuid;

/// A Person-facing prefix or one canonical result inside it. These never carry
/// an Agent Core turn ID, and callers cannot substitute a mutable latest head.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LiveProjectionSource {
    SurfacePrefix {
        through: SurfaceAnchor,
    },
    SurfaceResult {
        through: SurfaceAnchor,
        surface_message_id: MessageId,
    },
}
impl LiveProjectionSource {
    pub fn through(&self) -> SurfaceAnchor {
        match self {
            Self::SurfacePrefix { through } | Self::SurfaceResult { through, .. } => *through,
        }
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.through().validate()?;
        if let Self::SurfaceResult {
            through,
            surface_message_id,
        } = self
        {
            if through.sequence == 0 || !surface_message_id.is_valid() {
                return Err(AgentFailure::InvalidInput);
            }
        }
        Ok(())
    }
}

/// Context constructs this from canonical owner reads. The value itself is not
/// a permission or a claim that Source/Gateway/Knowledge remains authorized.
/// Inference must consume and continuously revalidate the matching live fence.
#[derive(Clone, Eq, PartialEq)]
pub struct LiveContextProjection {
    pub person_id: PersonId,
    pub device_id: String,
    pub runtime_epoch: u64,
    pub voice_session_id: Uuid,
    pub plan_id: Uuid,
    pub binding_digest: ModelBindingDigest,
    pub projection_id: Uuid,
    pub projection_revision: u64,
    pub source: LiveProjectionSource,
    pub coverage: DependencyCoverage,
    pub memories: Vec<MemoryRevisionReference>,
    pub input_data_classes: Vec<DataClass>,
    pub payload: LiveContextPayload,
}
impl LiveContextProjection {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        OwnerActor {
            person_id: self.person_id,
            device_id: self.device_id.clone(),
            runtime_epoch: self.runtime_epoch,
        }
        .validate()?;
        if self.voice_session_id.is_nil()
            || self.plan_id.is_nil()
            || self.projection_id.is_nil()
            || self.projection_revision == 0
            || self.binding_digest.0 == [0; 32]
            || self.input_data_classes.is_empty()
            || self.input_data_classes.len() > 16
            || self.input_data_classes.iter().any(|class| {
                !matches!(
                    class,
                    DataClass::Synthetic | DataClass::Personal | DataClass::HighlySensitive
                )
            })
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.source.validate()?;
        self.coverage
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        let mut seen = HashSet::new();
        if self.memories.len() > MAX_CONTEXT_MEMORIES
            || self.memories.iter().any(|memory| {
                memory.target_id.is_nil()
                    || memory.revision == 0
                    || memory.revision > i64::MAX as u64
                    || !seen.insert(memory.target_id)
            })
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
    pub fn validate_for_plan(
        &self,
        plan: &LiveModelPlan,
        voice_session_id: Uuid,
        surface_session_id: Uuid,
    ) -> Result<(), AgentFailure> {
        self.validate()?;
        plan.validate()?;
        if self.person_id != plan.actor.person_id
            || self.device_id != plan.actor.device_id
            || self.runtime_epoch != plan.actor.runtime_epoch
            || self.plan_id != plan.operation_id
            || self.binding_digest != plan.binding_digest
            || self.voice_session_id != voice_session_id
            || self.source.through().session_id != surface_session_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
    /// Correlation only. A digest cannot grant source processing or start audio.
    pub fn digest(&self) -> Result<[u8; 32], AgentFailure> {
        self.validate()?;
        let memories: Vec<_> = self
            .memories
            .iter()
            .map(|m| (m.target_id, m.revision))
            .collect();
        let bytes = serde_json::to_vec(&(
            self.person_id,
            &self.device_id,
            self.runtime_epoch,
            self.voice_session_id,
            self.plan_id,
            self.binding_digest,
            self.projection_id,
            self.projection_revision,
            &self.source,
            &self.coverage,
            memories,
            &self.input_data_classes,
            self.payload.as_str(),
        ))
        .map_err(|_| AgentFailure::InvalidInput)?;
        let mut digest = Sha256::new();
        digest.update(b"floe.live.context-projection.v1\0");
        digest.update(bytes);
        Ok(digest.finalize().into())
    }
}
impl fmt::Debug for LiveContextProjection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveContextProjection")
            .field("payload_bytes", &self.payload.byte_len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_model_contract::{
        LiveModelCapabilities, LiveTransportLimits, ModelSelectionCommitment, ProcessingBoundary,
    };
    fn fixture() -> (LiveContextProjection, LiveModelPlan) {
        let actor = OwnerActor {
            person_id: PersonId::new(),
            device_id: "fixture-device".into(),
            runtime_epoch: 1,
        };
        let plan = LiveModelPlan {
            operation_id: Uuid::new_v4(),
            actor: actor.clone(),
            capabilities: LiveModelCapabilities::conversational(),
            limits: LiveTransportLimits {
                initial_context_bytes: 4096,
                append_bytes: 500,
                local_session_duration_ms: 60000,
            },
            boundary: ProcessingBoundary::Device,
            binding_digest: ModelBindingDigest([1; 32]),
            selection_commitment: ModelSelectionCommitment([2; 32]),
        };
        let projection = LiveContextProjection {
            person_id: actor.person_id,
            device_id: actor.device_id,
            runtime_epoch: actor.runtime_epoch,
            voice_session_id: Uuid::new_v4(),
            plan_id: plan.operation_id,
            binding_digest: plan.binding_digest,
            projection_id: Uuid::new_v4(),
            projection_revision: 1,
            source: LiveProjectionSource::SurfacePrefix {
                through: SurfaceAnchor::empty(Uuid::new_v4()).unwrap(),
            },
            coverage: DependencyCoverage::Independent,
            memories: vec![],
            input_data_classes: vec![DataClass::Personal],
            payload: LiveContextPayload::new("private original context".into()).unwrap(),
        };
        (projection, plan)
    }
    #[test]
    fn projection_requires_exact_actor_plan_voice_and_surface_scope() {
        let (p, plan) = fixture();
        p.validate_for_plan(&plan, p.voice_session_id, p.source.through().session_id)
            .unwrap();
        for kind in 0..5 {
            let mut other = p.clone();
            match kind {
                0 => other.device_id = "other-device".into(),
                1 => other.runtime_epoch += 1,
                2 => other.plan_id = Uuid::new_v4(),
                3 => other.person_id = PersonId::new(),
                _ => other.binding_digest = ModelBindingDigest([3; 32]),
            }
            assert_eq!(
                other.validate_for_plan(&plan, p.voice_session_id, p.source.through().session_id),
                Err(AgentFailure::PolicyDenied)
            );
        }
        assert_eq!(
            p.validate_for_plan(&plan, Uuid::new_v4(), p.source.through().session_id),
            Err(AgentFailure::PolicyDenied)
        );
        assert_eq!(
            p.validate_for_plan(&plan, p.voice_session_id, Uuid::new_v4()),
            Err(AgentFailure::PolicyDenied)
        );
    }
    #[test]
    fn content_and_memory_revision_change_correlation_but_do_not_change_authority() {
        let (p, _) = fixture();
        let digest = p.digest().unwrap();
        let mut changed = p.clone();
        changed.payload = LiveContextPayload::new("different body".into()).unwrap();
        assert_ne!(digest, changed.digest().unwrap());
        changed = p.clone();
        changed.memories.push(MemoryRevisionReference {
            target_id: Uuid::new_v4(),
            revision: 1,
        });
        assert_ne!(digest, changed.digest().unwrap());
        changed.memories.push(changed.memories[0].clone());
        assert_eq!(changed.validate(), Err(AgentFailure::InvalidInput));
    }
    #[test]
    fn result_source_cannot_use_empty_prefix_and_diagnostics_hide_content() {
        let (mut p, _) = fixture();
        assert!(!format!("{p:?} {:?}", p.payload).contains("private original"));
        p.source = LiveProjectionSource::SurfaceResult {
            through: p.source.through(),
            surface_message_id: MessageId::new(),
        };
        assert_eq!(p.validate(), Err(AgentFailure::InvalidInput));
        p.source = LiveProjectionSource::SurfacePrefix {
            through: p.source.through(),
        };
        p.input_data_classes = vec![DataClass::Credential];
        assert_eq!(p.validate(), Err(AgentFailure::InvalidInput));
    }
}
