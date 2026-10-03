//! Mechanical Conversation owner request/result translation.
use crate::app_wire::{AppWireResult, agent_failure, internal_error, validation};
use floe_conversation::{EventPayload, EventRead};
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, CommandId, OwnerActor, RunId};
use floe_protocol::*;
use uuid::Uuid;

pub(crate) fn handles_command(command: &AppProductCommandDto) -> bool {
    matches!(
        command,
        AppProductCommandDto::ConversationSessionStart { .. }
            | AppProductCommandDto::ConversationSessionRecover { .. }
            | AppProductCommandDto::ConversationStartTurn { .. }
            | AppProductCommandDto::ConversationCancelRun { .. }
            | AppProductCommandDto::ConversationInteractionResolve { .. }
            | AppProductCommandDto::ConversationInteractionRefresh { .. }
    )
}
pub(crate) fn handles_query(query: &AppProductQueryDto) -> bool {
    matches!(
        query,
        AppProductQueryDto::ConversationSessionResume { .. }
            | AppProductQueryDto::ConversationSessionGet { .. }
            | AppProductQueryDto::ConversationGetCommand { .. }
            | AppProductQueryDto::ConversationGetRun { .. }
            | AppProductQueryDto::ConversationGetMessage { .. }
            | AppProductQueryDto::ConversationInteractionGet { .. }
            | AppProductQueryDto::ConversationInteractionList { .. }
    )
}
pub(crate) async fn command(
    owners: &floe_app::ReadyOwners,
    actor: &OwnerActor,
    command_id: Uuid,
    command: AppProductCommandDto,
    scope: &ExecutionScope,
) -> AppWireResult<AppCommandResultDto> {
    let service = owners.conversation.as_ref();
    let id = CommandId::from_uuid(command_id).ok_or_else(internal_error)?;
    match command {
        AppProductCommandDto::ConversationSessionStart {} => {
            let receipt = service
                .start_session(actor, id, scope)
                .await
                .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?;
            let snapshot = service
                .get_session(actor, receipt.session_id, scope)
                .await
                .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?;
            Ok(AppCommandResultDto::ConversationSession {
                session: session_snapshot(snapshot)?,
            })
        }
        AppProductCommandDto::ConversationSessionRecover {
            session_id,
            expected_revision,
        } => {
            let snapshot = service
                .recover_session(actor, id, session_id.get(), expected_revision, scope)
                .await
                .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?;
            Ok(AppCommandResultDto::ConversationSession {
                session: session_snapshot(snapshot)?,
            })
        }
        AppProductCommandDto::ConversationStartTurn {
            session_id,
            expected_revision,
            text,
            continuation_ref,
            retry_of,
        } => {
            let receipt = service
                .start_turn(
                    actor,
                    floe_conversation::StartTurn {
                        command_id: id,
                        session_id: session_id.get(),
                        expected_revision,
                        text,
                        continuation_ref: continuation_ref.map(|value| {
                            floe_conversation::ContinuationToken { id: value.id.get() }
                        }),
                        retry_of: retry_of
                            .map(|value| RunId::from_uuid(value.get()).ok_or_else(internal_error))
                            .transpose()?,
                    },
                    scope,
                )
                .await
                .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?;
            Ok(AppCommandResultDto::CommandReceipt {
                receipt: command_receipt(&receipt, actor.runtime_epoch)?,
            })
        }
        AppProductCommandDto::ConversationCancelRun { run_id } => {
            let run = RunId::from_uuid(run_id.get()).ok_or_else(internal_error)?;
            let receipt = service
                .cancel_run(actor, id, run, scope)
                .await
                .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?;
            Ok(AppCommandResultDto::CancelRunReceipt {
                command_id: CommandIdDto::new(receipt.command_id.as_uuid())
                    .ok_or_else(internal_error)?,
                run_id: RunRefDto::new(receipt.run_id.as_uuid()).ok_or_else(internal_error)?,
                runtime_epoch: actor.runtime_epoch,
                outcome: AppCancelRunOutcomeDto::Accepted,
            })
        }
        AppProductCommandDto::ConversationInteractionResolve {
            interaction_id,
            session_id,
            expected_revision,
            decision,
            reviewed_digest,
        } => {
            let result = service
                .resolve_interaction(
                    actor,
                    floe_conversation::ResolveInteraction {
                        command_id,
                        interaction_id: interaction_id.get(),
                        session_id: session_id.get(),
                        expected_revision,
                        decision: match decision {
                            AppInteractionDecisionDto::Approve => {
                                floe_conversation::InteractionDecisionKind::Approve
                            }
                            AppInteractionDecisionDto::Deny => {
                                floe_conversation::InteractionDecisionKind::Deny
                            }
                            AppInteractionDecisionDto::Dismiss => {
                                floe_conversation::InteractionDecisionKind::Dismiss
                            }
                        },
                        target_digest: digest_bytes(&reviewed_digest)?,
                    },
                    scope,
                )
                .await
                .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?;
            Ok(AppCommandResultDto::InteractionOperation {
                result: resolve_result(command_id, result, actor.runtime_epoch)?,
            })
        }
        AppProductCommandDto::ConversationInteractionRefresh {
            interaction_id,
            session_id,
            expected_revision,
        } => {
            let result = service
                .refresh_interaction(
                    actor,
                    floe_conversation::RefreshInteraction {
                        command_id,
                        interaction_id: interaction_id.get(),
                        session_id: session_id.get(),
                        expected_revision,
                    },
                    scope,
                )
                .await
                .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?;
            Ok(AppCommandResultDto::InteractionRefresh {
                result: refresh_result(command_id, result, actor.runtime_epoch)?,
            })
        }
        _ => Err(validation("command")),
    }
}
pub(crate) async fn query(
    owners: &floe_app::ReadyOwners,
    actor: &OwnerActor,
    query: AppProductQueryDto,
    scope: &ExecutionScope,
) -> AppWireResult<AppQueryResultDto> {
    let service = owners.conversation.as_ref();
    match query {
        AppProductQueryDto::ConversationSessionResume {} => {
            let session = service.resume_session(actor, scope).await.map_err(
                |failure| failure_dto(failure, scope.trace_context().request_id()),
            )?;
            Ok(match session {
                Some(session) => AppQueryResultDto::ConversationSession {
                    session: session_snapshot(session)?,
                },
                None => AppQueryResultDto::ConversationSessionAbsent {},
            })
        }
        AppProductQueryDto::ConversationSessionGet { session_id } => {
            Ok(AppQueryResultDto::ConversationSession {
                session: session_snapshot(
                    service
                        .get_session(actor, session_id.get(), scope)
                        .await
                        .map_err(|failure| {
                            failure_dto(failure, scope.trace_context().request_id())
                        })?,
                )?,
            })
        }
        AppProductQueryDto::ConversationGetCommand { command_id } => {
            let id = CommandId::from_uuid(command_id.get()).ok_or_else(internal_error)?;
            Ok(
                match service
                    .read_command(actor, id, scope)
                    .await
                    .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?
                {
                    Some(run) => AppQueryResultDto::CommandReceipt {
                        receipt: command_receipt(
                            &floe_conversation::CommandReceipt::from(&run),
                            actor.runtime_epoch,
                        )?,
                    },
                    None => AppQueryResultDto::UnknownCommand { command_id },
                },
            )
        }
        AppProductQueryDto::ConversationGetRun { run_id } => {
            let id = RunId::from_uuid(run_id.get()).ok_or_else(internal_error)?;
            let run = service
                .read_run(actor, id, scope)
                .await
                .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?
                .ok_or_else(|| agent_failure(AgentFailure::NotFound))?;
            Ok(AppQueryResultDto::RunSnapshot {
                run: run_snapshot(run, actor.runtime_epoch)?,
            })
        }
        AppProductQueryDto::ConversationGetMessage { message_id } => {
            let message = service
                .read_message(actor, message_id.get(), scope)
                .await
                .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?
                .ok_or_else(|| agent_failure(AgentFailure::NotFound))?;
            Ok(AppQueryResultDto::Message {
                message: AppMessageDto {
                    message_id: MessageRefDto::new(message.message_id)
                        .ok_or_else(internal_error)?,
                    role: match message.role {
                        floe_conversation::MessageSnapshotRole::User => AppMessageRoleDto::User,
                        floe_conversation::MessageSnapshotRole::Assistant => {
                            AppMessageRoleDto::Assistant
                        }
                    },
                    text: message.text,
                },
            })
        }
        AppProductQueryDto::ConversationInteractionGet { interaction_id } => Ok(
            match service
                .read_interaction(actor, interaction_id.get(), scope)
                .await
                .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?
            {
                Some(snapshot) => AppQueryResultDto::Interaction {
                    snapshot: interaction_snapshot(snapshot)?,
                },
                None => AppQueryResultDto::UnknownInteraction { interaction_id },
            },
        ),
        AppProductQueryDto::ConversationInteractionList { session_id } => {
            let interactions = service
                .list_interactions(actor, session_id.get(), scope)
                .await
                .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?;
            Ok(AppQueryResultDto::InteractionList {
                list: AppInteractionListDto {
                    session_id,
                    interactions: interactions
                        .into_iter()
                        .map(interaction_snapshot)
                        .collect::<AppWireResult<_>>()?,
                },
            })
        }
        _ => Err(validation("query")),
    }
}
fn command_receipt(
    receipt: &floe_conversation::CommandReceipt,
    runtime_epoch: u64,
) -> AppWireResult<AppCommandReceiptDto> {
    Ok(AppCommandReceiptDto {
        command_id: CommandIdDto::new(receipt.command_id.as_uuid()).ok_or_else(internal_error)?,
        runtime_epoch,
        admission: AppCommandStatusDto::Accepted,
        run_id: Some(RunRefDto::new(receipt.run_id.as_uuid()).ok_or_else(internal_error)?),
        session_revision: Some(receipt.session_revision),
        issue: None,
    })
}
fn digest_bytes(digest: &DigestHex64Dto) -> AppWireResult<[u8; 32]> {
    let bytes = digest.as_str().as_bytes();
    let mut result = [0; 32];
    for (index, chunk) in bytes.chunks_exact(2).enumerate() {
        let high = (chunk[0] as char).to_digit(16).ok_or_else(internal_error)?;
        let low = (chunk[1] as char).to_digit(16).ok_or_else(internal_error)?;
        result[index] = ((high << 4) | low) as u8;
    }
    Ok(result)
}

