use std::collections::BTreeMap;

use floe_app::{AgentFailure, EventPayload, EventRead, RunEventRecord, RunReceipt, RunState};
use floe_protocol::{
    AppCancelRunOutcomeDto, AppCommandDto, AppCommandReceiptDto, AppCommandRequestDto,
    AppCommandResultDto, AppCommandStatusDto, AppEventDto, AppEventKindDto, AppEventsRequestDto,
    AppEventsResultDto, AppMessageDto, AppMessageRoleDto, AppProfileSelectionDto, AppQueryDto,
    AppQueryRequestDto, AppQueryResultDto, AppReplyStatusDto, AppRunSnapshotDto, AppRunStateDto,
    AppTurnExecutionDto, AppTurnModeDto, AppTurnReportDto, AppWireErrorCodeDto, AppWireErrorDto,
};

use crate::bridge::FloeHandle;

pub(crate) type AppWireResult<T> = Result<T, AppWireErrorDto>;

pub(crate) fn command(
    handle: &FloeHandle,
    request: AppCommandRequestDto,
) -> AppWireResult<AppCommandResultDto> {
    command_with_host(&handle.app(), request)
}

fn command_with_host<Services>(
    host: &floe_app::AppHost<Services>,
    request: AppCommandRequestDto,
) -> AppWireResult<AppCommandResultDto>
where
    Services: floe_app::HostServices
        + floe_app::ConversationCommands
        + floe_app::VaultLifecycleCommands
        + floe_app::ConversationSessionCommands
        + floe_app::ExpertCommands
        + floe_app::LocalAccessCommands
        + floe_app::KnowledgeCommands
        + floe_app::DayCommands
        + floe_app::ActionCommands
        + floe_app::LocalContextCommands,
{
    request.validate().map_err(request_validation)?;
    let host_request = host.request(request.request_id).map_err(host_failure)?;
    let runtime_epoch = host_request.caller().runtime_epoch();
    match request.command {
        AppCommandDto::ContextApply { command } => {
            let command = crate::context_wire::command(command).map_err(structural_error)?;
            let result = host_request
                .services()
                .apply_local_context(host_request.caller(), command)
                .map_err(|failure| structural_error(floe_protocol::wire::agent_failure(failure)))?;
            Ok(AppCommandResultDto::ContextApplied {
                command_id: request.command_id,
                context: crate::conversion::native::local_context_result(
                    floe_app::PersonId(host_request.caller().person_id()),
                    result,
                ),
            })
        }
        AppCommandDto::ActionsCalendar { operation } => {
            let command = crate::conversion::owners::calendar_action_operation(operation)
                .map_err(structural_error)?;
            let result = host_request
                .services()
                .action_command(host_request.caller(), request.command_id, command)
                .map_err(service_error)?;
            Ok(AppCommandResultDto::ActionOperation {
                result: action_result(result, request.command_id)?,
            })
        }
        AppCommandDto::DayMutate { day, mutation } => {
            let result = host_request
                .services()
                .mutate_day(
                    host_request.caller(),
                    floe_app::DayMutationRequest {
                        command_id: request.command_id,
                        day: crate::day_wire::read(day).map_err(structural_error)?,
                        mutation: crate::day_wire::mutation(mutation).map_err(structural_error)?,
                    },
                )
                .map_err(day_error)?;
            if result.command_id != request.command_id {
                return Err(service_error(floe_app::ServiceError::Internal));
            }
            Ok(AppCommandResultDto::DayMutation {
                command_id: result.command_id,
                mutation: floe_protocol::MutationResultDto {
                    snapshot: crate::conversion::day_snapshot_to_dto(result.snapshot).map_err(
                        |failure| structural_error(floe_protocol::wire::conversion_error(failure)),
                    )?,
                    changed_item: result
                        .changed_item
                        .map(crate::conversion::timeline_item_to_dto),
                    capture: result.capture.map(crate::conversion::capture_to_dto),
                },
            })
        }
        AppCommandDto::KnowledgeMemoryDecide {
            candidate_id,
            decision,
        } => {
            let decision = floe_app::MemoryReviewDecision {
                candidate_id,
                kind: match decision {
                    floe_protocol::AgentMemoryReviewDecisionKindDto::Approve => {
                        floe_app::KnowledgeDecisionKind::Approve
                    }
                    floe_protocol::AgentMemoryReviewDecisionKindDto::Reject => {
                        floe_app::KnowledgeDecisionKind::Reject
                    }
                },
            };
            let result = host_request
                .services()
                .decide_memory(host_request.caller(), request.command_id, decision)
                .map_err(service_error)?;
            Ok(AppCommandResultDto::KnowledgeOperation {
                result: knowledge_result(result, request.command_id)?,
            })
        }
        AppCommandDto::AccessCalendarConfigure { change } => {
            let change = floe_app::CalendarGrantConfiguration {
                instance_id: change.instance_id,
                expected_revision: change.expected_revision,
                setup_id: change.setup_id,
                change: match change.change {
                    floe_protocol::CalendarGrantChangeDto::SetEnabled { enabled } => {
                        floe_app::CalendarGrantChange::SetEnabled { enabled }
                    }
                    floe_protocol::CalendarGrantChangeDto::Remove {} => {
                        floe_app::CalendarGrantChange::Remove
                    }
                    floe_protocol::CalendarGrantChangeDto::SetScope {
                        provider,
                        calendar_ids,
                        connection_scope,
                        connection_revision,
                        source_authority,
                        reviewed_native_subject_fingerprint,
                    } => floe_app::CalendarGrantChange::SetScope {
                        provider: crate::conversion::calendar_provider_from_dto(provider),
                        calendar_ids,
                        connection_scope: crate::conversion::calendar_scope_from_dto(
                            connection_scope,
                        ),
                        connection_revision,
                        source_authority,
                        reviewed_native_subject_fingerprint,
                    },
                },
            };
            let result = host_request
                .services()
                .local_access_command(
                    host_request.caller(),
                    request.command_id,
                    floe_app::LocalAccessCommand::Calendar(change),
                )
                .map_err(service_error)?;
            Ok(AppCommandResultDto::LocalAccessOperation {
                result: local_access_result(result, request.command_id)?,
            })
        }
        AppCommandDto::AccessPersonalConfigure { connector, change } => {
            let command = floe_app::LocalAccessCommand::Personal {
                connector,
                change: crate::conversion::owners::personal_access_change(&change),
            };
            let result = host_request
                .services()
                .local_access_command(host_request.caller(), request.command_id, command)
                .map_err(service_error)?;
            Ok(AppCommandResultDto::LocalAccessOperation {
                result: local_access_result(result, request.command_id)?,
            })
        }
        AppCommandDto::AccessContactsConfigure { connector, change } => {
            let command = floe_app::LocalAccessCommand::Contacts {
                connector,
                change: crate::conversion::owners::contacts_access_change(&change),
            };
            let result = host_request
                .services()
                .local_access_command(host_request.caller(), request.command_id, command)
                .map_err(service_error)?;
            Ok(AppCommandResultDto::LocalAccessOperation {
                result: local_access_result(result, request.command_id)?,
            })
        }
        AppCommandDto::ExpertsRegistryConfigure { change } => {
            let command =
                floe_app::ExpertCommand::ConfigureRegistry(floe_app::RegistryConfiguration {
                    instance_id: change.instance_id,
                    expected_revision: change.expected_revision,
                    target: match change.target {
                        floe_protocol::RegistryConfigurationTargetDto::Installation {
                            id,
                            enabled,
                        } => floe_app::RegistryConfigurationTarget::Installation { id, enabled },
                        floe_protocol::RegistryConfigurationTargetDto::Assignment {
                            id,
                            enabled,
                        } => floe_app::RegistryConfigurationTarget::Assignment { id, enabled },
                        floe_protocol::RegistryConfigurationTargetDto::CalendarView {
                            id,
                            enabled,
                        } => floe_app::RegistryConfigurationTarget::CalendarView { id, enabled },
                    },
                });
            let result = host_request
                .services()
                .expert_command(host_request.caller(), request.command_id, command)
                .map_err(service_error)?;
            Ok(AppCommandResultDto::ExpertOperation {
                result: expert_result(result, request.command_id)?,
            })
        }
        AppCommandDto::ExpertsCalendarInstall { setup } => {
            let command =
                floe_app::ExpertCommand::InstallCalendar(floe_app::CalendarExpertInstall {
                    instance_id: setup.instance_id,
                    expected_revision: setup.expected_revision,
                    setup_id: setup.setup_id,
                    provider: crate::conversion::calendar_provider_from_dto(setup.provider),
                    calendar_ids: setup.calendar_ids,
                    connection_scope: crate::conversion::calendar_scope_from_dto(
                        setup.connection_scope,
                    ),
                    connection_revision: setup.connection_revision,
                    source_authority: setup.source_authority,
                    reviewed_native_subject_fingerprint: setup.reviewed_native_subject_fingerprint,
                });
            let result = host_request
                .services()
                .expert_command(host_request.caller(), request.command_id, command)
                .map_err(service_error)?;
            Ok(AppCommandResultDto::ExpertOperation {
                result: expert_result(result, request.command_id)?,
            })
        }
        AppCommandDto::ConversationSessionStart {}
        | AppCommandDto::ConversationSessionResume {}
        | AppCommandDto::ConversationSessionRecover { .. } => {
            let command = match request.command {
                AppCommandDto::ConversationSessionStart {} => {
                    floe_app::ConversationSessionCommand::Start
                }
                AppCommandDto::ConversationSessionResume {} => {
                    floe_app::ConversationSessionCommand::Resume
                }
                AppCommandDto::ConversationSessionRecover {
                    session_id,
                    expected_revision,
                } => floe_app::ConversationSessionCommand::Recover {
                    session_id,
                    expected_revision,
                },
                _ => unreachable!(),
            };
            let result = host_request
                .services()
                .session_command(host_request.caller(), request.command_id, command)
                .map_err(service_error)?;
            Ok(AppCommandResultDto::ConversationSessionOperation {
                result: session_result(result, request.command_id)?,
            })
        }
        AppCommandDto::VaultCreate {}
        | AppCommandDto::VaultUnlock {}
        | AppCommandDto::VaultLock {} => {
            let command = match request.command {
                AppCommandDto::VaultCreate {} => floe_app::VaultLifecycleCommand::Create,
                AppCommandDto::VaultUnlock {} => floe_app::VaultLifecycleCommand::Unlock,
                AppCommandDto::VaultLock {} => floe_app::VaultLifecycleCommand::Lock,
                _ => unreachable!(),
            };
            let result = host_request
                .services()
                .vault_command(host_request.caller(), request.command_id, command)
                .map_err(service_error)?;
            if result.operation_id != request.command_id {
                return Err(service_error(floe_app::ServiceError::Internal));
            }
            Ok(AppCommandResultDto::VaultOperation {
                result: vault_result(result),
            })
        }
        AppCommandDto::ConversationStartTurn {
            session_id,
            expected_revision,
            text,
            mode,
            profile,
            retry_of,
        } => {
            let mode = match mode {
                AppTurnModeDto::NewTurn {} => floe_app::TurnMode::New,
                AppTurnModeDto::Continue { continuation_ref } => {
                    floe_app::TurnMode::Continue(floe_app::ContinuationRef {
                        run_id: continuation_ref.run_id,
                        executor_generation: continuation_ref.executor_generation,
                        level: continuation_ref.level,
                    })
                }
            };
            let profile = match profile {
                AppProfileSelectionDto::Auto => floe_app::ProfileSelection::Auto,
                AppProfileSelectionDto::Explicit { profile_id } => {
                    floe_app::ProfileSelection::Explicit(profile_id)
                }
            };
            let mut service_request = floe_app::StartTurn {
                command_id: request.command_id,
                session_id,
                expected_revision,
                text,
                mode,
                retry_of,
                profile,
            };
            service_request.normalize_text().map_err(service_error)?;
            let receipt = host_request
                .services()
                .start_turn(host_request.caller(), service_request)
                .map_err(service_error)?;
            Ok(AppCommandResultDto::CommandReceipt {
                receipt: AppCommandReceiptDto {
                    command_id: receipt.command_id,
                    runtime_epoch,
                    admission: AppCommandStatusDto::Accepted,
                    run_id: Some(receipt.run_id),
                    session_revision: Some(receipt.session_revision),
                    issue: None,
                },
            })
        }
        AppCommandDto::ConversationCancelRun { run_id, .. } => {
            let receipt = host_request
                .services()
                .cancel_run(
                    host_request.caller(),
                    floe_app::CancelRun {
                        command_id: request.command_id,
                        run_id,
                    },
                )
                .map_err(service_error)?;
            Ok(AppCommandResultDto::CancelRunReceipt {
                command_id: receipt.command_id,
                run_id: receipt.run_id,
                runtime_epoch,
                outcome: match receipt.outcome {
                    floe_app::CancelRunOutcome::Accepted => AppCancelRunOutcomeDto::Accepted,
                },
            })
        }
    }
}

