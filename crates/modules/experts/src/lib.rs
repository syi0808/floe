//! Expert directory, registration, endpoint dispatch, A2A Task transport and
//! Task ownership.
//!
//! Nothing here decides what a specific Expert means. The common path carries an
//! agent identity and a role-neutral invocation.

mod a2a;
mod directory;
mod dispatch;
mod invocation;
mod registry;
mod task;

pub use a2a::{
    A2A_PROTOCOL_VERSION, A2AArtifact, A2AHost, A2AMessage, A2AMessageRole, A2APart, A2ARouter,
    A2ASendMessageRequest, A2ATask, A2ATaskRequest, A2ATaskState, AgentCard,
    EXPERT_RESULT_MEDIA_TYPE, InProcessA2ATransport, InProcessAgent, NoA2AHost,
};
pub use directory::{Directory, DirectoryEntry, DirectoryQuery};
pub use dispatch::{
    ExpertDispatchTable, ExpertRun, TaskCoverageRecorder, admit_expert_message,
    completed_expert_task, delegate_expert_task, expert_report, record_task_coverage,
    task_receipt_to_a2a,
};
pub use invocation::{
    ExpertBudget, ExpertFocusProposal, ExpertInput, ExpertInsight, ExpertInvocation, ExpertResult,
    ViewCancellation, check_running,
};
pub use registry::{
    AgentId, AgentPackage, AgentRegistry, AssignmentOverview, BuiltinExpertAssignmentReceipt,
    BuiltinExpertSetup, BuiltinExpertSetupReceipt, BuiltinExpertSetupResult, BuiltinSourceBinding,
    BuiltinSourceState, CalendarAccessChange, CalendarAccessConfiguration, CalendarExpertOverview,
    CalendarExpertSetup, CalendarExpertSetupReceipt, CalendarExpertSetupResult, CalendarViewBinding,
    ExpertMetadata, ExpertPackaging, ExpertPrivateState, ExpertRule, ExpertSetupSpec,
    NoSetupValidator, PackageAssignment, PackageImplementation, PackageInstallation, PackageKind,
    PackageRef, RegistryConfiguration, RegistryConfigurationTarget, RegistryOverview,
    RegistrySnapshot, SetupValidator, SourceGrant,
};
pub use task::{TaskActivation, TaskAdmission, TaskCoordinator, TaskRecord, TaskRepository};
