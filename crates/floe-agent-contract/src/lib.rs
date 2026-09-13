use serde::{Deserialize, Serialize};

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
#[serde(rename_all = "snake_case")]
pub enum AgentRetryPolicy {
    Never,
    Immediate,
    Backoff,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClass {
    Synthetic,
    Personal,
    TemporaryAiContext,
    HighlySensitive,
    DeviceOnlyRaw,
    Credential,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelPlacement {
    DeviceLocal,
    Remote,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionProtection {
    SyntheticOnly,
    Encrypted,
    KeyUnavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferConsent {
    NotGranted,
    Granted,
}
