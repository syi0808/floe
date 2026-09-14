//! Foundational values shared by Floe modules.
//!
//! This crate intentionally contains no business state or orchestration.  The
//! UUID wrappers are the canonical definitions; older crates re-export them
//! while they are being migrated.

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
uuid_id!(TaskId);
uuid_id!(CommandId);
uuid_id!(EventId);
uuid_id!(NoteId);
uuid_id!(CaptureId);

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
