use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::day::TimedScheduleDto;
use super::{
    ActionAuthorityModeDto, ActionRefDto, DigestHex64Dto, TaskRefDto, UuidRefDto,
};

const MAX_ACTION_TITLE_BYTES: usize = 1_024;
const MAX_DESTINATION_LABEL_BYTES: usize = 512;
const MAX_ACTION_PAGE_ITEMS: usize = 100;
const MAX_ACTION_DESTINATIONS: usize = 256;
const MAX_NEXT_OBSERVATION_AFTER_MS: u64 = 60_000;
const MAX_TASK_JOURNAL_REVISION: u64 = 512;
const MAX_ACTION_REVISION: u64 = i64::MAX as u64;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskExecutionKeyDto {
    pub task_id: TaskRefDto,
    pub execution_id: UuidRefDto,
    pub executor_generation: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskExecutionReceiptRefDto {
    pub execution: TaskExecutionKeyDto,
    pub task_revision: u64,
    pub journal_revision: u64,
    pub digest: DigestHex64Dto,
}

impl TaskExecutionReceiptRefDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.execution.executor_generation == 0
            || self.task_revision < 2
            || self.journal_revision > MAX_TASK_JOURNAL_REVISION
            || self.digest.as_str() == "0000000000000000000000000000000000000000000000000000000000000000"
        {
            return Err("actions.task_execution_receipt_ref");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionIntentDto {
    DirectCreate {
        destination_ref: UuidRefDto,
        title: String,
        schedule: TimedScheduleDto,
    },
    DirectUpdate {
        event_ref: UuidRefDto,
        expected_revision: u64,
        title: String,
        schedule: TimedScheduleDto,
    },
    DirectDelete {
        event_ref: UuidRefDto,
        expected_revision: u64,
    },
    ExpertProposal {
        receipt: TaskExecutionReceiptRefDto,
        artifact_id: UuidRefDto,
        destination_ref: UuidRefDto,
        timezone: String,
    },
}

impl ActionIntentDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::DirectCreate { title, schedule, .. } => {
                validate_new_title(title)?;
                schedule.validate_new_action()
            }
            Self::DirectUpdate {
                expected_revision,
                title,
                schedule,
                ..
            } => {
                positive_revision(*expected_revision, "actions.expected_revision")?;
                validate_new_title(title)?;
                schedule.validate_new_action()
            }
            Self::DirectDelete {
                expected_revision, ..
            } => positive_revision(*expected_revision, "actions.expected_revision"),
            Self::ExpertProposal {
                receipt, timezone, ..
            } => {
                receipt.validate()?;
                if !bounded_trimmed_text(timezone, 128) {
                    return Err("actions.timezone");
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDestinationChoiceDto {
    pub destination_ref: UuidRefDto,
    pub label: String,
}

impl ActionDestinationChoiceDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !bounded_trimmed_text(&self.label, MAX_DESTINATION_LABEL_BYTES) {
            return Err("actions.destination.label");
        }
        Ok(())
    }
}

/// Validates the bounded owner-projected destination list without interpreting
/// its opaque compare-only references as authority.
pub fn validate_action_destination_choices(
    choices: &[ActionDestinationChoiceDto],
) -> Result<(), &'static str> {
    if choices.len() > MAX_ACTION_DESTINATIONS {
        return Err("actions.destinations");
    }
    let mut references = HashSet::with_capacity(choices.len());
    for choice in choices {
        choice.validate()?;
        if !references.insert(choice.destination_ref) {
            return Err("actions.destinations.duplicate");
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionsAuthorityDto {
    pub revision: u64,
    pub calendar_create: ActionAuthorityModeDto,
}

impl ActionsAuthorityDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        positive_revision(self.revision, "actions.authority.revision")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionReviewRefDto {
    pub id: UuidRefDto,
    pub action_id: ActionRefDto,
    pub effect_digest: DigestHex64Dto,
    pub source_digest: DigestHex64Dto,
    pub authority_revision: u64,
    pub expires_at: String,
}

impl ActionReviewRefDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        positive_revision(self.authority_revision, "actions.review_ref.authority_revision")?;
        parse_instant(&self.expires_at, "actions.review_ref.expires_at")?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionEffectSummaryDto {
    Create {
        destination_label: String,
        title: String,
        schedule: TimedScheduleDto,
    },
    Update {
        event_ref: UuidRefDto,
        expected_revision: u64,
        destination_label: String,
        previous_title: String,
        previous_schedule: TimedScheduleDto,
        title: String,
        schedule: TimedScheduleDto,
    },
    Delete {
        event_ref: UuidRefDto,
        expected_revision: u64,
        destination_label: String,
        title: String,
        schedule: TimedScheduleDto,
    },
}

impl ActionEffectSummaryDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Create {
                destination_label,
                title,
                schedule,
            } => {
                validate_destination_label(destination_label)?;
                validate_new_title(title)?;
                schedule.validate_new_action()
            }
            Self::Update {
                expected_revision,
                destination_label,
                previous_title,
                previous_schedule,
                title,
                schedule,
                ..
            } => {
                positive_revision(*expected_revision, "actions.effect.expected_revision")?;
                validate_destination_label(destination_label)?;
                validate_observed_title(previous_title)?;
                previous_schedule.validate_historical()?;
                validate_new_title(title)?;
                schedule.validate_new_action()
            }
            Self::Delete {
                expected_revision,
                destination_label,
                title,
                schedule,
                ..
            } => {
                positive_revision(*expected_revision, "actions.effect.expected_revision")?;
                validate_destination_label(destination_label)?;
                validate_observed_title(title)?;
                schedule.validate_historical()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionAllowedActionDto {
    Approve,
    Reject,
    Cancel,
    Reconcile,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionDecisionKindDto {
    Approve,
    Reject,
    Cancel,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionBlockedReasonDto {
    PermissionDenied,
    PolicyDenied,
    SourceChanged,
    ExecutorUnavailable,
    ScheduleConflict,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionNotAppliedReasonDto {
    SourceChanged,
    Cancelled,
    Timeout,
    PermissionDenied,
    ProviderRejected,
    ProviderUnavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionUnknownReasonDto {
    Timeout,
    ResponseLost,
    InvalidReceipt,
    CancelledAfterDispatch,
    InconclusiveLookup,
    NativeOperationPending,
    NativeReceiptUnavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCollectionStatusDto {
    Pending,
    Collected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionOriginKindDto {
    Direct,
    Expert,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionStatusDto {
    PendingReview,
    Approved,
    Rejected,
    Cancelled,
    Expired,
    Executing,
    Blocked { reason: ActionBlockedReasonDto },
    Failed { reason: ActionNotAppliedReasonDto },
    Unknown { reason: ActionUnknownReasonDto },
    Succeeded { collection: ActionCollectionStatusDto },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSnapshotDto {
    pub action_ref: ActionRefDto,
    pub revision: u64,
    pub origin: ActionOriginKindDto,
    pub effect: ActionEffectSummaryDto,
    pub review_ref: ActionReviewRefDto,
    pub created_at: String,
    pub expires_at: String,
    pub status: ActionStatusDto,
    pub allowed_actions: Vec<ActionAllowedActionDto>,
    pub next_observation_after_ms: Option<u64>,
}

impl ActionSnapshotDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        positive_revision(self.revision, "actions.snapshot.revision")?;
        self.effect.validate()?;
        self.review_ref.validate()?;
        if self.review_ref.action_id != self.action_ref {
            return Err("actions.snapshot.review_ref.action_id");
        }
        let created_at = parse_instant(&self.created_at, "actions.snapshot.created_at")?;
        let expires_at = parse_instant(&self.expires_at, "actions.snapshot.expires_at")?;
        let review_expires_at = parse_instant(
            &self.review_ref.expires_at,
            "actions.review_ref.expires_at",
        )?;
        if expires_at <= created_at || expires_at != review_expires_at {
            return Err("actions.snapshot.expires_at");
        }
        if self.allowed_actions.len() > 4 {
            return Err("actions.snapshot.allowed_actions");
        }
        for (index, action) in self.allowed_actions.iter().enumerate() {
            if self.allowed_actions[..index].contains(action) {
                return Err("actions.snapshot.allowed_actions.duplicate");
            }
        }
        if self
            .next_observation_after_ms
            .is_some_and(|value| value == 0 || value > MAX_NEXT_OBSERVATION_AFTER_MS)
        {
            return Err("actions.snapshot.next_observation_after_ms");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionsPageDto {
    pub actions: Vec<ActionSnapshotDto>,
    pub next_cursor: Option<ActionRefDto>,
}

impl ActionsPageDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.actions.len() > MAX_ACTION_PAGE_ITEMS {
            return Err("actions.page.actions");
        }
        let mut references = HashSet::with_capacity(self.actions.len());
        for action in &self.actions {
            action.validate()?;
            if !references.insert(action.action_ref) {
                return Err("actions.page.actions.duplicate");
            }
        }
        Ok(())
    }
}

fn validate_destination_label(value: &str) -> Result<(), &'static str> {
    if bounded_trimmed_text(value, MAX_DESTINATION_LABEL_BYTES) {
        Ok(())
    } else {
        Err("actions.effect.destination_label")
    }
}

fn validate_new_title(value: &str) -> Result<(), &'static str> {
    if bounded_trimmed_text(value, MAX_ACTION_TITLE_BYTES) {
        Ok(())
    } else {
        Err("actions.title")
    }
}

fn validate_observed_title(value: &str) -> Result<(), &'static str> {
    // Historical Calendar observations have a separate owner bound.
    if value.len() <= 4096 {
        Ok(())
    } else {
        Err("actions.observed_title")
    }
}

fn bounded_trimmed_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn positive_revision(value: u64, field: &'static str) -> Result<(), &'static str> {
    if value == 0 || value > MAX_ACTION_REVISION {
        Err(field)
    } else {
        Ok(())
    }
}

fn parse_instant(
    value: &str,
    field: &'static str,
) -> Result<chrono::DateTime<chrono::FixedOffset>, &'static str> {
    if value.is_empty() || value.len() > 64 {
        return Err(field);
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .map_err(|_| field)
        .and_then(|instant| {
            if instant.offset().local_minus_utc() == 0 {
                Ok(instant)
            } else {
                Err(field)
            }
        })
}
