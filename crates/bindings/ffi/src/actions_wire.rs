//! Mechanical conversion of safe Actions product values. The owner resolves
//! destinations, Task evidence, approval, execution and recovery.
use chrono::{DateTime, Utc};
use floe_actions as owner;
use floe_app::{ActionsCommands, ActionsQueries};
use floe_protocol as dto;
use uuid::Uuid;

use super::app_wire::{AppWireResult, agent_failure, internal_error, validation};

pub(crate) fn handles_command(command: &dto::AppProductCommandDto) -> bool {
    use dto::AppProductCommandDto::*;
    matches!(
        command,
        ActionsSubmit { .. }
            | ActionsDecide { .. }
            | ActionsReconcile { .. }
            | ActionsSetAuthority { .. }
    )
}

pub(crate) fn handles_query(query: &dto::AppProductQueryDto) -> bool {
    use dto::AppProductQueryDto::*;
    matches!(
        query,
        ActionsDestinations {}
            | ActionsProposalPreview { .. }
            | ActionsAuthority {}
            | ActionsInspect { .. }
            | ActionsList { .. }
    )
}

pub(crate) fn command(
    app: &floe_app::AppComposition,
    caller: &floe_app::CallerContext,
    command_id: Uuid,
    command: dto::AppProductCommandDto,
) -> AppWireResult<dto::AppCommandResultDto> {
    if command_id.is_nil() {
        return Err(validation("command_id"));
    }
    use dto::AppProductCommandDto as C;
    let (request, expected_action, expects_authority) = match command {
        C::ActionsSubmit { intent } => (
            floe_app::ActionsCommand::Submit {
                intent: intent_in(intent)?,
            },
            None,
            false,
        ),
        C::ActionsDecide {
            action_ref,
            review_ref,
            decision,
            expected_revision,
        } => (
            floe_app::ActionsCommand::Decide {
                action_ref: action_ref.get(),
                review_ref: review_in(review_ref)?,
                decision: decision_in(decision),
                expected_revision: revision_in(expected_revision)?,
            },
            Some(action_ref.get()),
            false,
        ),
        C::ActionsReconcile {
            action_ref,
            expected_revision,
        } => (
            floe_app::ActionsCommand::Reconcile {
                action_ref: action_ref.get(),
                expected_revision: revision_in(expected_revision)?,
            },
            Some(action_ref.get()),
            false,
        ),
        C::ActionsSetAuthority {
            mode,
            expected_revision,
        } => (
            floe_app::ActionsCommand::SetAuthority {
                mode: authority_mode_in(mode),
                expected_revision: revision_in(expected_revision)?,
            },
            None,
            true,
        ),
        _ => return Err(validation("command")),
    };
    match app
        .actions_command(caller, command_id, request)
        .map_err(agent_failure)?
    {
        floe_app::ActionsCommandResult::Action(value) if !expects_authority => {
            if expected_action.is_some_and(|expected| value.action_ref != expected) {
                return Err(internal_error());
            }
            Ok(dto::AppCommandResultDto::Action {
                action: snapshot_out(value)?,
            })
        }
        floe_app::ActionsCommandResult::Authority(value) if expects_authority => {
            Ok(dto::AppCommandResultDto::ActionsAuthority {
                authority: authority_out(value, caller)?,
            })
        }
        _ => Err(internal_error()),
    }
}