macro_rules! reference {
    ($kind:ident, $value:expr) => {
        $kind::new($value).ok_or_else(internal_error)?
    };
}
fn session_snapshot(
    value: floe_conversation::SessionSnapshot,
) -> AppWireResult<ConversationSessionSnapshotDto> {
    let snapshot = ConversationSessionSnapshotDto {
        id: reference!(SessionRefDto, value.id),
        person_id: reference!(UuidRefDto, value.person_id.0),
        revision: value.revision,
        active_turn: value
            .active_turn
            .map(|id| UuidRefDto::new(id).ok_or_else(internal_error))
            .transpose()?,
        last_outcome: value
            .last_outcome
            .map(|outcome| -> AppWireResult<_> {
                Ok(match outcome {
                    floe_conversation::AgentOutcome::Completed => {
                        ConversationSessionOutcomeDto::Completed {}
                    }
                    floe_conversation::AgentOutcome::Blocked {
                        run_id,
                        review_group_id,
                    } => ConversationSessionOutcomeDto::Blocked {
                        run_id: reference!(RunRefDto, run_id.as_uuid()),
                        review_group_id: reference!(UuidRefDto, review_group_id),
                    },
                    floe_conversation::AgentOutcome::Halted { reason } => {
                        ConversationSessionOutcomeDto::Halted {
                            reason: failure_dto(reason, value.id),
                        }
                    }
                })
            })
            .transpose()?,
        usage: ConversationSessionUsageDto {
            unknown_token_attempts: value.usage.unknown_token_attempts,
            unknown_cost_attempts: value.usage.unknown_cost_attempts,
            model_attempts: value.usage.model_attempts,
            estimated_tokens: value.usage.estimated_tokens,
            estimated_cost_micros: value.usage.estimated_cost_micros,
            iterations: value.usage.iterations,
            capability_calls: value.usage.capability_calls,
            tokens: value.usage.tokens,
            cost_micros: value.usage.cost_micros,
        },
        continuation_ref: value
            .continuation_ref
            .map(|value| -> AppWireResult<_> {
                Ok(ContinuationRefDto {
                    id: reference!(UuidRefDto, value.id),
                })
            })
            .transpose()?,
        messages: value
            .messages
            .into_iter()
            .map(session_message)
            .collect::<AppWireResult<_>>()?,
        has_earlier_messages: value.has_earlier_messages,
    };
    snapshot.validate().map_err(|_| internal_error())?;
    Ok(snapshot)
}
fn session_message(
    message: floe_conversation::SessionMessage,
) -> AppWireResult<ConversationSessionMessageDto> {
    use floe_conversation::SessionMessage as M;
    Ok(match message {
        M::User {
            message_id,
            turn_id,
            text,
        } => ConversationSessionMessageDto::User {
            message_id: reference!(MessageRefDto, message_id),
            turn_id: reference!(UuidRefDto, turn_id),
            text,
        },
        M::Assistant {
            message_id,
            turn_id,
            text,
        } => ConversationSessionMessageDto::Assistant {
            message_id: reference!(MessageRefDto, message_id),
            turn_id: reference!(UuidRefDto, turn_id),
            text,
        },
        M::Preamble {
            message_id,
            turn_id,
            text,
        } => ConversationSessionMessageDto::Preamble {
            message_id: reference!(MessageRefDto, message_id),
            turn_id: reference!(UuidRefDto, turn_id),
            text,
        },
        M::Compaction {
            message_id,
            turn_id,
            summary,
        } => ConversationSessionMessageDto::Compaction {
            message_id: reference!(MessageRefDto, message_id),
            turn_id: reference!(UuidRefDto, turn_id),
            summary,
        },
        M::Capability {
            message_id,
            turn_id,
            call_id,
            capability_id,
            result,
        } => ConversationSessionMessageDto::Capability {
            message_id: reference!(MessageRefDto, message_id),
            turn_id: reference!(UuidRefDto, turn_id),
            call_id: reference!(UuidRefDto, call_id),
            capability_id,
            result: result.map_err(|failure| failure_dto(failure, call_id)),
        },
        M::Delegation {
            message_id,
            turn_id,
            task,
        } => ConversationSessionMessageDto::Delegation {
            message_id: reference!(MessageRefDto, message_id),
            turn_id: reference!(UuidRefDto, turn_id),
            task: ConversationSessionTaskDto {
                execution_receipt: task
                    .execution_receipt
                    .map(task_receipt_reference)
                    .transpose()?,
                task_id: reference!(TaskRefDto, task.task_id.as_uuid()),
                agent_id: task.agent_id,
                state: match task.state {
                    floe_agent_contract::TaskState::Submitted => {
                        ConversationTaskStateDto::Submitted
                    }
                    floe_agent_contract::TaskState::Working => ConversationTaskStateDto::Working,
                    floe_agent_contract::TaskState::Blocked => ConversationTaskStateDto::Blocked,
                    floe_agent_contract::TaskState::Completed => {
                        ConversationTaskStateDto::Completed
                    }
                    floe_agent_contract::TaskState::Failed => ConversationTaskStateDto::Failed,
                    floe_agent_contract::TaskState::Cancelled => {
                        ConversationTaskStateDto::Cancelled
                    }
                    floe_agent_contract::TaskState::Rejected => ConversationTaskStateDto::Rejected,
                    floe_agent_contract::TaskState::TimedOut => ConversationTaskStateDto::TimedOut,
                    floe_agent_contract::TaskState::Interrupted => {
                        ConversationTaskStateDto::Interrupted
                    }
                },
                result: task.result,
                issue: task
                    .issue
                    .map(|failure| failure_dto(failure, task.task_id.as_uuid())),
                artifacts: task
                    .artifacts
                    .into_iter()
                    .map(|artifact| -> AppWireResult<_> {
                        Ok(ConversationSessionArtifactDto {
                            artifact_id: reference!(UuidRefDto, artifact.artifact_id),
                            name: artifact.name,
                            media_types: artifact.media_types,
                        })
                    })
                    .collect::<AppWireResult<_>>()?,
            },
        },
        M::Interaction {
            message_id,
            turn_id,
            interaction_id,
            interaction_kind,
        } => ConversationSessionMessageDto::Interaction {
            message_id: reference!(MessageRefDto, message_id),
            turn_id: reference!(UuidRefDto, turn_id),
            interaction_id: reference!(InteractionRefDto, interaction_id),
            interaction_kind: interaction_kind_dto(interaction_kind),
        },
    })
}
fn interaction_kind_dto(kind: floe_agent_contract::UserInteractionKind) -> AppInteractionKindDto {
    match kind {
        floe_agent_contract::UserInteractionKind::SourceAccess => {
            AppInteractionKindDto::SourceAccess
        }
        floe_agent_contract::UserInteractionKind::ExpertBinding => {
            AppInteractionKindDto::ExpertBinding
        }
    }
}
fn interaction_snapshot(
    value: floe_conversation::InteractionSnapshot,
) -> AppWireResult<AppInteractionSnapshotDto> {
    use floe_conversation::{
        InteractionAction as A, InteractionStatus as S, InteractionTarget as T,
    };
    let snapshot = AppInteractionSnapshotDto {
        interaction_id: reference!(InteractionRefDto, value.interaction_id),
        session_id: reference!(SessionRefDto, value.session_id),
        origin_run_id: reference!(RunRefDto, value.origin_run_id.as_uuid()),
        interaction_kind: interaction_kind_dto(value.interaction_kind),
        state: match value.state {
            S::Pending => AppInteractionStateDto::Pending,
            S::Resolving => AppInteractionStateDto::Resolving,
            S::Resolved => AppInteractionStateDto::Resolved,
            S::Denied => AppInteractionStateDto::Denied,
            S::Cancelled => AppInteractionStateDto::Dismissed,
            S::Superseded => AppInteractionStateDto::Superseded,
            S::Expired => AppInteractionStateDto::Expired,
        },
        revision: value.revision,
        target_digest: DigestHex64Dto::new(
            value
                .target_digest
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect(),
        )
        .ok_or_else(internal_error)?,
        created_at: chrono::DateTime::from_timestamp_millis(value.created_at_unix_ms)
            .ok_or_else(internal_error)?,
        expires_at: chrono::DateTime::from_timestamp_millis(value.expires_at_unix_ms)
            .ok_or_else(internal_error)?,
        target: match value.target {
            T::SourceReview { review } => AppInteractionTargetDto::SourceReview {
                review: crate::connections_wire::observe_review(review)?,
            },
            T::NavigationOnly {
                destination,
                source_label,
            } => AppInteractionTargetDto::NavigationOnly {
                source_label,
                destination: match destination {
                    floe_conversation::NavigationDestination::ConnectionSettings => {
                        AppNavigationDestinationDto::ConnectionSettings
                    }
                    floe_conversation::NavigationDestination::SystemPermission => {
                        AppNavigationDestinationDto::SystemPermission
                    }
                    floe_conversation::NavigationDestination::ResourcePicker => {
                        AppNavigationDestinationDto::ResourcePicker
                    }
                },
            },
            T::ExpertBinding { review } => AppInteractionTargetDto::ExpertBinding {
                review: crate::experts_wire::binding_review_to_dto(review)?,
            },
        },
        actions: value
            .allowed_actions
            .into_iter()
            .map(|action| match action {
                A::Allow => AppInteractionActionDto::Allow,
                A::Deny => AppInteractionActionDto::Deny,
                A::Dismiss => AppInteractionActionDto::Dismiss,
                A::Refresh => AppInteractionActionDto::Refresh,
                A::OpenConnection => AppInteractionActionDto::OpenConnection,
                A::ReviewSource => AppInteractionActionDto::ReviewSource,
                A::RequestPermission => AppInteractionActionDto::RequestPermission,
                A::OpenExpertSettings => AppInteractionActionDto::OpenExpertSettings,
            })
            .collect(),
    };
    snapshot.validate().map_err(|_| internal_error())?;
    Ok(snapshot)
}
fn resolve_result(
    command_id: Uuid,
    value: floe_conversation::InteractionResult,
    epoch: u64,
) -> AppWireResult<AppInteractionResolveResultDto> {
    use floe_conversation::InteractionStatus as S;
    let outcome = match value.interaction.state {
        S::Pending => AppInteractionResolveOutcomeDto::Pending,
        S::Resolving => AppInteractionResolveOutcomeDto::Resolving,
        S::Resolved => AppInteractionResolveOutcomeDto::Resolved,
        S::Denied => AppInteractionResolveOutcomeDto::Denied,
        S::Cancelled => AppInteractionResolveOutcomeDto::Dismissed,
        S::Superseded => AppInteractionResolveOutcomeDto::Superseded,
        S::Expired => AppInteractionResolveOutcomeDto::Expired,
    };
    Ok(AppInteractionResolveResultDto {
        command_id: reference!(CommandIdDto, command_id),
        outcome,
        replacement_id: value
            .interaction
            .replacement_id
            .map(|id| InteractionRefDto::new(id).ok_or_else(internal_error))
            .transpose()?,
        snapshot: interaction_snapshot(value.interaction)?,
        linked_run: value
            .linked
            .as_ref()
            .map(|value| command_receipt(value, epoch))
            .transpose()?,
    })
}
fn refresh_result(
    command_id: Uuid,
    value: floe_conversation::InteractionResult,
    epoch: u64,
) -> AppWireResult<AppInteractionRefreshResultDto> {
    use floe_conversation::InteractionStatus as S;
    let outcome = match value.interaction.state {
        S::Pending => AppInteractionRefreshOutcomeDto::Pending,
        S::Resolving => AppInteractionRefreshOutcomeDto::Resolving,
        S::Resolved => AppInteractionRefreshOutcomeDto::Resolved,
        S::Denied => AppInteractionRefreshOutcomeDto::Denied,
        S::Cancelled => AppInteractionRefreshOutcomeDto::Dismissed,
        S::Superseded => AppInteractionRefreshOutcomeDto::Superseded,
        S::Expired => AppInteractionRefreshOutcomeDto::Expired,
    };
    Ok(AppInteractionRefreshResultDto {
        command_id: reference!(CommandIdDto, command_id),
        outcome,
        replacement_id: value
            .interaction
            .replacement_id
            .map(|id| InteractionRefDto::new(id).ok_or_else(internal_error))
            .transpose()?,
        snapshot: interaction_snapshot(value.interaction)?,
        linked_run: value
            .linked
            .as_ref()
            .map(|value| command_receipt(value, epoch))
            .transpose()?,
    })
}
fn run_snapshot(
    run: floe_conversation::RunReceipt,
    epoch: u64,
) -> AppWireResult<AppRunSnapshotDto> {
    run_projection(
        floe_conversation::project_run_snapshot(&run).map_err(agent_failure)?,
        epoch,
    )
}
fn run_event_snapshot(
    run: floe_conversation::RunEventRecord,
    epoch: u64,
) -> AppWireResult<AppRunSnapshotDto> {
    run_projection(
        floe_conversation::project_run_event(run).map_err(agent_failure)?,
        epoch,
    )
}
fn run_projection(
    run: floe_conversation::RunSnapshot,
    runtime_epoch: u64,
) -> AppWireResult<AppRunSnapshotDto> {
    use floe_conversation::{PublicRunState as S, ReplyStatus as R, TurnExecution as E};
    let report = run
        .report
        .map(|report| -> AppWireResult<_> {
            Ok(AppTurnReportDto {
                execution: match report.execution {
                    E::Completed => AppTurnExecutionDto::Completed,
                    E::Partial => AppTurnExecutionDto::Partial,
                    E::Failed => AppTurnExecutionDto::Failed,
                    E::Cancelled => AppTurnExecutionDto::Cancelled,
                    E::Indeterminate => AppTurnExecutionDto::Indeterminate,
                    E::Blocked => AppTurnExecutionDto::Blocked,
                },
                reply: match report.reply {
                    R::Generated => AppReplyStatusDto::Generated,
                    R::NotProduced => AppReplyStatusDto::NotProduced,
                },
                issues: report
                    .issues
                    .into_iter()
                    .map(|failure| failure_dto(failure, run.run_id.as_uuid()))
                    .collect(),
                action_refs: report
                    .action_refs
                    .into_iter()
                    .map(|id| ActionRefDto::new(id).ok_or_else(internal_error))
                    .collect::<AppWireResult<_>>()?,
                interaction_refs: report
                    .interaction_refs
                    .into_iter()
                    .map(|id| InteractionRefDto::new(id).ok_or_else(internal_error))
                    .collect::<AppWireResult<_>>()?,
                final_message_ref: report
                    .final_message_ref
                    .map(|id| MessageRefDto::new(id).ok_or_else(internal_error))
                    .transpose()?,
            })
        })
        .transpose()?;
    let snapshot = AppRunSnapshotDto {
        run_id: reference!(RunRefDto, run.run_id.as_uuid()),
        session_id: reference!(SessionRefDto, run.session_id),
        revision: run.revision,
        runtime_epoch,
        executor_generation: run.executor_generation,
        state: match run.state {
            S::Accepted => AppRunStateDto::Accepted,
            S::Executing => AppRunStateDto::Executing,
            S::Finalizing => AppRunStateDto::Finalizing,
            S::Cancelling => AppRunStateDto::Cancelling,
            S::Blocked => AppRunStateDto::Blocked,
            S::Finished => AppRunStateDto::Finished,
        },
        progress: run.progress,
        task_refs: run
            .task_refs
            .into_iter()
            .map(|id| TaskRefDto::new(id).ok_or_else(internal_error))
            .collect::<AppWireResult<_>>()?,
        attempt_refs: run
            .attempt_refs
            .into_iter()
            .map(|id| AttemptRefDto::new(id).ok_or_else(internal_error))
            .collect::<AppWireResult<_>>()?,
        report,
    };
    snapshot.validate().map_err(|_| internal_error())?;
    Ok(snapshot)
}
pub(crate) async fn events(
    owners: &floe_app::ReadyOwners,
    actor: &OwnerActor,
    request: AppEventsRequestDto,
    scope: &ExecutionScope,
) -> AppWireResult<AppEventsResultDto> {
    let read = owners
        .conversation
        .read_events(
            actor,
            floe_conversation::ReadConversationEvents {
                runtime_epoch: request.runtime_epoch,
                cursor: request.cursor,
                limit: request.limit,
            },
            scope,
        )
        .await
        .map_err(|failure| failure_dto(failure, scope.trace_context().request_id()))?;
    Ok(match read {
        EventRead::ResyncRequired { snapshot_cursor } => AppEventsResultDto::ResyncRequired {
            runtime_epoch: actor.runtime_epoch,
            snapshot_cursor,
        },
        EventRead::Events {
            next_cursor,
            events,
        } => AppEventsResultDto::Events {
            runtime_epoch: actor.runtime_epoch,
            next_cursor,
            events: events
                .into_iter()
                .map(|event| -> AppWireResult<_> {
                    Ok(AppEventDto {
                        cursor: event.cursor,
                        aggregate_revision: event.aggregate_revision,
                        runtime_epoch: actor.runtime_epoch,
                        event: match event.payload {
                            EventPayload::CommandUpdated {
                                command_id,
                                run_id,
                                session_revision,
                            } => AppEventKindDto::CommandUpdated {
                                receipt: command_receipt(
                                    &floe_conversation::CommandReceipt {
                                        command_id,
                                        run_id,
                                        session_revision,
                                    },
                                    actor.runtime_epoch,
                                )?,
                            },
                            EventPayload::RunUpdated(run) => AppEventKindDto::RunUpdated {
                                run: run_event_snapshot(run, actor.runtime_epoch)?,
                            },
                        },
                    })
                })
                .collect::<AppWireResult<_>>()?,
        },
    })
}

