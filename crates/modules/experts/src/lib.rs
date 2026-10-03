//! Expert directory, registration, shared Engine execution and
//! Task ownership.
//!
//! Nothing here decides what a specific Expert means. The common path carries an
//! agent identity and a role-neutral invocation.

mod api;
#[path = "application/service.rs"]
mod service;
#[path = "application/engine_endpoint.rs"]
mod engine_endpoint;
pub use engine_endpoint::EngineExpertEndpoint;
#[path = "application/binding.rs"]
mod binding;
pub use binding::{binding_review_digest, project_binding_mutation_receipt, project_binding_review,
    project_expert_directory, validate_binding_review_descriptor};
pub use service::{ExpertsDependencies, ExpertsService};
mod directory;
mod manifest;
mod registry;
mod selection;
mod settlement;
mod task;
#[path = "domain/task_record.rs"]
mod task_record;
#[path = "ports/task_repository.rs"]
mod task_repository;
mod program;
pub mod ports {
    pub mod binding_review;
    pub mod candidate_catalog;
    pub mod registry_repository;
    pub mod source;
}
pub use api::{BindingCandidateSummary, BindingInspection, BindingInspectionCandidate,
    BindingMutationReceipt, BindingReview, BindingReviewAction, ExpertAssignmentSummary,
    ExpertClock, ExpertDirectorySnapshot, ExpertInstallationSummary, ExpertRequirementSummary,
    ExpertsOwner, SystemExpertClock};
pub use ports::binding_review::{BindingPrepareIdentity, BindingReplacementReceipt,
    BindingReviewDescriptor, BindingReviewRef, BindingReviewRepository, ReviewedBindingReplacement,
    ReviewedCandidate};
pub use ports::candidate_catalog::{Candidate, CandidateAvailability, CandidateCatalog,
    CandidateQuery, CandidateSnapshot, CandidateSourceExpectation};
pub use ports::registry_repository::{RegistryCommit, RegistryCommitReceipt, RegistryRepository};
pub use program::{ExpertFinalOutput, ExpertProgram, ExpertProgramRequest, ExpertProgramSpec,
    ExpertSourceObservation, ExpertToolObservation, ExpertToolSpec};
pub use ports::source::{ExpertProjectionPort, ExpertProjectionRequest, ExpertSourcePort,
    ExpertSourceRead, ExpertSourceRequest};

pub use directory::{
    Directory, DirectoryEntry, DirectoryQuery, ExpertAdmissionIdentity,
    RunExpertEnvironmentIdentity,
};
/// What one Expert is asked to do and what it answers are contract values; what
/// this module adds is the registry that admits an invocation and records it.
pub use floe_agent_contract::{AgentCard, PackageKind, PackageRef};
pub use manifest::{
    ContractRef, EXPERT_MANIFEST_SCHEMA_VERSION, ExpertManifest, ExpertRegistration,
    ExpertSourceRequirement, MAX_REQUIREMENT_SOURCES, manifest_set_digest,
};
pub use registry::{
    AgentRegistry, AssignmentOverview, BindingOperationReceipt, EXPERT_BINDING_SCHEMA_VERSION,
    EXPERT_REGISTRY_SCHEMA_VERSION, ExpertBindingCommand, ExpertBindingState,
    ExpertInstallOperation, ExpertInstallReceipt, ExpertInstallResult, ExpertPrivateState,
    InstalledExpert, PackageAssignment, PackageInstallation, RegistryConfiguration,
    RegistryConfigurationTarget, RegistryOverview, RegistrySnapshot, RequirementBinding,
    ResolvedExpert,
};
pub use selection::{
    AdmittedRequirementSelection, EXPERT_EXECUTION_SELECTION_SCHEMA_VERSION,
    ExpertExecutionSelection,
};
pub use settlement::{ExpertSettlement, ExpertTaskCompletion, prepare_expert_completion};
pub use task::{RunExpertEnvironment, TaskCoordinator};
pub use task_record::{TaskRecord, TaskArtifactEvidence, settle_task_execution,
    interrupt_task_execution, validate_task_artifact, advance_task_journal, validate_task_journal,
    MAX_TASK_RECORD_BYTES, MAX_TASK_TERMINAL_RESERVE_BYTES};
pub use task_repository::{TaskActivation, TaskAdmission, TaskExecutionCommit, TaskRepository};

/// Role-neutral inference consumer for admitted delegated Experts.
pub const DELEGATED_EXPERT_INFERENCE_CONSUMER: &str = "experts.delegated";
