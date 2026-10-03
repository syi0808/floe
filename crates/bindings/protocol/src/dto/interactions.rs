use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{
    AssignmentRefDto, CommandIdDto, DigestHex64Dto, InteractionRefDto, RunRefDto, SessionRefDto,
};

/// Maximum snapshots returned by one interaction list read.
pub const MAX_INTERACTIONS_PER_LIST: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppInteractionKindDto {
    SourceAccess,
    ExpertBinding,
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

/// A source access card carries the safe stored review projection itself.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum AppInteractionTargetDto {
    #[serde(rename = "navigation_only")]
    NavigationOnly {
        destination: AppNavigationDestinationDto,
        source_label: String,
    },
    #[serde(rename = "source_review")]
    SourceReview { review: super::ObserveReviewDto },
    #[serde(rename = "expert_binding")]
    ExpertBinding { review: super::BindingReviewDto },
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
    OpenExpertSettings,
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
        match (&self.interaction_kind, &self.target) {
            (
                AppInteractionKindDto::SourceAccess,
                AppInteractionTargetDto::SourceReview { review },
            ) => {
                review.validate()?;
            }
            (
                AppInteractionKindDto::SourceAccess,
                AppInteractionTargetDto::NavigationOnly { source_label, .. },
            ) if !source_label.is_empty() && source_label.len() <= 256 => {}
            (
                AppInteractionKindDto::ExpertBinding,
                AppInteractionTargetDto::ExpertBinding { review },
            ) => {
                review.validate()?;
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