pub(crate) fn query(
    handle: &FloeHandle,
    request: AppQueryRequestDto,
) -> AppWireResult<AppQueryResultDto> {
    query_with_host(&handle.app(), request)
}

fn query_with_host<
    Services: floe_app::HostServices
        + floe_app::ConversationQueries
        + floe_app::VaultLifecycleQueries
        + floe_app::ConversationSessionQueries
        + floe_app::ExpertQueries
        + floe_app::LocalAccessQueries
        + floe_app::KnowledgeQueries
        + floe_app::ConnectionsQueries
        + floe_app::DayQueries
        + floe_app::ActionQueries
        + floe_app::LocalContextQueries,
>(
    host: &floe_app::AppHost<Services>,
    request: AppQueryRequestDto,
) -> AppWireResult<AppQueryResultDto> {
    request.validate().map_err(request_validation)?;
    let host_request = host.request(request.request_id).map_err(host_failure)?;
    let caller = host_request.caller();
    let runtime_epoch = caller.runtime_epoch();
    let services = host_request.services();
    match request.query {
        AppQueryDto::ContextRead { query } => {
            let query = crate::context_wire::query(query).map_err(structural_error)?;
            let result = services
                .query_local_context(caller, query)
                .map_err(|failure| structural_error(floe_protocol::wire::agent_failure(failure)))?;
            Ok(AppQueryResultDto::ContextRead {
                context: crate::conversion::native::local_context_result(
                    floe_app::PersonId(caller.person_id()),
                    result,
                ),
            })
        }
        AppQueryDto::ActionsCapabilities {}
        | AppQueryDto::ActionsAuthority {}
        | AppQueryDto::ActionsList {}
        | AppQueryDto::ActionsGet { .. }
        | AppQueryDto::ActionsProposalInspect { .. } => {
            let inspection = match request.query {
                AppQueryDto::ActionsCapabilities {} => floe_app::ActionInspection::Capabilities,
                AppQueryDto::ActionsAuthority {} => floe_app::ActionInspection::Authority,
                AppQueryDto::ActionsList {} => floe_app::ActionInspection::List,
                AppQueryDto::ActionsGet { action_id } => {
                    floe_app::ActionInspection::Get { action_id }
                }
                AppQueryDto::ActionsProposalInspect {
                    session_id,
                    invocation_id,
                } => floe_app::ActionInspection::Proposal {
                    session_id,
                    invocation_id,
                },
                _ => unreachable!(),
            };
            let result = services
                .inspect_actions(caller, request.request_id, inspection)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::ActionOperation {
                result: action_result(result, request.request_id)?,
            })
        }
        AppQueryDto::ActionsReadResult {
            operation_id,
            release,
        } => {
            let result = services
                .read_action_result(caller, operation_id, release)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::ActionOperation {
                result: action_result(result, operation_id)?,
            })
        }
        AppQueryDto::DaySnapshot { day } => {
            let result = services
                .read_day(
                    caller,
                    crate::day_wire::read(day).map_err(structural_error)?,
                )
                .map_err(day_error)?;
            Ok(AppQueryResultDto::DaySnapshot {
                snapshot: crate::conversion::day_snapshot_to_dto(result).map_err(|failure| {
                    structural_error(floe_protocol::wire::conversion_error(failure))
                })?,
            })
        }
        AppQueryDto::KnowledgeMemoryOverview {} | AppQueryDto::KnowledgeMemoryReview {} => {
            let inspection = if matches!(request.query, AppQueryDto::KnowledgeMemoryOverview {}) {
                floe_app::KnowledgeInspection::Memory
            } else {
                floe_app::KnowledgeInspection::Review
            };
            let result = services
                .inspect_knowledge(caller, request.request_id, inspection)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::KnowledgeOperation {
                result: knowledge_result(result, request.request_id)?,
            })
        }
        AppQueryDto::KnowledgeReadResult {
            operation_id,
            release,
        } => {
            let result = services
                .read_knowledge_result(caller, operation_id, release)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::KnowledgeOperation {
                result: knowledge_result(result, operation_id)?,
            })
        }
        AppQueryDto::ConnectionsOverview {} => {
            let result = services
                .inspect_connections(caller, request.request_id)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::ConnectionsOperation {
                result: connections_result(result, request.request_id)?,
            })
        }
        AppQueryDto::ConnectionsReadResult {
            operation_id,
            release,
        } => {
            let result = services
                .read_connections_result(caller, operation_id, release)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::ConnectionsOperation {
                result: connections_result(result, operation_id)?,
            })
        }
        AppQueryDto::AccessCalendarPreview { request: subject } => {
            let inspection =
                floe_app::LocalAccessInspection::CalendarSubject(floe_app::CalendarSubjectIntent {
                    provider: crate::conversion::calendar_provider_from_dto(subject.provider),
                    connection_id: subject.connection_id,
                    calendar_ids: subject.calendar_ids,
                    connection_scope: crate::conversion::calendar_scope_from_dto(
                        subject.connection_scope,
                    ),
                    connection_revision: subject.connection_revision,
                    source_authority: subject.source_authority,
                });
            let result = services
                .inspect_local_access(caller, request.request_id, inspection)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::LocalAccessOperation {
                result: local_access_result(result, request.request_id)?,
            })
        }
        AppQueryDto::AccessPersonalInspect { connector } => {
            let result = services
                .inspect_local_access(
                    caller,
                    request.request_id,
                    floe_app::LocalAccessInspection::Personal { connector },
                )
                .map_err(service_error)?;
            Ok(AppQueryResultDto::LocalAccessOperation {
                result: local_access_result(result, request.request_id)?,
            })
        }
        AppQueryDto::AccessContactsInspect {
            connector,
            selected_handles,
        } => {
            let result = services
                .inspect_local_access(
                    caller,
                    request.request_id,
                    floe_app::LocalAccessInspection::Contacts {
                        connector,
                        selected_handles,
                    },
                )
                .map_err(service_error)?;
            Ok(AppQueryResultDto::LocalAccessOperation {
                result: local_access_result(result, request.request_id)?,
            })
        }
        AppQueryDto::AccessLocalReadResult {
            operation_id,
            release,
        } => {
            let result = services
                .read_local_access_result(caller, operation_id, release)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::LocalAccessOperation {
                result: local_access_result(result, operation_id)?,
            })
        }
        AppQueryDto::ExpertsRegistryInspect {} | AppQueryDto::ExpertsCalendarInspect {} => {
            let inspection = if matches!(request.query, AppQueryDto::ExpertsRegistryInspect {}) {
                floe_app::ExpertInspection::Registry
            } else {
                floe_app::ExpertInspection::Calendar
            };
            let result = services
                .inspect_experts(caller, request.request_id, inspection)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::ExpertOperation {
                result: expert_result(result, request.request_id)?,
            })
        }
        AppQueryDto::ExpertsReadResult {
            operation_id,
            release,
        } => {
            let result = services
                .read_expert_result(caller, operation_id, release)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::ExpertOperation {
                result: expert_result(result, operation_id)?,
            })
        }
        AppQueryDto::ConversationSessionGet { session_id } => {
            let result = services
                .get_session(caller, request.request_id, session_id)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::ConversationSessionOperation {
                result: session_result(result, request.request_id)?,
            })
        }
        AppQueryDto::ConversationSessionReadResult {
            operation_id,
            release,
        } => {
            let result = services
                .read_session_result(caller, operation_id, release)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::ConversationSessionOperation {
                result: session_result(result, operation_id)?,
            })
        }
        AppQueryDto::VaultStatus {} => {
            let result = services
                .vault_status(caller, request.request_id)
                .map_err(service_error)?;
            if result.operation_id != request.request_id {
                return Err(service_error(floe_app::ServiceError::Internal));
            }
            Ok(AppQueryResultDto::VaultOperation {
                result: vault_result(result),
            })
        }
        AppQueryDto::VaultReadResult {
            operation_id,
            release,
        } => {
            let result = services
                .read_vault_result(caller, operation_id, release)
                .map_err(service_error)?;
            if result.operation_id != operation_id {
                return Err(service_error(floe_app::ServiceError::Internal));
            }
            Ok(AppQueryResultDto::VaultOperation {
                result: vault_result(result),
            })
        }
        AppQueryDto::ConversationGetCommand { command_id } => {
            let receipt = services
                .read_conversation(caller, floe_app::ReadConversation::Command { command_id })
                .map_err(service_error)?;
            Ok(match receipt {
                Some(receipt) => AppQueryResultDto::CommandReceipt {
                    receipt: command_receipt(&receipt, runtime_epoch),
                },
                None => AppQueryResultDto::UnknownCommand { command_id },
            })
        }
        AppQueryDto::ConversationGetRun { run_id } => {
            let receipt = services
                .read_conversation(caller, floe_app::ReadConversation::Run { run_id })
                .map_err(service_error)?
                .ok_or_else(not_found)?;
            Ok(AppQueryResultDto::RunSnapshot {
                run: run_snapshot(receipt, runtime_epoch),
            })
        }
        AppQueryDto::ConversationGetMessage { message_id } => {
            let receipt = services
                .read_conversation(caller, floe_app::ReadConversation::Message { message_id })
                .map_err(service_error)?
                .ok_or_else(not_found)?;
            let text = receipt.output.ok_or_else(not_found)?;
            Ok(AppQueryResultDto::Message {
                message: AppMessageDto {
                    message_id,
                    role: AppMessageRoleDto::Assistant,
                    text,
                },
            })
        }
    }
}