pub(crate) fn failure_dto(reason: AgentFailure, correlation_id: Uuid) -> AppWireErrorDto {
    let value = floe_conversation::project_conversation_failure(reason, correlation_id);
    let mut error = agent_failure(reason);
    use floe_conversation::ConversationRecovery as R;
    error.owner_failure = Some(OwnerFailureDto {
        domain: value.domain,
        category: value.category,
        reason: value.reason,
        incident_id: UuidRefDto::new(value.incident_id).expect("owner incident identity"),
        correlation_id: UuidRefDto::new(value.correlation_id)
            .expect("admitted correlation identity"),
        reload_required: value.reload_required,
        seal_session: value.seal_session,
        recovery: match value.recovery {
            R::None => OwnerRecoveryDto::None,
            R::Reobserve => OwnerRecoveryDto::Reobserve,
            R::Reconcile => OwnerRecoveryDto::Reconcile,
            R::Unlock => OwnerRecoveryDto::Unlock,
            R::Reopen => OwnerRecoveryDto::Reopen,
            R::NewReview => OwnerRecoveryDto::NewReview,
        },
        safe_actions: value.safe_actions,
    });
    error
}

fn task_receipt_reference(
    value: floe_agent_contract::TaskExecutionReceiptRef,
) -> AppWireResult<TaskExecutionReceiptRefDto> {
    value.validate().map_err(|_| internal_error())?;
    let digest = value
        .digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let result = TaskExecutionReceiptRefDto {
        execution: TaskExecutionKeyDto {
            task_id: TaskRefDto::new(value.execution.task_id.as_uuid())
                .ok_or_else(internal_error)?,
            execution_id: UuidRefDto::new(value.execution.execution_id)
                .ok_or_else(internal_error)?,
            executor_generation: value.execution.executor_generation,
        },
        task_revision: value.task_revision,
        journal_revision: value.journal_revision,
        digest: DigestHex64Dto::new(digest).ok_or_else(internal_error)?,
    };
    result.validate().map_err(|_| internal_error())?;
    Ok(result)
}
