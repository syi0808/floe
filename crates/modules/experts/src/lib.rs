//! Expert directory, registration, endpoint dispatch, A2A Task transport and
//! Task ownership.
//!
//! Nothing here decides what a specific Expert means. The common path carries an
//! agent identity and a role-neutral invocation.

mod a2a;
mod bundle_install;
mod directory;
mod dispatch;
mod manifest;
mod registry;
mod selection;
mod settlement;
mod task;

pub use a2a::{
    A2A_PROTOCOL_VERSION, A2AArtifact, A2AHost, A2AMessage, A2AMessageRole, A2APart, A2ARouter,
    A2ASendMessageRequest, A2ATask, A2ATaskRequest, A2ATaskState, AgentCard, InProcessA2ATransport,
    InProcessAgent, NoA2AHost,
};
pub use bundle_install::{
    BoxFuture, ExpertInstallRefresh, ExpertInstallStore, ExpertRefreshOutcome,
    ensure_expert_bundle, expert_refresh_outcome,
};
pub use directory::{
    Directory, DirectoryEntry, DirectoryQuery, ExpertAdmissionIdentity, ResolvedDirectoryEntry,
};
pub use dispatch::{
    ExpertDispatchTable, ExpertRun, TaskCoverageRecorder, admit_expert_message,
    completed_expert_task, record_task_coverage, task_receipt_to_a2a,
};
/// What one Expert is asked to do and what it answers are contract values; what
/// this module adds is the registry that admits an invocation and records it.
pub use floe_agent_contract::{ExpertBudget, MAX_EXPERT_VIEW_BYTES, PackageKind, PackageRef};
pub use manifest::{
    ContractRef, EXPERT_MANIFEST_SCHEMA_VERSION, ExpertManifest, ExpertRegistration,
    ExpertSourceRequirement, MAX_REQUIREMENT_SOURCES, manifest_set_digest,
};
pub use registry::{
    AgentRegistry, AssignmentOverview, BindingOperationReceipt, EXPERT_BINDING_SCHEMA_VERSION,
    EXPERT_REGISTRY_SCHEMA_VERSION, ExpertBindingCommand, ExpertBindingState,
    ExpertInstallOperation,
    ExpertInstallReceipt, ExpertInstallResult, ExpertPrivateState, InstalledExpert,
    PackageAssignment, PackageInstallation, RegistryConfiguration, RegistryConfigurationTarget,
    RegistryOverview, RegistrySnapshot, RequirementBinding, ResolvedExpert,
    eligible_cards_for_availability,
};
pub use settlement::{ExpertSettlement, ExpertTaskCompletion};
pub use selection::{AdmittedRequirementSelection, EXPERT_EXECUTION_SELECTION_SCHEMA_VERSION, ExpertExecutionSelection};
pub use task::{TaskActivation, TaskAdmission, TaskCoordinator, TaskRecord, TaskRepository};
