//! Mechanical conversion of Calendar Operations snapshots and receipt data.
use floe_access as access;
use floe_calendar_operations as owner;
use floe_protocol as dto;
use uuid::Uuid;

use super::app_wire::{AppWireResult, agent_failure, internal_error, validation};

pub(crate) fn task_receipt_in(
    value: dto::TaskExecutionReceiptRefDto,
) -> AppWireResult<floe_agent_contract::TaskExecutionReceiptRef> {
    value.validate().map_err(validation)?;
    let reference = floe_agent_contract::TaskExecutionReceiptRef {
        execution: floe_agent_contract::TaskExecutionKey {
            task_id: floe_agent_contract::TaskId::from_uuid(value.execution.task_id.get())
                .ok_or_else(|| validation("actions.receipt.task_id"))?,
            execution_id: value.execution.execution_id.get(),
            executor_generation: value.execution.executor_generation,
        },
        task_revision: value.task_revision,
        journal_revision: value.journal_revision,
        digest: digest_in(&value.digest)?,
    };
    reference.validate().map_err(agent_failure)?;
    Ok(reference)
}

fn digest_in(value: &dto::DigestHex64Dto) -> AppWireResult<[u8; 32]> {
    let mut digest = [0; 32];
    if value.as_str().len() != 64 {
        return Err(validation("actions.digest"));
    }
    for (index, chunk) in value.as_str().as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|_| validation("actions.digest"))?;
        digest[index] = u8::from_str_radix(text, 16).map_err(|_| validation("actions.digest"))?;
    }
    Ok(digest)
}

pub(crate) fn authority_mode_in(value: dto::OperationPolicyModeDto) -> access::OperationPolicyMode {
    match value {
        dto::OperationPolicyModeDto::Allow => access::OperationPolicyMode::Allow,
        dto::OperationPolicyModeDto::Ask => access::OperationPolicyMode::Ask,
        dto::OperationPolicyModeDto::Deny => access::OperationPolicyMode::Deny,
    }
}

pub(crate) fn authority_out(
    value: access::OperationAuthorizationPolicy,
    caller: &floe_app::CallerContext,
) -> AppWireResult<dto::CalendarOperationPolicyDto> {
    if value.person_id.0 != caller.person_id() {
        return Err(internal_error());
    }
    let authority = dto::CalendarOperationPolicyDto {
        revision: value.revision,
        calendar_create: match value.calendar_create {
            access::OperationPolicyMode::Allow => dto::OperationPolicyModeDto::Allow,
            access::OperationPolicyMode::Ask => dto::OperationPolicyModeDto::Ask,
            access::OperationPolicyMode::Deny => dto::OperationPolicyModeDto::Deny,
        },
    };
    authority.validate().map_err(|_| internal_error())?;
    Ok(authority)
}

pub(crate) fn snapshot_out(value: owner::ActionSnapshot) -> AppWireResult<dto::ActionSnapshotDto> {
    let snapshot = dto::ActionSnapshotDto {
        action_ref: action_ref_out(value.action_ref)?,
        revision: value.revision,
        origin: match value.origin {
            owner::ActionOriginKind::Direct => dto::ActionOriginKindDto::Direct,
            owner::ActionOriginKind::Expert => dto::ActionOriginKindDto::Expert,
        },
        effect: effect_out(value.effect)?,
        review_ref: review_out(value.review_ref)?,
        created_at: value.created_at.to_rfc3339(),
        expires_at: value.expires_at.to_rfc3339(),
        status: status_out(value.status),
        allowed_actions: value
            .allowed_actions
            .into_iter()
            .map(|action| match action {
                owner::ActionAllowedAction::Approve => dto::ActionAllowedActionDto::Approve,
                owner::ActionAllowedAction::Reject => dto::ActionAllowedActionDto::Reject,
                owner::ActionAllowedAction::Cancel => dto::ActionAllowedActionDto::Cancel,
                owner::ActionAllowedAction::Reconcile => dto::ActionAllowedActionDto::Reconcile,
            })
            .collect(),
        next_observation_after_ms: value.next_observation_after_ms,
    };
    snapshot.validate().map_err(|_| internal_error())?;
    Ok(snapshot)
}

fn effect_out(value: owner::ActionEffectSummary) -> AppWireResult<dto::ActionEffectSummaryDto> {
    Ok(match value {
        owner::ActionEffectSummary::Create {
            destination_label,
            title,
            schedule,
        } => dto::ActionEffectSummaryDto::Create {
            destination_label,
            title,
            schedule: schedule_out(schedule),
        },
        owner::ActionEffectSummary::Update {
            event_ref,
            expected_revision,
            destination_label,
            previous_title,
            previous_schedule,
            title,
            schedule,
        } => dto::ActionEffectSummaryDto::Update {
            event_ref: uuid_out(event_ref.as_uuid())?,
            expected_revision: expected_revision.0,
            destination_label,
            previous_title,
            previous_schedule: schedule_out(previous_schedule),
            title,
            schedule: schedule_out(schedule),
        },
        owner::ActionEffectSummary::Delete {
            event_ref,
            expected_revision,
            destination_label,
            title,
            schedule,
        } => dto::ActionEffectSummaryDto::Delete {
            event_ref: uuid_out(event_ref.as_uuid())?,
            expected_revision: expected_revision.0,
            destination_label,
            title,
            schedule: schedule_out(schedule),
        },
    })
}