pub(crate) fn query(
    app: &floe_app::AppComposition,
    caller: &floe_app::CallerContext,
    request_id: Uuid,
    query: dto::AppProductQueryDto,
) -> AppWireResult<dto::AppQueryResultDto> {
    if request_id.is_nil() {
        return Err(validation("request_id"));
    }
    use dto::AppProductQueryDto as Q;
    let request = match query {
        Q::ActionsDestinations {} => floe_app::ActionsQuery::Destinations,
        Q::ActionsProposalPreview {
            receipt,
            artifact_id,
        } => floe_app::ActionsQuery::ProposalPreview {
            receipt: task_receipt_in(receipt)?,
            artifact_id: artifact_id.get(),
        },
        Q::ActionsAuthority {} => floe_app::ActionsQuery::Authority,
        Q::ActionsInspect { action_ref } => floe_app::ActionsQuery::Inspect {
            action_ref: action_ref.get(),
        },
        Q::ActionsList { cursor, limit } => {
            if !(1..=100).contains(&limit) {
                return Err(validation("query.limit"));
            }
            floe_app::ActionsQuery::List {
                cursor: cursor.map(|value| value.get()),
                limit,
            }
        }
        _ => return Err(validation("query")),
    };
    let result = app
        .actions_query(caller, request_id, request.clone())
        .map_err(agent_failure)?;
    match (request, result) {
        (
            floe_app::ActionsQuery::Destinations,
            floe_app::ActionsQueryResult::Destinations(values),
        ) => {
            let destinations = values
                .into_iter()
                .map(|value| {
                    Ok(dto::ActionDestinationChoiceDto {
                        destination_ref: uuid_out(value.destination_ref)?,
                        label: value.label,
                    })
                })
                .collect::<AppWireResult<Vec<_>>>()?;
            dto::validate_action_destination_choices(&destinations)
                .map_err(|_| internal_error())?;
            Ok(dto::AppQueryResultDto::ActionsDestinations { destinations })
        }
        (
            floe_app::ActionsQuery::ProposalPreview { .. },
            floe_app::ActionsQueryResult::ProposalPreview(value),
        ) => {
            let preview = match value {
                owner::ActionProposalPreview::Ready {
                    title,
                    schedule,
                    destinations,
                } => dto::ActionProposalPreviewDto::Ready {
                    title,
                    schedule: schedule_out(schedule),
                    destinations: destinations
                        .into_iter()
                        .map(|value| {
                            Ok(dto::ActionDestinationChoiceDto {
                                destination_ref: uuid_out(value.destination_ref)?,
                                label: value.label,
                            })
                        })
                        .collect::<AppWireResult<Vec<_>>>()?,
                },
                owner::ActionProposalPreview::Existing { action } => {
                    dto::ActionProposalPreviewDto::Existing {
                        action: snapshot_out(action)?,
                    }
                }
            };
            preview.validate().map_err(|_| internal_error())?;
            Ok(dto::AppQueryResultDto::ActionsProposalPreview { preview })
        }
        (floe_app::ActionsQuery::Authority, floe_app::ActionsQueryResult::Authority(value)) => {
            Ok(dto::AppQueryResultDto::ActionsAuthority {
                authority: authority_out(value, caller)?,
            })
        }
        (
            floe_app::ActionsQuery::Inspect { action_ref },
            floe_app::ActionsQueryResult::Action(value),
        ) => {
            if value.action_ref != action_ref {
                return Err(internal_error());
            }
            Ok(dto::AppQueryResultDto::Action {
                action: snapshot_out(value)?,
            })
        }
        (floe_app::ActionsQuery::List { limit, .. }, floe_app::ActionsQueryResult::Page(value)) => {
            if value.actions.len() > usize::from(limit) {
                return Err(internal_error());
            }
            let page = dto::ActionsPageDto {
                actions: value
                    .actions
                    .into_iter()
                    .map(snapshot_out)
                    .collect::<AppWireResult<Vec<_>>>()?,
                next_cursor: value.next_cursor.map(action_ref_out).transpose()?,
            };
            page.validate().map_err(|_| internal_error())?;
            Ok(dto::AppQueryResultDto::ActionsPage { page })
        }
        _ => Err(internal_error()),
    }
}

fn revision_in(value: u64) -> AppWireResult<u64> {
    if value == 0 || value > i64::MAX as u64 {
        return Err(validation("actions.expected_revision"));
    }
    Ok(value)
}

fn intent_in(value: dto::ActionIntentDto) -> AppWireResult<owner::ActionIntent> {
    value.validate().map_err(validation)?;
    let intent = match value {
        dto::ActionIntentDto::DirectCreate {
            destination_ref,
            title,
            schedule,
        } => owner::ActionIntent::DirectCreate {
            destination_ref: destination_ref.get(),
            title,
            schedule: schedule_in(schedule)?,
        },
        dto::ActionIntentDto::DirectUpdate {
            event_ref,
            expected_revision,
            title,
            schedule,
        } => owner::ActionIntent::DirectUpdate {
            event_ref: event_id_in(event_ref)?,
            expected_revision: floe_day::Revision(expected_revision),
            title,
            schedule: schedule_in(schedule)?,
        },
        dto::ActionIntentDto::DirectDelete {
            event_ref,
            expected_revision,
        } => owner::ActionIntent::DirectDelete {
            event_ref: event_id_in(event_ref)?,
            expected_revision: floe_day::Revision(expected_revision),
        },
        dto::ActionIntentDto::ExpertProposal {
            receipt,
            artifact_id,
            destination_ref,
        } => owner::ActionIntent::ExpertProposal {
            receipt: task_receipt_in(receipt)?,
            artifact_id: artifact_id.get(),
            destination_ref: destination_ref.get(),
        },
    };
    Ok(intent)
}