pub(crate) fn events(
    handle: &FloeHandle,
    request: AppEventsRequestDto,
) -> AppWireResult<AppEventsResultDto> {
    events_with_host(&handle.app(), request)
}

fn events_with_host<Services: floe_app::HostServices + floe_app::ConversationEvents>(
    host: &floe_app::AppHost<Services>,
    request: AppEventsRequestDto,
) -> AppWireResult<AppEventsResultDto> {
    request.validate().map_err(request_validation)?;
    let host_request = host.request(request.request_id).map_err(host_failure)?;
    let runtime_epoch = host_request.caller().runtime_epoch();
    let read = host_request
        .services()
        .read_conversation_events(
            host_request.caller(),
            floe_app::ReadConversationEvents {
                runtime_epoch: request.runtime_epoch,
                cursor: request.cursor,
                limit: request.limit,
            },
        )
        .map_err(service_error)?;
    Ok(match read {
        EventRead::Events {
            next_cursor,
            events,
        } => AppEventsResultDto::Events {
            runtime_epoch,
            next_cursor,
            events: events
                .into_iter()
                .map(|event| AppEventDto {
                    cursor: event.cursor,
                    aggregate_revision: event.aggregate_revision,
                    runtime_epoch,
                    event: match event.payload {
                        EventPayload::CommandUpdated {
                            command_id,
                            run_id,
                            session_revision,
                        } => AppEventKindDto::CommandUpdated {
                            receipt: AppCommandReceiptDto {
                                command_id: command_id.as_uuid(),
                                runtime_epoch,
                                admission: AppCommandStatusDto::Accepted,
                                run_id: Some(run_id.as_uuid()),
                                session_revision: Some(session_revision),
                                issue: None,
                            },
                        },
                        EventPayload::RunUpdated(run) => AppEventKindDto::RunUpdated {
                            run: run_event_snapshot(run, runtime_epoch),
                        },
                    },
                })
                .collect(),
        },
        EventRead::ResyncRequired { snapshot_cursor } => AppEventsResultDto::ResyncRequired {
            runtime_epoch,
            snapshot_cursor,
        },
    })
}