fn schedule_out(value: floe_day::TimedSchedule) -> dto::TimedScheduleDto {
    dto::TimedScheduleDto {
        starts_at: value.starts_at.to_rfc3339(),
        ends_at: value.ends_at.to_rfc3339(),
        timezone: value.timezone,
    }
}

pub(crate) fn review_out(
    value: access::OperationApprovalRef,
) -> AppWireResult<dto::ActionReviewRefDto> {
    let review = dto::ActionReviewRefDto {
        id: uuid_out(value.id)?,
        operation_id: action_ref_out(value.operation_id)?,
        effect_digest: digest_out(value.effect_digest)?,
        source_digest: digest_out(value.source_digest)?,
        person_id: uuid_out(value.person_id.0)?,
        device_id: value.device_id,
        policy_revision: value.policy_revision,
        created_at: value.created_at.to_rfc3339(),
        expires_at: value.expires_at.to_rfc3339(),
    };
    review.validate().map_err(|_| internal_error())?;
    Ok(review)
}

fn status_out(value: owner::ActionStatus) -> dto::ActionStatusDto {
    match value {
        owner::ActionStatus::PendingReview => dto::ActionStatusDto::PendingReview,
        owner::ActionStatus::Approved => dto::ActionStatusDto::Approved,
        owner::ActionStatus::Rejected => dto::ActionStatusDto::Rejected,
        owner::ActionStatus::Cancelled => dto::ActionStatusDto::Cancelled,
        owner::ActionStatus::Expired => dto::ActionStatusDto::Expired,
        owner::ActionStatus::Executing => dto::ActionStatusDto::Executing,
        owner::ActionStatus::Blocked { reason } => dto::ActionStatusDto::Blocked {
            reason: match reason {
                owner::ActionBlockedReason::PermissionDenied => {
                    dto::ActionBlockedReasonDto::PermissionDenied
                }
                owner::ActionBlockedReason::PolicyDenied => {
                    dto::ActionBlockedReasonDto::PolicyDenied
                }
                owner::ActionBlockedReason::SourceChanged => {
                    dto::ActionBlockedReasonDto::SourceChanged
                }
                owner::ActionBlockedReason::ExecutorUnavailable => {
                    dto::ActionBlockedReasonDto::ExecutorUnavailable
                }
                owner::ActionBlockedReason::ScheduleConflict => {
                    dto::ActionBlockedReasonDto::ScheduleConflict
                }
            },
        },
        owner::ActionStatus::Failed { reason } => dto::ActionStatusDto::Failed {
            reason: match reason {
                owner::ActionNotAppliedReason::PermissionDenied => {
                    dto::ActionNotAppliedReasonDto::PermissionDenied
                }
                owner::ActionNotAppliedReason::ProviderRejected => {
                    dto::ActionNotAppliedReasonDto::ProviderRejected
                }
                owner::ActionNotAppliedReason::ProviderUnavailable => {
                    dto::ActionNotAppliedReasonDto::ProviderUnavailable
                }
                owner::ActionNotAppliedReason::SourceChanged => {
                    dto::ActionNotAppliedReasonDto::SourceChanged
                }
                owner::ActionNotAppliedReason::Cancelled => {
                    dto::ActionNotAppliedReasonDto::Cancelled
                }
                owner::ActionNotAppliedReason::Timeout => dto::ActionNotAppliedReasonDto::Timeout,
            },
        },
        owner::ActionStatus::Unknown { reason } => dto::ActionStatusDto::Unknown {
            reason: match reason {
                owner::ActionUnknownReason::Timeout => dto::ActionUnknownReasonDto::Timeout,
                owner::ActionUnknownReason::ResponseLost => {
                    dto::ActionUnknownReasonDto::ResponseLost
                }
                owner::ActionUnknownReason::InvalidReceipt => {
                    dto::ActionUnknownReasonDto::InvalidReceipt
                }
                owner::ActionUnknownReason::CancelledAfterDispatch => {
                    dto::ActionUnknownReasonDto::CancelledAfterDispatch
                }
                owner::ActionUnknownReason::InconclusiveLookup => {
                    dto::ActionUnknownReasonDto::InconclusiveLookup
                }
                owner::ActionUnknownReason::NativeOperationPending => {
                    dto::ActionUnknownReasonDto::NativeOperationPending
                }
                owner::ActionUnknownReason::NativeReceiptUnavailable => {
                    dto::ActionUnknownReasonDto::NativeReceiptUnavailable
                }
            },
        },
        owner::ActionStatus::Succeeded { collection } => dto::ActionStatusDto::Succeeded {
            collection: match collection {
                owner::ActionCollectionStatus::Pending => dto::ActionCollectionStatusDto::Pending,
                owner::ActionCollectionStatus::Collected => {
                    dto::ActionCollectionStatusDto::Collected
                }
            },
        },
    }
}

fn uuid_out(value: Uuid) -> AppWireResult<dto::UuidRefDto> {
    dto::UuidRefDto::new(value).ok_or_else(internal_error)
}
fn action_ref_out(value: Uuid) -> AppWireResult<dto::ActionRefDto> {
    dto::ActionRefDto::new(value).ok_or_else(internal_error)
}
fn digest_out(value: [u8; 32]) -> AppWireResult<dto::DigestHex64Dto> {
    dto::DigestHex64Dto::new(value.iter().map(|byte| format!("{byte:02x}")).collect())
        .ok_or_else(internal_error)
}
