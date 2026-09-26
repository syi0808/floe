use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Maximum snapshots returned by one interaction list read.
pub const MAX_INTERACTIONS_PER_LIST: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppInteractionKindDto {
    SourceAccess,
    ProcessingRecipient,
    ExpertBinding,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppInteractionStateDto {
    Pending,
    Resolving,
    Resolved,
    Denied,
    Cancelled,
    Superseded,
    Expired,
}

/// One safe reviewed-target projection: only the user-facing identity the
/// person needs for informed review. Fingerprints, revisions, policy
/// authorities, lineage, devices and projection audits never cross the wire.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppInteractionTargetDto {
    InlineObserve {
        connection_id: String,
        source_id: String,
        consumer: String,
        purpose: String,
        members: Vec<AppObservedMemberDto>,
    },
    NavigationOnly {
        destination: AppNavigationDestinationDto,
        source_id: String,
        consumer: String,
        purpose: String,
    },
    RecipientConsent {
        recipient: String,
        profile_id: String,
        purpose: String,
        consumer: String,
        input_data_classes: Vec<String>,
        source_scopes: Vec<AppConsentScopeDto>,
    },
    ExpertBinding {
        assignment_id: Uuid,
        package_id: String,
        package_version: String,
        requirement_key: String,
        capability: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppObservedMemberDto {
    pub member_id: String,
    pub resource: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppNavigationDestinationDto {
    ConnectionSettings,
    SystemPermission,
    ResourcePicker,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppConsentScopeDto {
    pub connection_id: String,
    pub resources: Vec<String>,
    pub categories: Vec<String>,
    pub operation: String,
    pub purpose: String,
    pub consumer: String,
}

/// Actions the backend projection offers for one snapshot. Flutter renders
/// only these; it never derives its own actions from state or target shape.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppInteractionActionDto {
    Allow,
    Deny,
    Dismiss,
    Refresh,
    ContinueRequest,
    OpenConnection,
    ReviewSource,
    RequestPermission,
    OpenExpertSettings,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppInteractionSnapshotDto {
    pub interaction_id: Uuid,
    pub session_id: Uuid,
    pub origin_run_id: Uuid,
    pub interaction_kind: AppInteractionKindDto,
    pub state: AppInteractionStateDto,
    pub revision: u64,
    pub target_digest: [u8; 32],
    pub created_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub target: AppInteractionTargetDto,
    pub actions: Vec<AppInteractionActionDto>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppInteractionDecisionDto {
    Approve,
    Deny,
    Dismiss,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppInteractionResolveOutcomeDto {
    Resolved,
    Resolving,
    Denied,
    Cancelled,
    Superseded,
    Expired,
    Stale,
    Terminal,
    WrongDevice,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppInteractionRefreshOutcomeDto {
    Resolved,
    StillPending,
    Superseded,
    Terminal,
    Expired,
    Stale,
    WrongDevice,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppInteractionResolveResultDto {
    pub command_id: Uuid,
    pub outcome: AppInteractionResolveOutcomeDto,
    pub snapshot: AppInteractionSnapshotDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_run: Option<super::AppCommandReceiptDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppInteractionRefreshResultDto {
    pub command_id: Uuid,
    pub outcome: AppInteractionRefreshOutcomeDto,
    pub snapshot: AppInteractionSnapshotDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_run: Option<super::AppCommandReceiptDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppInteractionListDto {
    pub session_id: Uuid,
    pub interactions: Vec<AppInteractionSnapshotDto>,
}
