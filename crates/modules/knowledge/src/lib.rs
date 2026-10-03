pub mod api;
pub mod prompts;
pub mod ports {
    pub mod evidence;
    pub mod learner_projection;
    pub mod learner_repository;
    pub mod knowledge_repository;
}
pub use application::learner::validate_learner_input;
pub use application::learner_scheduling::{KnowledgeForegroundLease, LearnerLease, LearnerScheduling};
pub use application::learner_service::LearnerService;
pub use application::service::{KnowledgeClock, KnowledgeDependencies, KnowledgeOwner,
    KnowledgeRead, KnowledgeService, SystemKnowledgeClock};
pub use ports::evidence::EvidenceReader;
pub use ports::learner_projection::{LearnerClaimRef, LearnerEvidenceRepository,
    LearnerProjectionBounds, LearnerProjectionPort, LearnerProjectionRequest};
pub use ports::learner_repository::{LearnerJobRepository, LearnerJournalFactory,
    LearningSessionSnapshot, LearningTranscriptMessage};
pub use ports::knowledge_repository::{KnowledgeRepository, MemoryDecisionRequest,
    MemoryStageOrigin, MemoryStageRequest};
pub mod application {
    pub mod discovery;
    pub mod learner;
    pub mod learner_scheduling;
    pub mod learner_service;
    pub mod memory;
    pub mod playbooks;
    pub mod review;
    pub mod service;
    pub mod storage_policy;
    pub mod learner_journal;
}

pub use api::{
    ContextMemory, EpistemicStatus, EvidenceProjectionPurpose, KNOWLEDGE_VERSION, KnowledgeActor,
    KnowledgeCandidate, KnowledgeCandidateState, KnowledgeDecision, KnowledgeDecisionKind,
    KnowledgeDecisionResult, KnowledgeKind, KnowledgeMutation, KnowledgeOperation,
    KnowledgePayload, KnowledgeRevision, KnowledgeRevisionState, LearningEvidenceRef,
    LearningEvidenceSnapshot, LearningObservation, LearningObservationKind, LearningOutcome,
    MAX_CONTEXT_MEMORIES, MAX_CONTEXT_MEMORY_BYTES, MAX_MEMORY_OVERVIEW_ITEMS,
    MemoryContextSnapshot, MemoryOrigin, MemoryOverviewSnapshot, MemoryReviewSnapshot,
    MemorySummary, PersonalMemoryKind, PersonalMemoryValue, StageMemoryCandidate,
};
pub use application::learner::{
    LEARNER_INFERENCE_CONSUMER, LEARNER_INFERENCE_PURPOSE, LEARNER_JOB_LEASE_SECONDS,
    LEARNER_JOB_RETRY_DELAY_SECONDS, LearnerBudget, LearnerJobClaim, LearnerJobLifecycle,
    LearnerJobSettlement, LearnerJobState, LearnerMemoryProposal, LearnerProjectionBlock,
    LearnerReviewInput, LearnerReviewJob,
    MAX_LEARNER_JOB_ATTEMPTS, claim_learner_job, explicit_learning_signal,
    parse_learner_review_output, reject_learner_claim, retryable_learner_failure,
    settle_learner_job, settlement_for_learner_result, validate_learner_job_lifecycle,
};
pub use application::discovery::explicit_review_input;
pub use application::memory::{
    admit_learning_evidence, project_memory_summary, validate_learning_evidence,
    validate_memory_overview_limit, validate_stage_request,
};
pub use application::playbooks::{
    LoadedPlaybook, MAX_LOADED_PLAYBOOK_BYTES, MAX_LOADED_PLAYBOOKS, MAX_PLAYBOOK_DEPTH,
    MAX_VISIBLE_PLAYBOOKS, Playbook, PlaybookAudience, PlaybookBody, PlaybookChild,
    PlaybookIndexEntry, PlaybookRef, PlaybookRegistry, PlaybookSession,
};
pub use application::review::{
    ReviewAdmission, ReviewPlan, plan_review, validate_approval_candidate, validate_memory_review_candidate,
    validate_review_actor, validate_review_candidate,
};

pub use application::storage_policy::{MemoryStageIdentity, MemoryStagePlan, MemoryContextFact,
    knowledge_content_hash, memory_stage_identity, plan_memory_stage, project_memory_context,
    learner_job_key, new_learner_job};
pub use application::learner_journal::{LearnerClaimJournal, LearnerJournalHead,
    advance_learner_journal, validate_learner_journal, validate_learner_budget, recover_learner_claim, validate_learner_stage};