fn vault_result(result: floe_app::VaultLifecycleResult) -> floe_protocol::VaultLifecycleResultDto {
    floe_protocol::VaultLifecycleResultDto {
        operation_id: result.operation_id,
        done: result.done,
        state: result.state.map(crate::conversion::owners::vault_state_dto),
        failure: result.failure.as_ref().map(|failure| {
            crate::conversion::owners::failure_envelope(
                failure,
                &result.stage,
                &result.operation_id.to_string(),
            )
        }),
    }
}

fn structural_error(error: floe_protocol::ErrorDto) -> AppWireErrorDto {
    let mut metadata = error.metadata;
    let domain_code = serde_json::to_value(error.code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned));
    if let Some(code) = domain_code {
        metadata.insert("owner_code".into(), code);
    }
    AppWireErrorDto {
        code: match error.code {
            floe_protocol::ErrorCodeDto::NotFound => AppWireErrorCodeDto::NotFound,
            floe_protocol::ErrorCodeDto::Conflict => AppWireErrorCodeDto::Conflict,
            floe_protocol::ErrorCodeDto::Storage => AppWireErrorCodeDto::Unavailable,
            floe_protocol::ErrorCodeDto::Internal => AppWireErrorCodeDto::Internal,
            _ => AppWireErrorCodeDto::Validation,
        },
        message: error.message,
        field: error.field,
        metadata,
    }
}

fn day_error(error: floe_app::CoreError) -> AppWireErrorDto {
    structural_error(crate::bridge::core_error(error))
}

fn session_result(
    result: floe_app::ConversationSessionResult,
    operation_id: uuid::Uuid,
) -> AppWireResult<floe_protocol::ConversationSessionResultDto> {
    if result.operation_id != operation_id {
        return Err(service_error(floe_app::ServiceError::Internal));
    }
    Ok(floe_protocol::ConversationSessionResultDto {
        operation_id,
        done: result.done,
        state: result.state.map(crate::conversion::owners::vault_state_dto),
        session: result
            .session
            .as_ref()
            .map(floe_protocol::wire::protocol_payload)
            .transpose()
            .map_err(|_| service_error(floe_app::ServiceError::Internal))?,
        failure: result.failure.as_ref().map(|failure| {
            crate::conversion::owners::failure_envelope(
                failure,
                &result.stage,
                &operation_id.to_string(),
            )
        }),
    })
}

fn expert_result(
    result: floe_app::ExpertOperationResult,
    operation_id: uuid::Uuid,
) -> AppWireResult<floe_protocol::ExpertOperationResultDto> {
    if result.operation_id != operation_id {
        return Err(service_error(floe_app::ServiceError::Internal));
    }
    Ok(floe_protocol::ExpertOperationResultDto {
        operation_id,
        done: result.done,
        state: result.state.map(crate::conversion::owners::vault_state_dto),
        registry: result
            .registry
            .as_ref()
            .map(floe_protocol::wire::protocol_payload)
            .transpose()
            .map_err(|_| service_error(floe_app::ServiceError::Internal))?,
        calendar_experts: result
            .calendar_experts
            .as_ref()
            .map(floe_protocol::wire::protocol_payload)
            .transpose()
            .map_err(|_| service_error(floe_app::ServiceError::Internal))?,
        failure: result.failure.as_ref().map(|failure| {
            crate::conversion::owners::failure_envelope(
                failure,
                &result.stage,
                &operation_id.to_string(),
            )
        }),
    })
}

fn local_access_result(
    result: floe_app::LocalAccessResult,
    operation_id: uuid::Uuid,
) -> AppWireResult<floe_protocol::LocalAccessResultDto> {
    if result.operation_id != operation_id {
        return Err(service_error(floe_app::ServiceError::Internal));
    }
    Ok(floe_protocol::LocalAccessResultDto {
        operation_id,
        done: result.done,
        state: result.state.map(crate::conversion::owners::vault_state_dto),
        calendar_experts: result
            .calendar_experts
            .as_ref()
            .map(floe_protocol::wire::protocol_payload)
            .transpose()
            .map_err(|_| service_error(floe_app::ServiceError::Internal))?,
        calendar_subject_preview: result
            .calendar_subject_preview
            .map(crate::conversion::owners::subject_preview_dto),
        personal_access: result
            .personal_access
            .map(crate::conversion::owners::personal_access_dto),
        failure: result.failure.as_ref().map(|failure| {
            crate::conversion::owners::failure_envelope(
                failure,
                &result.stage,
                &operation_id.to_string(),
            )
        }),
    })
}

fn knowledge_result(
    result: floe_app::KnowledgeOperationResult,
    operation_id: uuid::Uuid,
) -> AppWireResult<floe_protocol::KnowledgeOperationResultDto> {
    if result.operation_id != operation_id {
        return Err(service_error(floe_app::ServiceError::Internal));
    }
    Ok(floe_protocol::KnowledgeOperationResultDto {
        operation_id,
        done: result.done,
        state: result.state.map(crate::conversion::owners::vault_state_dto),
        memory: result
            .memory
            .map(crate::conversion::owners::memory_dto)
            .transpose()
            .map_err(|_| service_error(floe_app::ServiceError::Internal))?,
        memory_review: result
            .memory_review
            .map(crate::conversion::owners::memory_review_dto)
            .transpose()
            .map_err(|_| service_error(floe_app::ServiceError::Internal))?,
        failure: result.failure.as_ref().map(|failure| {
            crate::conversion::owners::failure_envelope(
                failure,
                &result.stage,
                &operation_id.to_string(),
            )
        }),
    })
}

