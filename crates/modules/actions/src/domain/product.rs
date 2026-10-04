use chrono::{DateTime, Utc};
use floe_agent_contract::TaskExecutionReceiptRef;
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::record::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionIntent {
    DirectCreate {
        destination_ref: Uuid,
        title: String,
        schedule: floe_day::TimedSchedule,
    },
    DirectUpdate {
        event_ref: floe_day::EventId,
        expected_revision: floe_day::Revision,
        title: String,
        schedule: floe_day::TimedSchedule,
    },
    DirectDelete {
        event_ref: floe_day::EventId,
        expected_revision: floe_day::Revision,
    },
    ExpertProposal {
        receipt: TaskExecutionReceiptRef,
        artifact_id: Uuid,
        destination_ref: Uuid,
    },
}

impl ActionIntent {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        let content = |title: &str, schedule: &floe_day::TimedSchedule| {
            if !super::record::bounded(title, 1024)
                || !super::record::bounded(&schedule.timezone, 128)
                || schedule.ends_at <= schedule.starts_at
                || schedule.ends_at - schedule.starts_at > chrono::Duration::hours(24)
            {
                Err(AgentFailure::InvalidInput)
            } else {
                Ok(())
            }
        };
        match self {
            Self::DirectCreate {
                destination_ref,
                title,
                schedule,
            } => {
                if destination_ref.is_nil() {
                    return Err(AgentFailure::InvalidInput);
                }
                content(title, schedule)
            }
            Self::DirectUpdate {
                event_ref,
                expected_revision,
                title,
                schedule,
            } => {
                if !event_ref.is_valid() || expected_revision.0 == 0 {
                    return Err(AgentFailure::InvalidInput);
                }
                content(title, schedule)
            }
            Self::DirectDelete {
                event_ref,
                expected_revision,
            } => {
                if !event_ref.is_valid() || expected_revision.0 == 0 {
                    Err(AgentFailure::InvalidInput)
                } else {
                    Ok(())
                }
            }
            Self::ExpertProposal {
                receipt,
                artifact_id,
                destination_ref,
            } => {
                receipt.validate()?;
                if artifact_id.is_nil() || destination_ref.is_nil() {
                    return Err(AgentFailure::InvalidInput);
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDestinationChoice {
    pub destination_ref: Uuid,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionProposalPreview {
    Ready {
        title: String,
        schedule: floe_day::TimedSchedule,
        destinations: Vec<ActionDestinationChoice>,
    },
    Existing {
        action: ActionSnapshot,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionEffectSummary {
    Create {
        destination_label: String,
        title: String,
        schedule: floe_day::TimedSchedule,
    },
    Update {
        event_ref: floe_day::EventId,
        expected_revision: floe_day::Revision,
        destination_label: String,
        previous_title: String,
        previous_schedule: floe_day::TimedSchedule,
        title: String,
        schedule: floe_day::TimedSchedule,
    },
    Delete {
        event_ref: floe_day::EventId,
        expected_revision: floe_day::Revision,
        destination_label: String,
        title: String,
        schedule: floe_day::TimedSchedule,
    },
}

impl ActionEffectSummary {
    pub fn from_effect(effect: &CalendarEffect) -> Result<Self, AgentFailure> {
        let destination_label = effect.destination().calendar_name.clone();
        Ok(match effect {
            CalendarEffect::Create {
                title, schedule, ..
            } => Self::Create {
                destination_label,
                title: title.clone(),
                schedule: schedule.clone(),
            },
            CalendarEffect::Update {
                target,
                title,
                schedule,
                ..
            } => {
                let floe_day::EventSchedule::Timed(previous_schedule) = &target.original.schedule
                else {
                    return Err(AgentFailure::InvalidInput);
                };
                Self::Update {
                    event_ref: target.original.id,
                    expected_revision: target.original.revision,
                    destination_label,
                    previous_title: target.original.title.clone(),
                    previous_schedule: previous_schedule.clone(),
                    title: title.clone(),
                    schedule: schedule.clone(),
                }
            }
            CalendarEffect::Delete { target, .. } => {
                let floe_day::EventSchedule::Timed(schedule) = &target.original.schedule else {
                    return Err(AgentFailure::InvalidInput);
                };
                Self::Delete {
                    event_ref: target.original.id,
                    expected_revision: target.original.revision,
                    destination_label,
                    title: target.original.title.clone(),
                    schedule: schedule.clone(),
                }
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionAllowedAction {
    Approve,
    Reject,
    Cancel,
    Reconcile,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionStatus {
    PendingReview,
    Approved,
    Rejected,
    Cancelled,
    Expired,
    Executing,
    Blocked { reason: ActionBlockedReason },
    Failed { reason: ActionNotAppliedReason },
    Unknown { reason: ActionUnknownReason },
    Succeeded { collection: ActionCollectionStatus },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCollectionStatus {
    Pending,
    Collected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionOriginKind {
    Direct,
    Expert,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSnapshot {
    pub action_ref: Uuid,
    pub revision: u64,
    pub origin: ActionOriginKind,
    pub effect: ActionEffectSummary,
    pub review_ref: ActionReviewRef,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: ActionStatus,
    pub allowed_actions: Vec<ActionAllowedAction>,
    pub next_observation_after_ms: Option<u64>,
}

impl ActionSnapshot {
    pub fn from_record(record: &ActionRecord, now: DateTime<Utc>) -> Result<Self, AgentFailure> {
        record.validate()?;
        let mut allowed_actions = Vec::new();
        let status = match &record.state {
            ActionState::PendingReview | ActionState::Approved if now >= record.expires_at => {
                ActionStatus::Expired
            }
            ActionState::PendingReview => {
                if now >= record.created_at && now < record.expires_at {
                    allowed_actions.extend([
                        ActionAllowedAction::Approve,
                        ActionAllowedAction::Reject,
                        ActionAllowedAction::Cancel,
                    ]);
                }
                ActionStatus::PendingReview
            }
            ActionState::Approved => {
                if now < record.expires_at {
                    allowed_actions.push(ActionAllowedAction::Cancel);
                }
                ActionStatus::Approved
            }
            ActionState::Rejected => ActionStatus::Rejected,
            ActionState::Cancelled => ActionStatus::Cancelled,
            ActionState::Expired => ActionStatus::Expired,
            ActionState::Executing { .. } => {
                allowed_actions.push(ActionAllowedAction::Reconcile);
                ActionStatus::Executing
            }
            ActionState::Blocked { reason } => ActionStatus::Blocked { reason: *reason },
            ActionState::Failed { reason, .. } => ActionStatus::Failed { reason: *reason },
            ActionState::Unknown { reason } => {
                allowed_actions.push(ActionAllowedAction::Reconcile);
                ActionStatus::Unknown { reason: *reason }
            }
            ActionState::Succeeded { collection, .. } => {
                let collection = match collection {
                    ActionCollectionState::Pending { .. } => {
                        allowed_actions.push(ActionAllowedAction::Reconcile);
                        ActionCollectionStatus::Pending
                    }
                    ActionCollectionState::Collected { .. } => ActionCollectionStatus::Collected,
                };
                ActionStatus::Succeeded { collection }
            }
        };
        Ok(Self {
            action_ref: record.id,
            revision: record.revision,
            origin: match record.origin {
                ActionOrigin::Direct { .. } => ActionOriginKind::Direct,
                ActionOrigin::Expert { .. } => ActionOriginKind::Expert,
            },
            effect: ActionEffectSummary::from_effect(&record.effect)?,
            review_ref: record.review.clone(),
            created_at: record.created_at,
            expires_at: record.expires_at,
            status,
            allowed_actions,
            next_observation_after_ms: None,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionsPage {
    pub actions: Vec<ActionSnapshot>,
    pub next_cursor: Option<Uuid>,
}
