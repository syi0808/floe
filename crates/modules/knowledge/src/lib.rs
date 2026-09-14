pub mod api;
pub mod application {
    pub mod memory;
}

pub use api::{
    EpistemicStatus, KNOWLEDGE_VERSION, KnowledgeActor, LearningEvidenceSnapshot,
    LearningObservationKind, PersonalMemoryKind, PersonalMemoryValue, StageMemoryCandidate,
};
pub use application::memory::{validate_learning_evidence, validate_stage_request};