fn connections_result(
    result: floe_app::ConnectionsResult,
    operation_id: uuid::Uuid,
) -> AppWireResult<floe_protocol::ConnectionsResultDto> {
    if result.operation_id != operation_id {
        return Err(service_error(floe_app::ServiceError::Internal));
    }
    Ok(floe_protocol::ConnectionsResultDto {
        operation_id,
        done: result.done,
        state: result.state.map(crate::conversion::owners::vault_state_dto),
        connections: result
            .connections
            .as_ref()
            .map(|connections| {
                connections
                    .iter()
                    .map(floe_protocol::wire::protocol_payload)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()
            .map_err(|_| service_error(floe_app::ServiceError::Internal))?,
        failure: result.failure.as_ref().map(|failure| {
            crate::conversion::owners::failure_envelope(
                failure,
                &result.stage,
                &operation_id.to_string(),
            )
        }),
    })
}

fn action_result(
    result: floe_app::ActionOperationResult,
    operation_id: uuid::Uuid,
) -> AppWireResult<floe_protocol::ActionOperationResultDto> {
    if result.operation_id != operation_id {
        return Err(service_error(floe_app::ServiceError::Internal));
    }
    Ok(floe_protocol::ActionOperationResultDto {
        operation_id,
        done: result.done,
        state: result.state.map(crate::conversion::owners::vault_state_dto),
        calendar_actions: result
            .calendar_actions
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| service_error(floe_app::ServiceError::Internal))?,
        proposal: result.proposal.map(crate::conversion::owners::proposal_dto),
        failure: result.failure.as_ref().map(|failure| {
            crate::conversion::owners::failure_envelope(
                failure,
                &result.stage,
                &operation_id.to_string(),
            )
        }),
    })
}

fn command_receipt(receipt: &RunReceipt, runtime_epoch: u64) -> AppCommandReceiptDto {
    AppCommandReceiptDto {
        command_id: receipt.command_id.as_uuid(),
        runtime_epoch,
        admission: AppCommandStatusDto::Accepted,
        run_id: Some(receipt.run_id.as_uuid()),
        session_revision: Some(receipt.session_revision),
        issue: None,
    }
}

fn run_snapshot(receipt: RunReceipt, runtime_epoch: u64) -> AppRunSnapshotDto {
    let report = receipt.state.is_terminal().then(|| turn_report(&receipt));
    AppRunSnapshotDto {
        run_id: receipt.run_id.as_uuid(),
        session_id: receipt.session_id,
        revision: receipt.aggregate_revision,
        runtime_epoch,
        executor_generation: receipt.executor_generation,
        state: if receipt.state == RunState::Working {
            AppRunStateDto::Executing
        } else {
            AppRunStateDto::Finished
        },
        progress: match receipt.state {
            RunState::Working => "executing",
            RunState::Completed => "completed",
            RunState::Failed => "failed",
            RunState::Cancelled => "cancelled",
            RunState::TimedOut => "timed_out",
            RunState::Interrupted => "interrupted",
        }
        .into(),
        task_refs: Vec::new(),
        attempt_refs: Vec::new(),
        report,
    }
}

fn run_event_snapshot(run: RunEventRecord, runtime_epoch: u64) -> AppRunSnapshotDto {
    let report = run.state.is_terminal().then(|| AppTurnReportDto {
        execution: match run.state {
            RunState::Completed => AppTurnExecutionDto::Completed,
            RunState::Failed if run.generated_reply => AppTurnExecutionDto::Partial,
            RunState::Failed | RunState::TimedOut => AppTurnExecutionDto::Failed,
            RunState::Cancelled => AppTurnExecutionDto::Cancelled,
            RunState::Interrupted | RunState::Working => AppTurnExecutionDto::Indeterminate,
        },
        reply: if run.generated_reply {
            AppReplyStatusDto::Generated
        } else {
            AppReplyStatusDto::NotProduced
        },
        issues: run.issue.map(agent_failure).into_iter().collect(),
        action_refs: Vec::new(),
        final_message_ref: run.generated_reply.then(|| run.run_id.as_uuid()),
    });
    AppRunSnapshotDto {
        run_id: run.run_id.as_uuid(),
        session_id: run.session_id,
        revision: run.aggregate_revision,
        runtime_epoch,
        executor_generation: run.executor_generation,
        state: if run.state == RunState::Working {
            AppRunStateDto::Executing
        } else {
            AppRunStateDto::Finished
        },
        progress: match run.state {
            RunState::Working => "executing",
            RunState::Completed => "completed",
            RunState::Failed => "failed",
            RunState::Cancelled => "cancelled",
            RunState::TimedOut => "timed_out",
            RunState::Interrupted => "interrupted",
        }
        .into(),
        task_refs: Vec::new(),
        attempt_refs: Vec::new(),
        report,
    }
}

fn turn_report(receipt: &RunReceipt) -> AppTurnReportDto {
    let generated = receipt.output.is_some();
    AppTurnReportDto {
        execution: match receipt.state {
            RunState::Completed => AppTurnExecutionDto::Completed,
            RunState::Failed if generated => AppTurnExecutionDto::Partial,
            RunState::Failed | RunState::TimedOut => AppTurnExecutionDto::Failed,
            RunState::Cancelled => AppTurnExecutionDto::Cancelled,
            RunState::Interrupted => AppTurnExecutionDto::Indeterminate,
            RunState::Working => AppTurnExecutionDto::Indeterminate,
        },
        reply: if generated {
            AppReplyStatusDto::Generated
        } else {
            AppReplyStatusDto::NotProduced
        },
        issues: receipt.issue.map(agent_failure).into_iter().collect(),
        action_refs: Vec::new(),
        final_message_ref: generated.then(|| receipt.run_id.as_uuid()),
    }
}

pub(crate) fn validation(field: &'static str) -> AppWireErrorDto {
    wire_error(
        AppWireErrorCodeDto::Validation,
        "invalid app request",
        Some(field),
    )
}

pub(crate) fn request_validation(field: &'static str) -> AppWireErrorDto {
    if field == "schema_version" {
        wire_error(
            AppWireErrorCodeDto::UnsupportedVersion,
            "unsupported app wire version",
            Some(field),
        )
    } else {
        validation(field)
    }
}

pub(crate) fn host_failure(failure: floe_app::HostError) -> AppWireErrorDto {
    match failure {
        floe_app::HostError::InvalidIdentity => wire_error(
            AppWireErrorCodeDto::AccessDenied,
            "local caller identity is invalid",
            None,
        ),
        floe_app::HostError::InvalidRequest => validation("request_id"),
        floe_app::HostError::Closing | floe_app::HostError::Shutdown => wire_error(
            AppWireErrorCodeDto::Unavailable,
            "app host is unavailable",
            None,
        ),
        floe_app::HostError::UnsupportedCaller => wire_error(
            AppWireErrorCodeDto::UnsupportedVersion,
            "app wire is unavailable for this host",
            None,
        ),
        floe_app::HostError::IdentityUnavailable => wire_error(
            AppWireErrorCodeDto::Unavailable,
            "local caller identity is unavailable",
            None,
        ),
    }
}

pub(crate) fn service_error(failure: floe_app::ServiceError) -> AppWireErrorDto {
    let (code, message) = match failure {
        floe_app::ServiceError::InvalidInput => {
            (AppWireErrorCodeDto::Validation, "invalid app command")
        }
        floe_app::ServiceError::NotFound => (AppWireErrorCodeDto::NotFound, "record was not found"),
        floe_app::ServiceError::Conflict => (
            AppWireErrorCodeDto::Conflict,
            "app command conflicts with durable state",
        ),
        floe_app::ServiceError::AccessDenied => (
            AppWireErrorCodeDto::AccessDenied,
            "app command is not authorized",
        ),
        floe_app::ServiceError::Unavailable => (
            AppWireErrorCodeDto::Unavailable,
            "app command is unavailable",
        ),
        floe_app::ServiceError::Internal => (
            AppWireErrorCodeDto::Internal,
            "app command could not complete",
        ),
    };
    wire_error(code, message, None)
}

pub(crate) fn agent_failure(failure: AgentFailure) -> AppWireErrorDto {
    let code = match failure {
        AgentFailure::UnsupportedVersion => AppWireErrorCodeDto::UnsupportedVersion,
        AgentFailure::InvalidInput => AppWireErrorCodeDto::Validation,
        AgentFailure::NotFound => AppWireErrorCodeDto::NotFound,
        AgentFailure::Conflict => AppWireErrorCodeDto::Conflict,
        AgentFailure::PolicyDenied
        | AgentFailure::ConsentRequired
        | AgentFailure::CapabilityDenied
        | AgentFailure::AccessReviewRequired => AppWireErrorCodeDto::AccessDenied,
        AgentFailure::StorageUnavailable
        | AgentFailure::VaultUnavailable
        | AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::ServerModelRequestRejected
        | AgentFailure::CredentialExpired
        | AgentFailure::QuotaExceeded
        | AgentFailure::CapabilityUnavailable
        | AgentFailure::StaleContext
        | AgentFailure::BudgetExceeded
        | AgentFailure::Cancelled
        | AgentFailure::DeadlineExceeded
        | AgentFailure::Interrupted => AppWireErrorCodeDto::Unavailable,
        AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput
        | AgentFailure::Stalled => AppWireErrorCodeDto::Internal,
    };
    let mut error = wire_error(code, "app request could not complete", None);
    let reason_code = serde_json::to_value(failure)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".into());
    error.metadata.insert("reason_code".into(), reason_code);
    if matches!(
        failure,
        AgentFailure::ModelUnavailable
            | AgentFailure::LocalModelUnavailable
            | AgentFailure::ServerModelUnavailable
            | AgentFailure::ServerModelTimeout
            | AgentFailure::InvalidModelOutput
            | AgentFailure::LocalModelInvalidOutput
            | AgentFailure::ServerModelInvalidOutput
    ) {
        error
            .metadata
            .insert("recovery_action".into(), "retry_read".into());
    }
    error
}

pub(crate) fn internal_error() -> AppWireErrorDto {
    wire_error(
        AppWireErrorCodeDto::Internal,
        "app request could not complete",
        None,
    )
}

fn not_found() -> AppWireErrorDto {
    wire_error(AppWireErrorCodeDto::NotFound, "record was not found", None)
}

fn wire_error(
    code: AppWireErrorCodeDto,
    message: &'static str,
    field: Option<&'static str>,
) -> AppWireErrorDto {
    AppWireErrorDto {
        code,
        message: message.into(),
        field: field.map(str::to_owned),
        metadata: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    use super::*;

    #[derive(Clone, Default)]
    struct Services {
        captured: Arc<Mutex<Option<(Uuid, String, floe_app::StartTurn)>>>,
        cancelled: Arc<Mutex<Option<(Uuid, String, floe_app::CancelRun)>>>,
        queries: Arc<Mutex<Vec<(Uuid, String, floe_app::ReadConversation)>>>,
        events: Arc<Mutex<Vec<(Uuid, String, floe_app::ReadConversationEvents)>>>,
        receipt: Arc<Mutex<Option<RunReceipt>>>,
        event_read: Arc<Mutex<Option<EventRead>>>,
        failure: Arc<Mutex<Option<floe_app::ServiceError>>>,
        vault_calls: Arc<Mutex<Vec<(Uuid, String, Uuid)>>>,
        vault_result: Arc<Mutex<Option<floe_app::VaultLifecycleResult>>>,
    }

    impl Services {
        fn vault_result_for(
            &self,
            caller: &floe_app::CallerContext,
            operation_id: Uuid,
        ) -> floe_app::VaultLifecycleResult {
            self.vault_calls.lock().unwrap().push((
                caller.person_id(),
                caller.device_id().into(),
                operation_id,
            ));
            self.vault_result
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(floe_app::VaultLifecycleResult {
                    operation_id,
                    stage: "status".into(),
                    done: true,
                    state: Some(floe_app::VaultState::Missing),
                    failure: None,
                })
        }
    }

    impl floe_app::VaultLifecycleCommands for Services {
        fn vault_command(
            &self,
            caller: &floe_app::CallerContext,
            operation_id: Uuid,
            _command: floe_app::VaultLifecycleCommand,
        ) -> Result<floe_app::VaultLifecycleResult, floe_app::ServiceError> {
            Ok(self.vault_result_for(caller, operation_id))
        }
    }

    impl floe_app::ConversationSessionCommands for Services {
        fn session_command(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _command: floe_app::ConversationSessionCommand,
        ) -> Result<floe_app::ConversationSessionResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
    }

    impl floe_app::ExpertCommands for Services {
        fn expert_command(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _command: floe_app::ExpertCommand,
        ) -> Result<floe_app::ExpertOperationResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
    }

    impl floe_app::LocalAccessCommands for Services {
        fn local_access_command(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _command: floe_app::LocalAccessCommand,
        ) -> Result<floe_app::LocalAccessResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
    }

    impl floe_app::KnowledgeCommands for Services {
        fn decide_memory(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _decision: floe_app::MemoryReviewDecision,
        ) -> Result<floe_app::KnowledgeOperationResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
    }

    impl floe_app::DayCommands for Services {
        fn mutate_day(
            &self,
            _caller: &floe_app::CallerContext,
            _request: floe_app::DayMutationRequest,
        ) -> Result<floe_app::DayMutationResult, floe_app::CoreError> {
            Err(floe_app::CoreError::new(
                floe_app::ErrorCode::Storage,
                "test service unavailable",
            ))
        }
    }

    impl floe_app::ActionCommands for Services {
        fn action_command(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _command: floe_app::CalendarActionOperation,
        ) -> Result<floe_app::ActionOperationResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
    }

    impl floe_app::LocalContextCommands for Services {
        fn apply_local_context(
            &self,
            _caller: &floe_app::CallerContext,
            _command: floe_app::ContextCommand,
        ) -> Result<floe_app::LocalContextOutcome, AgentFailure> {
            Err(AgentFailure::CapabilityUnavailable)
        }
    }
    impl floe_app::LocalContextQueries for Services {
        fn query_local_context(
            &self,
            _caller: &floe_app::CallerContext,
            _query: floe_app::ContextQuery,
        ) -> Result<floe_app::LocalContextOutcome, AgentFailure> {
            Err(AgentFailure::CapabilityUnavailable)
        }
    }
    impl floe_app::ActionQueries for Services {
        fn inspect_actions(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _inspection: floe_app::ActionInspection,
        ) -> Result<floe_app::ActionOperationResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
        fn read_action_result(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _release: bool,
        ) -> Result<floe_app::ActionOperationResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
    }
    impl floe_app::DayQueries for Services {
        fn read_day(
            &self,
            _caller: &floe_app::CallerContext,
            _request: floe_app::DayRead,
        ) -> Result<floe_app::DaySnapshot, floe_app::CoreError> {
            Err(floe_app::CoreError::new(
                floe_app::ErrorCode::Storage,
                "test service unavailable",
            ))
        }
    }
    impl floe_app::KnowledgeQueries for Services {
        fn inspect_knowledge(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _inspection: floe_app::KnowledgeInspection,
        ) -> Result<floe_app::KnowledgeOperationResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
        fn read_knowledge_result(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _release: bool,
        ) -> Result<floe_app::KnowledgeOperationResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
    }
    impl floe_app::ConnectionsQueries for Services {
        fn inspect_connections(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
        ) -> Result<floe_app::ConnectionsResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
        fn read_connections_result(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _release: bool,
        ) -> Result<floe_app::ConnectionsResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
    }

    impl floe_app::LocalAccessQueries for Services {
        fn inspect_local_access(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _inspection: floe_app::LocalAccessInspection,
        ) -> Result<floe_app::LocalAccessResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
        fn read_local_access_result(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _release: bool,
        ) -> Result<floe_app::LocalAccessResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
    }

    impl floe_app::ExpertQueries for Services {
        fn inspect_experts(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _inspection: floe_app::ExpertInspection,
        ) -> Result<floe_app::ExpertOperationResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
        fn read_expert_result(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _release: bool,
        ) -> Result<floe_app::ExpertOperationResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
    }

    impl floe_app::ConversationSessionQueries for Services {
        fn get_session(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _session_id: Uuid,
        ) -> Result<floe_app::ConversationSessionResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
        fn read_session_result(
            &self,
            _caller: &floe_app::CallerContext,
            _operation_id: Uuid,
            _release: bool,
        ) -> Result<floe_app::ConversationSessionResult, floe_app::ServiceError> {
            Err(floe_app::ServiceError::Unavailable)
        }
    }

    impl floe_app::VaultLifecycleQueries for Services {
        fn vault_status(
            &self,
            caller: &floe_app::CallerContext,
            operation_id: Uuid,
        ) -> Result<floe_app::VaultLifecycleResult, floe_app::ServiceError> {
            Ok(self.vault_result_for(caller, operation_id))
        }

        fn read_vault_result(
            &self,
            caller: &floe_app::CallerContext,
            operation_id: Uuid,
            _release: bool,
        ) -> Result<floe_app::VaultLifecycleResult, floe_app::ServiceError> {
            Ok(self.vault_result_for(caller, operation_id))
        }
    }

    #[test]
    fn vault_services_admit_identity_and_reject_wrong_operation_results() {
        let person_id = Uuid::new_v4();
        let operation_id = Uuid::new_v4();
        let services = Services::default();
        let host = floe_app::AppHost::bootstrap_claim(
            services.clone(),
            floe_app::LocalIdentityClaim {
                person_id,
                device_id: "verified-device".into(),
            },
        )
        .unwrap();
        let command = || AppCommandRequestDto {
            schema_version: 2,
            request_id: Uuid::new_v4(),
            command_id: operation_id,
            command: AppCommandDto::VaultCreate {},
        };
        assert!(
            matches!(command_with_host(&host, command()).unwrap(), AppCommandResultDto::VaultOperation { result } if result.operation_id == operation_id)
        );
        assert_eq!(
            services.vault_calls.lock().unwrap().as_slice(),
            &[(person_id, "verified-device".into(), operation_id)]
        );
        *services.vault_result.lock().unwrap() = Some(floe_app::VaultLifecycleResult {
            operation_id: Uuid::new_v4(),
            stage: "create".into(),
            done: true,
            state: Some(floe_app::VaultState::Ready),
            failure: None,
        });
        assert!(command_with_host(&host, command()).is_err());
        assert!(
            query_with_host(
                &host,
                AppQueryRequestDto {
                    schema_version: 2,
                    request_id: Uuid::new_v4(),
                    query: AppQueryDto::VaultReadResult {
                        operation_id,
                        release: true
                    },
                }
            )
            .is_err()
        );
    }

    impl floe_app::ConversationQueries for Services {
        fn read_conversation(
            &self,
            caller: &floe_app::CallerContext,
            request: floe_app::ReadConversation,
        ) -> Result<Option<RunReceipt>, floe_app::ServiceError> {
            request.validate()?;
            self.queries.lock().unwrap().push((
                caller.person_id(),
                caller.device_id().into(),
                request,
            ));
            if let Some(failure) = *self.failure.lock().unwrap() {
                return Err(failure);
            }
            Ok(self.receipt.lock().unwrap().clone())
        }
    }

    impl floe_app::ConversationEvents for Services {
        fn read_conversation_events(
            &self,
            caller: &floe_app::CallerContext,
            request: floe_app::ReadConversationEvents,
        ) -> Result<EventRead, floe_app::ServiceError> {
            request.validate()?;
            self.events.lock().unwrap().push((
                caller.person_id(),
                caller.device_id().into(),
                request,
            ));
            if let Some(failure) = *self.failure.lock().unwrap() {
                return Err(failure);
            }
            Ok(self
                .event_read
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(EventRead::ResyncRequired { snapshot_cursor: 4 }))
        }
    }

    fn receipt(person_id: Uuid) -> RunReceipt {
        RunReceipt {
            run_id: floe_app::RunId::new(),
            command_id: floe_app::CommandId::new(),
            session_id: Uuid::new_v4(),
            principal: person_id.to_string(),
            request_digest: [1; 32],
            state: RunState::Completed,
            output: Some("answer".into()),
            coverage: serde_json::from_str("\"independent\"").unwrap(),
            issue: None,
            session_revision: 2,
            aggregate_revision: 3,
            executor_generation: 1,
            continuation_of: None,
            continuation_executor_generation: None,
            continuation_level: 0,
            retry_of: None,
            profile: floe_app::ProfileSelection::Auto,
        }
    }

    #[test]
    fn query_services_receive_verified_identity_and_preserve_owner_results() {
        let person_id = Uuid::new_v4();
        let services = Services::default();
        let receipt = receipt(person_id);
        *services.receipt.lock().unwrap() = Some(receipt.clone());
        let host = floe_app::AppHost::bootstrap_claim(
            services.clone(),
            floe_app::LocalIdentityClaim {
                person_id,
                device_id: "mac-local".into(),
            },
        )
        .unwrap();
        let query = |query| {
            query_with_host(
                &host,
                AppQueryRequestDto {
                    schema_version: floe_protocol::APP_WIRE_VERSION,
                    request_id: Uuid::new_v4(),
                    query,
                },
            )
        };
        let command_id = receipt.command_id.as_uuid();
        let run_id = receipt.run_id.as_uuid();
        assert!(
            matches!(query(AppQueryDto::ConversationGetCommand { command_id }).unwrap(),
            AppQueryResultDto::CommandReceipt { receipt } if receipt.command_id == command_id)
        );
        assert!(
            matches!(query(AppQueryDto::ConversationGetRun { run_id }).unwrap(),
            AppQueryResultDto::RunSnapshot { run } if run.run_id == run_id)
        );
        assert!(
            matches!(query(AppQueryDto::ConversationGetMessage { message_id: run_id }).unwrap(),
            AppQueryResultDto::Message { message } if message.message_id == run_id && message.text == "answer")
        );
        assert_eq!(
            *services.queries.lock().unwrap(),
            vec![
                (
                    person_id,
                    "mac-local".into(),
                    floe_app::ReadConversation::Command { command_id }
                ),
                (
                    person_id,
                    "mac-local".into(),
                    floe_app::ReadConversation::Run { run_id }
                ),
                (
                    person_id,
                    "mac-local".into(),
                    floe_app::ReadConversation::Message { message_id: run_id }
                ),
            ]
        );
        *services.receipt.lock().unwrap() = None;
        assert!(
            matches!(query(AppQueryDto::ConversationGetCommand { command_id }).unwrap(), AppQueryResultDto::UnknownCommand { command_id: actual } if actual == command_id)
        );
        assert_eq!(
            query(AppQueryDto::ConversationGetRun { run_id })
                .unwrap_err()
                .code,
            AppWireErrorCodeDto::NotFound
        );
        *services.failure.lock().unwrap() = Some(floe_app::ServiceError::AccessDenied);
        assert_eq!(
            query(AppQueryDto::ConversationGetRun { run_id })
                .unwrap_err()
                .code,
            AppWireErrorCodeDto::AccessDenied
        );
        let calls = services.queries.lock().unwrap().len();
        assert_eq!(
            query(AppQueryDto::ConversationGetRun {
                run_id: Uuid::nil()
            })
            .unwrap_err()
            .code,
            AppWireErrorCodeDto::Validation
        );
        assert_eq!(services.queries.lock().unwrap().len(), calls);
        assert!(services.cancelled.lock().unwrap().is_none());
    }

    #[test]
    fn event_service_preserves_cursor_identity_values_and_errors() {
        let person_id = Uuid::new_v4();
        let services = Services::default();
        let host = floe_app::AppHost::bootstrap_claim(
            services.clone(),
            floe_app::LocalIdentityClaim {
                person_id,
                device_id: "mac-local".into(),
            },
        )
        .unwrap();
        let read = |runtime_epoch, cursor, limit| {
            events_with_host(
                &host,
                AppEventsRequestDto {
                    schema_version: floe_protocol::APP_WIRE_VERSION,
                    request_id: Uuid::new_v4(),
                    runtime_epoch,
                    cursor,
                    limit,
                },
            )
        };
        let AppEventsResultDto::ResyncRequired {
            runtime_epoch,
            snapshot_cursor,
        } = read(None, None, 2).unwrap()
        else {
            panic!("expected resync");
        };
        assert_eq!(snapshot_cursor, 4);
        let receipt = receipt(person_id);
        *services.event_read.lock().unwrap() = Some(EventRead::Events {
            next_cursor: 5,
            events: vec![floe_app::ConversationEvent {
                cursor: 5,
                aggregate_revision: 3,
                payload: EventPayload::CommandUpdated {
                    command_id: receipt.command_id,
                    run_id: receipt.run_id,
                    session_revision: 2,
                },
            }],
        });
        assert!(
            matches!(read(Some(runtime_epoch), Some(4), 1).unwrap(), AppEventsResultDto::Events {
            next_cursor: 5, events, ..
        } if events.len() == 1 && events[0].cursor == 5 && events[0].runtime_epoch == runtime_epoch)
        );
        let captured = services.events.lock().unwrap().clone();
        assert_eq!(
            captured[1],
            (
                person_id,
                "mac-local".into(),
                floe_app::ReadConversationEvents {
                    runtime_epoch: Some(runtime_epoch),
                    cursor: Some(4),
                    limit: 1
                }
            )
        );
        assert_eq!(
            read(Some(runtime_epoch), Some(4), 0).unwrap_err().code,
            AppWireErrorCodeDto::Validation
        );
        assert_eq!(services.events.lock().unwrap().len(), captured.len());
        *services.failure.lock().unwrap() = Some(floe_app::ServiceError::Unavailable);
        assert_eq!(
            read(None, None, 1).unwrap_err().code,
            AppWireErrorCodeDto::Unavailable
        );
        assert!(services.cancelled.lock().unwrap().is_none());
    }

    impl floe_app::HostServices for Services {
        fn shutdown(&self) -> Result<(), floe_app::HostError> {
            Ok(())
        }
    }

    impl floe_app::ConversationCommands for Services {
        fn start_turn(
            &self,
            caller: &floe_app::CallerContext,
            request: floe_app::StartTurn,
        ) -> Result<floe_app::CommandReceipt, floe_app::ServiceError> {
            *self.captured.lock().unwrap() = Some((
                caller.person_id(),
                caller.device_id().to_owned(),
                request.clone(),
            ));
            Ok(floe_app::CommandReceipt {
                command_id: request.command_id,
                run_id: Uuid::new_v4(),
                session_revision: request.expected_revision + 1,
            })
        }

        fn cancel_run(
            &self,
            caller: &floe_app::CallerContext,
            request: floe_app::CancelRun,
        ) -> Result<floe_app::CancelRunReceipt, floe_app::ServiceError> {
            *self.cancelled.lock().unwrap() = Some((
                caller.person_id(),
                caller.device_id().to_owned(),
                request.clone(),
            ));
            Ok(floe_app::CancelRunReceipt {
                command_id: request.command_id,
                run_id: request.run_id,
                outcome: floe_app::CancelRunOutcome::Accepted,
            })
        }
    }

    #[test]
    fn start_turn_uses_verified_host_identity_and_returns_durable_receipt() {
        let person_id = Uuid::new_v4();
        let services = Services::default();
        let host = floe_app::AppHost::bootstrap_claim(
            services.clone(),
            floe_app::LocalIdentityClaim {
                person_id,
                device_id: "mac-local".into(),
            },
        )
        .unwrap();
        let command_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        let retry_of = Uuid::new_v4();
        let result = command_with_host(
            &host,
            AppCommandRequestDto {
                schema_version: floe_protocol::APP_WIRE_VERSION,
                request_id: Uuid::new_v4(),
                command_id,
                command: AppCommandDto::ConversationStartTurn {
                    session_id,
                    expected_revision: 7,
                    text: "\thello\t".into(),
                    mode: AppTurnModeDto::NewTurn {},
                    profile: AppProfileSelectionDto::Explicit {
                        profile_id: "local-fast".into(),
                    },
                    retry_of: Some(retry_of),
                },
            },
        )
        .unwrap();

        let AppCommandResultDto::CommandReceipt { receipt } = result else {
            panic!("expected command receipt");
        };
        assert_eq!(receipt.command_id, command_id);
        assert_eq!(receipt.session_revision, Some(8));
        let (captured_person, captured_device, captured) =
            services.captured.lock().unwrap().clone().unwrap();
        assert_eq!(captured_person, person_id);
        assert_eq!(captured_device, "mac-local");
        assert_eq!(captured.session_id, session_id);
        assert_eq!(captured.text, "hello");
        assert_eq!(captured.retry_of, Some(retry_of));
        assert_eq!(
            captured.profile,
            floe_app::ProfileSelection::Explicit("local-fast".into())
        );
    }

    #[test]
    fn cancel_run_uses_verified_host_identity() {
        let person_id = Uuid::new_v4();
        let services = Services::default();
        let host = floe_app::AppHost::bootstrap_claim(
            services.clone(),
            floe_app::LocalIdentityClaim {
                person_id,
                device_id: "mac-local".into(),
            },
        )
        .unwrap();
        let command_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let result = command_with_host(
            &host,
            AppCommandRequestDto {
                schema_version: floe_protocol::APP_WIRE_VERSION,
                request_id: Uuid::new_v4(),
                command_id,
                command: AppCommandDto::ConversationCancelRun {
                    run_id,
                    reason: floe_protocol::AppCancelRunReasonDto::UserRequested,
                },
            },
        )
        .unwrap();

        assert!(matches!(
            result,
            AppCommandResultDto::CancelRunReceipt {
                command_id,
                run_id,
                runtime_epoch,
                outcome: AppCancelRunOutcomeDto::Accepted,
            } if runtime_epoch > 0
        ));
        let (captured_person, captured_device, captured) =
            services.cancelled.lock().unwrap().clone().unwrap();
        assert_eq!(captured_person, person_id);
        assert_eq!(captured_device, "mac-local");
        assert_eq!(captured.run_id, run_id);
    }

    #[test]
    fn terminal_model_issue_exposes_safe_retry_classification() {
        let error = agent_failure(AgentFailure::ServerModelUnavailable);
        assert_eq!(
            error.metadata.get("reason_code").map(String::as_str),
            Some("server_model_unavailable")
        );
        assert_eq!(
            error.metadata.get("recovery_action").map(String::as_str),
            Some("retry_read")
        );
        assert!(
            !agent_failure(AgentFailure::Cancelled)
                .metadata
                .contains_key("recovery_action")
        );
    }
}
