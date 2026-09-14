use crate::AgentOutcome;

pub use floe_knowledge::{
    ContextMemory, EpistemicStatus, KNOWLEDGE_VERSION, KnowledgeActor, KnowledgeCandidate,
    KnowledgeCandidateState, KnowledgeDecision, KnowledgeDecisionKind, KnowledgeDecisionResult,
    KnowledgeKind, KnowledgeMutation, KnowledgeOperation, KnowledgePayload, KnowledgeRevision,
    KnowledgeRevisionState, LearningEvidenceRef, LearningObservation, LearningObservationKind,
    LearningOutcome, PersonalMemoryKind, PersonalMemoryValue, StageMemoryCandidate,
};

impl From<AgentOutcome> for LearningOutcome {
    fn from(outcome: AgentOutcome) -> Self {
        match outcome {
            AgentOutcome::Completed => Self::Completed,
            AgentOutcome::Halted { reason } => Self::Halted { reason },
        }
    }
}
