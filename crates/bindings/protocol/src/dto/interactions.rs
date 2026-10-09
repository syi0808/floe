use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{CommandIdDto, DigestHex64Dto, InteractionRefDto, RunRefDto, SessionRefDto};

/// Maximum snapshots returned by one interaction list read.
pub const MAX_INTERACTIONS_PER_LIST: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppInteractionKindDto {
    SourceAccess,
    AssistantFeatureSources,
    OperationApproval,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppInteractionStateDto {
    Pending,
    Resolving,
    Resolved,
    Denied,
    Dismissed,
    Superseded,
    Expired,
    Stale,
    WrongDevice,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppSourceAccessReasonDto {
    EnableObserve,
    ReviewChangedSource,
    RequestSystemPermission,
    Reconnect,
    ReviewProcessing,
    SelectResource,
}

/// The projected requirement keeps source-access, assistant-feature, and operation
/// approvals distinct at the product boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppInteractionRequirementDto {
    SourceAccess {
        reason: AppSourceAccessReasonDto,
        source_id: String,
        connection_id: Option<String>,
        consumer: String,
        purpose: String,
        inline: bool,
    },
    AssistantFeatureSourceReview {
        source_requirement_ref: String,
        review_ref: super::AssistantFeatureSourceReviewRefDto,
    },
    OperationApproval {
        review_ref: super::ActionReviewRefDto,
    },
}

/// A source access card carries the safe stored review projection itself.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum AppInteractionTargetDto {
    #[serde(rename = "navigation_only")]
    NavigationOnly {
        destination: AppNavigationDestinationDto,
        source_label: String,
        source_ref: Option<super::ConnectionsSourceRefDto>,
    },
    #[serde(rename = "source_review")]
    SourceReview { review: super::ObserveReviewDto },
    #[serde(rename = "assistant_feature_source_review")]
    AssistantFeatureSourceReview {
        review: super::AssistantFeatureSourceReviewDto,
    },
    #[serde(rename = "operation_approval")]
    OperationApproval { operation: super::ActionSnapshotDto },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppInteractionActionDto {
    Allow,
    Deny,
    Dismiss,
    Refresh,
    OpenConnection,
    ReviewSource,
    RequestPermission,
    OpenAssistantFeatureSettings,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppInteractionSnapshotDto {
    pub interaction_id: InteractionRefDto,
    pub session_id: SessionRefDto,
    pub origin_run_id: RunRefDto,
    pub interaction_kind: AppInteractionKindDto,
    pub state: AppInteractionStateDto,
    pub revision: u64,
    pub target_digest: DigestHex64Dto,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub requirement: AppInteractionRequirementDto,
    pub target: AppInteractionTargetDto,
    pub actions: Vec<AppInteractionActionDto>,
}

impl AppInteractionSnapshotDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.revision == 0 || self.revision > i64::MAX as u64 {
            return Err("interaction.revision");
        }
        if self.expires_at <= self.created_at {
            return Err("interaction.expires_at");
        }
        match (&self.interaction_kind, &self.requirement, &self.target) {
            (
                AppInteractionKindDto::SourceAccess,
                AppInteractionRequirementDto::SourceAccess {
                    source_id,
                    connection_id,
                    consumer,
                    purpose,
                    ..
                },
                AppInteractionTargetDto::SourceReview { review },
            ) => {
                review.validate()?;
                validate_requirement_text(source_id, 128, "interaction.requirement.source_id")?;
                if let Some(connection_id) = connection_id {
                    validate_requirement_text(
                        connection_id,
                        256,
                        "interaction.requirement.connection_id",
                    )?;
                }
                validate_requirement_text(consumer, 256, "interaction.requirement.consumer")?;
                validate_requirement_text(purpose, 64, "interaction.requirement.purpose")?;
            }
            (
                AppInteractionKindDto::SourceAccess,
                AppInteractionRequirementDto::SourceAccess {
                    source_id,
                    connection_id,
                    consumer,
                    purpose,
                    ..
                },
                AppInteractionTargetDto::NavigationOnly { source_label, .. },
            ) if !source_label.is_empty() && source_label.len() <= 256 => {
                validate_requirement_text(source_id, 128, "interaction.requirement.source_id")?;
                if let Some(connection_id) = connection_id {
                    validate_requirement_text(
                        connection_id,
                        256,
                        "interaction.requirement.connection_id",
                    )?;
                }
                validate_requirement_text(consumer, 256, "interaction.requirement.consumer")?;
                validate_requirement_text(purpose, 64, "interaction.requirement.purpose")?;
            }
            (
                AppInteractionKindDto::AssistantFeatureSources,
                AppInteractionRequirementDto::AssistantFeatureSourceReview {
                    source_requirement_ref,
                    review_ref,
                },
                AppInteractionTargetDto::AssistantFeatureSourceReview { review },
            ) => {
                review.validate()?;
                review_ref.validate()?;
                validate_requirement_text(
                    source_requirement_ref,
                    128,
                    "interaction.requirement.source_requirement_ref",
                )?;
                if &review.review_ref != review_ref
                    || &review.source_requirement_ref != source_requirement_ref
                {
                    return Err("interaction.requirement.binding");
                }
            }
            (
                AppInteractionKindDto::OperationApproval,
                AppInteractionRequirementDto::OperationApproval { review_ref },
                AppInteractionTargetDto::OperationApproval { operation },
            ) => {
                operation.validate()?;
                review_ref.validate()?;
                if &operation.review_ref != review_ref {
                    return Err("interaction.requirement.operation");
                }
            }
            _ => return Err("interaction.target.kind"),
        }
        if self.actions.len() > 16
            || self
                .actions
                .iter()
                .enumerate()
                .any(|(index, action)| self.actions[index + 1..].contains(action))
        {
            return Err("interaction.actions");
        }
        Ok(())
    }
}

fn validate_requirement_text(
    value: &str,
    max_bytes: usize,
    field: &'static str,
) -> Result<(), &'static str> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(field)
    } else {
        Ok(())
    }
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
    Pending,
    Resolved,
    Resolving,
    Denied,
    Dismissed,
    Superseded,
    Expired,
    Stale,
    WrongDevice,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppInteractionRefreshOutcomeDto {
    Pending,
    Denied,
    Dismissed,
    Resolved,
    Resolving,
    Superseded,
    Expired,
    Stale,
    WrongDevice,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppInteractionResolveResultDto {
    pub command_id: CommandIdDto,
    pub outcome: AppInteractionResolveOutcomeDto,
    pub snapshot: AppInteractionSnapshotDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement_id: Option<InteractionRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_run: Option<super::AppCommandReceiptDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppInteractionRefreshResultDto {
    pub command_id: CommandIdDto,
    pub outcome: AppInteractionRefreshOutcomeDto,
    pub snapshot: AppInteractionSnapshotDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement_id: Option<InteractionRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_run: Option<super::AppCommandReceiptDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppInteractionListDto {
    pub session_id: SessionRefDto,
    pub interactions: Vec<AppInteractionSnapshotDto>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppNavigationDestinationDto {
    ConnectionSettings,
    SystemPermission,
    ResourcePicker,
}