fn task_receipt_in(
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

fn event_id_in(value: dto::UuidRefDto) -> AppWireResult<floe_day::EventId> {
    floe_day::EventId::from_uuid(value.get()).ok_or_else(|| validation("actions.event_ref"))
}

fn schedule_in(value: dto::TimedScheduleDto) -> AppWireResult<floe_day::TimedSchedule> {
    value.validate_new_action().map_err(validation)?;
    Ok(floe_day::TimedSchedule {
        starts_at: instant_in(&value.starts_at, "actions.schedule.starts_at")?,
        ends_at: instant_in(&value.ends_at, "actions.schedule.ends_at")?,
        timezone: value.timezone,
    })
}

fn instant_in(value: &str, field: &'static str) -> AppWireResult<DateTime<Utc>> {
    let instant = DateTime::parse_from_rfc3339(value).map_err(|_| validation(field))?;
    if value.len() > 64 || instant.offset().local_minus_utc() != 0 {
        return Err(validation(field));
    }
    Ok(instant.with_timezone(&Utc))
}

fn review_in(value: dto::ActionReviewRefDto) -> AppWireResult<owner::ActionReviewRef> {
    value.validate().map_err(validation)?;
    Ok(owner::ActionReviewRef {
        id: value.id.get(),
        action_id: value.action_id.get(),
        effect_digest: digest_in(&value.effect_digest)?,
        source_digest: digest_in(&value.source_digest)?,
        authority_revision: value.authority_revision,
        expires_at: instant_in(&value.expires_at, "actions.review_ref.expires_at")?,
    })
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

fn authority_mode_in(value: dto::ActionAuthorityModeDto) -> owner::ActionAuthorityMode {
    match value {
        dto::ActionAuthorityModeDto::Allow => owner::ActionAuthorityMode::Allow,
        dto::ActionAuthorityModeDto::Ask => owner::ActionAuthorityMode::Ask,
        dto::ActionAuthorityModeDto::Deny => owner::ActionAuthorityMode::Deny,
    }
}

fn decision_in(value: dto::ActionDecisionKindDto) -> owner::ActionDecisionKind {
    match value {
        dto::ActionDecisionKindDto::Approve => owner::ActionDecisionKind::Approve,
        dto::ActionDecisionKindDto::Reject => owner::ActionDecisionKind::Reject,
        dto::ActionDecisionKindDto::Cancel => owner::ActionDecisionKind::Cancel,
    }
}

fn authority_out(
    value: owner::ActionsAuthority,
    caller: &floe_app::CallerContext,
) -> AppWireResult<dto::ActionsAuthorityDto> {
    if value.person_id.0 != caller.person_id() {
        return Err(internal_error());
    }
    let authority = dto::ActionsAuthorityDto {
        revision: value.revision,
        calendar_create: match value.calendar_create {
            owner::ActionAuthorityMode::Allow => dto::ActionAuthorityModeDto::Allow,
            owner::ActionAuthorityMode::Ask => dto::ActionAuthorityModeDto::Ask,
            owner::ActionAuthorityMode::Deny => dto::ActionAuthorityModeDto::Deny,
        },
    };
    authority.validate().map_err(|_| internal_error())?;
    Ok(authority)
}

fn snapshot_out(value: owner::ActionSnapshot) -> AppWireResult<dto::ActionSnapshotDto> {
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

fn review_out(value: owner::ActionReviewRef) -> AppWireResult<dto::ActionReviewRefDto> {
    let review = dto::ActionReviewRefDto {
        id: uuid_out(value.id)?,
        action_id: action_ref_out(value.action_id)?,
        effect_digest: digest_out(value.effect_digest)?,
        source_digest: digest_out(value.source_digest)?,
        authority_revision: value.authority_revision,
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
