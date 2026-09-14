pub mod api;
pub mod prompts;
pub mod ports {
    pub mod evidence;
    pub mod repository;
}
pub use application::learner::validate_learner_input;
pub use ports::evidence::EvidenceReader;
pub use ports::repository::{LearnerJobRepository, MemoryContextReader};
pub use application::learner_service::LearnerService;
pub use application::learner_scheduling::{LearnerLease, LearnerScheduling};
pub mod application {
    pub mod inference;
    pub mod learner;
    pub mod learner_service;
    pub mod learner_scheduling;
    pub mod memory;
    pub mod playbooks;
    pub mod review;
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
    LEARNER_JOB_LEASE_SECONDS, LEARNER_JOB_RETRY_DELAY_SECONDS, LearnerBudget, LearnerJobClaim,
    LearnerJobLifecycle, LearnerJobSettlement, LearnerJobState, LearnerMemoryProposal,
    LearnerModel, LearnerModelRequest, LearnerReviewInput, LearnerReviewJob, LearnerReviewOutput,
    LearnerRuntime, MAX_LEARNER_JOB_ATTEMPTS, MemoryCandidateSink, claim_learner_job,
    explicit_learning_signal, reject_learner_claim, retryable_learner_failure, settle_learner_job,
    settlement_for_learner_result, validate_learner_job_lifecycle,
};
pub use application::inference::{
    InferenceLearnerModel, LearnerInferenceResponse, LearnerInferenceTransport,
    LEARNER_INFERENCE_CAPABILITY, LEARNER_INFERENCE_CONSUMER, LEARNER_INFERENCE_PURPOSE,
};
pub use application::memory::{
    acquire_memory_context, admit_learning_evidence, project_memory_summary,
    validate_learning_evidence, validate_memory_overview_limit, validate_stage_request,
};
pub use application::playbooks::{
    LoadedPlaybook, MAX_LOADED_PLAYBOOK_BYTES, MAX_LOADED_PLAYBOOKS, MAX_PLAYBOOK_DEPTH,
    MAX_VISIBLE_PLAYBOOKS, Playbook, PlaybookAudience, PlaybookBody, PlaybookChild,
    PlaybookIndexEntry, PlaybookRef, PlaybookRegistry, PlaybookSession,
};
pub use application::review::{
    ReviewAdmission, ReviewPlan, plan_review, validate_approval_candidate,
    validate_memory_review_candidate, validate_review_actor, validate_review_candidate,
};
