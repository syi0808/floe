//! Foundational values shared by Floe modules.
//!
//! This crate intentionally contains no business state or orchestration.  The
//! UUID wrappers are the canonical definitions; older crates re-export them
//! while they are being migrated.

/// The schema version every agent-facing contract in this workspace speaks.
pub const AGENT_VERSION: u32 = 1;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn from_uuid(value: Uuid) -> Option<Self> {
                (!value.is_nil()).then_some(Self(value))
            }

            pub fn as_uuid(self) -> Uuid {
                self.0
            }

            pub fn is_valid(self) -> bool {
                !self.0.is_nil()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

uuid_id!(PersonId);
uuid_id!(RunId);
uuid_id!(ScopeId);
uuid_id!(TaskId);
uuid_id!(CommandId);
uuid_id!(EventId);
uuid_id!(NoteId);
uuid_id!(CaptureId);

/// Correlation values that are safe to carry across asynchronous boundaries.
///
/// The context deliberately contains identifiers only.  Prompt text, model
/// output, credentials, and other request payloads must not be added here.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct TraceContext {
    request_id: Uuid,
    run_id: Option<RunId>,
    task_id: Option<TaskId>,
    attempt_id: Option<Uuid>,
}

impl TraceContext {
    pub fn new(request_id: Uuid) -> Self {
        Self {
            request_id,
            run_id: None,
            task_id: None,
            attempt_id: None,
        }
    }

    pub fn request_id(self) -> Uuid {
        self.request_id
    }

    pub fn run_id(self) -> Option<RunId> {
        self.run_id
    }

    pub fn task_id(self) -> Option<TaskId> {
        self.task_id
    }

    pub fn attempt_id(self) -> Option<Uuid> {
        self.attempt_id
    }

    pub fn with_run_id(mut self, run_id: RunId) -> Self {
        self.run_id = Some(run_id);
        self
    }

    pub fn with_task_id(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn with_attempt_id(mut self, attempt_id: Uuid) -> Self {
        self.attempt_id = Some(attempt_id);
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Revision(pub u64);

impl Revision {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFailure {
    UnsupportedVersion,
    InvalidInput,
    NotFound,
    Conflict,
    StorageUnavailable,
    VaultUnavailable,
    PolicyDenied,
    ConsentRequired,
    ModelUnavailable,
    LocalModelUnavailable,
    ServerModelUnavailable,
    ServerModelTimeout,
    ServerModelRequestRejected,
    CredentialExpired,
    QuotaExceeded,
    InvalidModelOutput,
    LocalModelInvalidOutput,
    ServerModelInvalidOutput,
    CapabilityDenied,
    CapabilityUnavailable,
    StaleContext,
    AccessReviewRequired,
    BudgetExceeded,
    Stalled,
    Cancelled,
    DeadlineExceeded,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFailureDomain {
    Source,
    Capability,
    Turn,
    Session,
    Vault,
    App,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFailureCategory {
    UserConfiguration,
    Transient,
    Integrity,
    Security,
    Internal,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFailureSafeAction {
    ContinueWithoutSource,
    ReviewSource,
    Retry,
    RefreshSession,
    StartNewSession,
    ReopenVault,
    ResetLocalAgentState,
    ExportDiagnostics,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentRetryPolicy {
    Never,
    Immediate,
    Backoff,
}
