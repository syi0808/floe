use crate::bridge::FloeHandle;
use floe_app::{
    ActionCommands, ActionQueries, DayCommands, DayQueries, ExpertCommands, ExpertQueries,
    KnowledgeCommands, KnowledgeQueries, NativeHostCommands, NativeHostQueries,
    VaultLifecycleCommands, VaultLifecycleQueries,
};
use floe_kernel::AgentFailure;
use floe_protocol::*;
use std::{collections::BTreeMap, time::Duration};
pub(crate) type AppWireResult<T> = Result<T, AppWireErrorDto>;

pub(crate) fn command(
    handle: &FloeHandle,
    request: AppCommandRequestDto,
) -> AppWireResult<AppCommandResultDto> {
    request.validate().map_err(request_validation)?;
    let host = handle.app();
    let host_request = host
        .request(request.request_id.get())
        .map_err(host_failure)?;
    let caller = host_request.caller();
    let services = host_request.services();
    let command_id = request.command_id.get();
    let command = match request.command {
        AppCommandDto::NativeHost(command) => {
            let result = services
                .apply_native_host(
                    caller,
                    crate::context_wire::command(command).map_err(structural_error)?,
                )
                .map_err(agent_failure)?;
            return crate::conversion::native::native_host_command_result(result)
                .map_err(structural_error);
        }
        AppCommandDto::Product(command) => command,
    };
    if crate::connections_wire::handles_command(&command)
        || crate::conversation_wire::handles_command(&command)
    {
        let owners = services.ready_owners(caller).map_err(|failure| {
            if crate::conversation_wire::handles_command(&command) {
                crate::conversation_wire::failure_dto(failure, command_id)
            } else {
                agent_failure(failure)
            }
        })?;
        let actor = caller.owner_actor();
        let scope = floe_app::host_scope(
            command_id,
            floe_execution::Cancellation::default(),
            Duration::from_secs(35),
        );
        return services.execute_owner(async {
            if crate::connections_wire::handles_command(&command) {
                crate::connections_wire::command(&owners, &actor, command_id, command, &scope).await
            } else {
                crate::conversation_wire::command(&owners, &actor, command_id, command, &scope)
                    .await
            }
        });
    }
    match command {
        AppProductCommandDto::ActionsCalendar { operation } => {
            let command = crate::conversion::owners::calendar_action_operation(operation)
                .map_err(structural_error)?;
            let result = host_request
                .services()
                .action_command(host_request.caller(), command_id, command)
                .map_err(service_error)?;
            Ok(AppCommandResultDto::ActionOperation {
                result: action_result(result, command_id)?,
            })
        }
        AppProductCommandDto::DayMutate { day, mutation } => {
            let result = host_request
                .services()
                .mutate_day(
                    host_request.caller(),
                    floe_app::DayMutationRequest {
                        command_id: command_id,
                        day: crate::day_wire::read(day).map_err(structural_error)?,
                        mutation: crate::day_wire::mutation(mutation).map_err(structural_error)?,
                    },
                )
                .map_err(day_error)?;
            if result.command_id != command_id {
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
        AppProductCommandDto::KnowledgeMemoryDecide {
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
                .decide_memory(host_request.caller(), command_id, decision)
                .map_err(service_error)?;
            Ok(AppCommandResultDto::KnowledgeOperation {
                result: knowledge_result(result, command_id)?,
            })
        }
        AppProductCommandDto::ExpertsRegistryConfigure { change } => {
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
                    },
                });
            let result = host_request
                .services()
                .expert_command(host_request.caller(), command_id, command)
                .map_err(service_error)?;
            Ok(AppCommandResultDto::ExpertOperation {
                result: expert_result(result, command_id)?,
            })
        }
        AppProductCommandDto::ExpertsBindingReplace { selection } => {
            let result = host_request
                .services()
                .expert_command(
                    host_request.caller(),
                    command_id,
                    floe_app::ExpertCommand::ReplaceBinding(
                        floe_app::ExpertBindingSelectionIntent {
                            assignment_id: selection.assignment_id,
                            package_id: selection.package_id,
                            package_version: selection.package_version,
                            definition_revision: selection.definition_revision,
                            requirement_key: selection.requirement_key,
                            expected_binding_revision: selection.expected_binding_revision,
                            candidate_ids: selection.candidate_ids,
                        },
                    ),
                )
                .map_err(service_error)?;
            Ok(AppCommandResultDto::ExpertOperation {
                result: expert_result(result, command_id)?,
            })
        }
        AppProductCommandDto::VaultCreate {}
        | AppProductCommandDto::VaultUnlock {}
        | AppProductCommandDto::VaultLock {} => {
            let command = match command {
                AppProductCommandDto::VaultCreate {} => floe_app::VaultLifecycleCommand::Create,
                AppProductCommandDto::VaultUnlock {} => floe_app::VaultLifecycleCommand::Unlock,
                AppProductCommandDto::VaultLock {} => floe_app::VaultLifecycleCommand::Lock,
                _ => unreachable!(),
            };
            let result = host_request
                .services()
                .vault_command(host_request.caller(), command_id, command)
                .map_err(service_error)?;
            if result.operation_id != command_id {
                return Err(service_error(floe_app::ServiceError::Internal));
            }
            Ok(AppCommandResultDto::VaultOperation {
                result: vault_result(result),
            })
        }
        AppProductCommandDto::DayRefresh { .. } => {
            Err(agent_failure(AgentFailure::CapabilityUnavailable))
        }
        _ => Err(validation("command")),
    }
}
pub(crate) fn query(
    handle: &FloeHandle,
    request: AppQueryRequestDto,
) -> AppWireResult<AppQueryResultDto> {
    request.validate().map_err(request_validation)?;
    let host = handle.app();
    let request_id = request.request_id.get();
    let host_request = host.request(request_id).map_err(host_failure)?;
    let caller = host_request.caller();
    let services = host_request.services();
    let query = match request.query {
        AppQueryDto::NativeHost(query) => {
            let result = services
                .query_native_host(
                    caller,
                    crate::context_wire::query(query).map_err(structural_error)?,
                )
                .map_err(agent_failure)?;
            return crate::conversion::native::native_host_query_result(result)
                .map_err(structural_error);
        }
        AppQueryDto::Product(query) => query,
    };
    if crate::connections_wire::handles_query(&query)
        || crate::conversation_wire::handles_query(&query)
    {
        let owners = services.ready_owners(caller).map_err(|failure| {
            if crate::conversation_wire::handles_query(&query) {
                crate::conversation_wire::failure_dto(failure, request_id)
            } else {
                agent_failure(failure)
            }
        })?;
        let actor = caller.owner_actor();
        let scope = floe_app::host_scope(
            request_id,
            floe_execution::Cancellation::default(),
            Duration::from_secs(35),
        );
        return services.execute_owner(async {
            if crate::connections_wire::handles_query(&query) {
                crate::connections_wire::query(&owners, &actor, query, &scope).await
            } else {
                crate::conversation_wire::query(&owners, &actor, query, &scope).await
            }
        });
    }
    match query {
        AppProductQueryDto::ActionsCapabilities {}
        | AppProductQueryDto::ActionsAuthority {}
        | AppProductQueryDto::ActionsList {}
        | AppProductQueryDto::ActionsGet { .. }
        | AppProductQueryDto::ActionsProposalInspect { .. } => {
            let inspection = match query {
                AppProductQueryDto::ActionsCapabilities {} => {
                    floe_app::ActionInspection::Capabilities
                }
                AppProductQueryDto::ActionsAuthority {} => floe_app::ActionInspection::Authority,
                AppProductQueryDto::ActionsList {} => floe_app::ActionInspection::List,
                AppProductQueryDto::ActionsGet { action_id } => {
                    floe_app::ActionInspection::Get { action_id }
                }
                AppProductQueryDto::ActionsProposalInspect {
                    session_id,
                    invocation_id,
                } => floe_app::ActionInspection::Proposal {
                    session_id,
                    invocation_id,
                },
                _ => unreachable!(),
            };
            let result = services
                .inspect_actions(caller, request_id, inspection)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::ActionOperation {
                result: action_result(result, request_id)?,
            })
        }
        AppProductQueryDto::ActionsReadResult {
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
        AppProductQueryDto::DaySnapshot { day } => {
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
        AppProductQueryDto::KnowledgeMemoryOverview {}
        | AppProductQueryDto::KnowledgeMemoryReview {} => {
            let inspection = if matches!(
                request.query,
                AppProductQueryDto::KnowledgeMemoryOverview {}
            ) {
                floe_app::KnowledgeInspection::Memory
            } else {
                floe_app::KnowledgeInspection::Review
            };
            let result = services
                .inspect_knowledge(caller, request_id, inspection)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::KnowledgeOperation {
                result: knowledge_result(result, request_id)?,
            })
        }
        AppProductQueryDto::KnowledgeReadResult {
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
        AppProductQueryDto::ExpertsRegistryInspect {} => {
            let inspection = floe_app::ExpertInspection::Registry;
            let result = services
                .inspect_experts(caller, request_id, inspection)
                .map_err(service_error)?;
            Ok(AppQueryResultDto::ExpertOperation {
                result: expert_result(result, request_id)?,
            })
        }
        AppProductQueryDto::ExpertsSourceCandidates {
            assignment_id,
            requirement_key,
        } => {
            let result = services
                .inspect_experts(
                    caller,
                    request_id,
                    floe_app::ExpertInspection::Candidates {
                        assignment_id,
                        requirement_key,
                    },
                )
                .map_err(service_error)?;
            Ok(AppQueryResultDto::ExpertOperation {
                result: expert_result(result, request_id)?,
            })
        }
        AppProductQueryDto::ExpertsReadResult {
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
        AppProductQueryDto::VaultStatus {} => {
            let result = services
                .vault_status(caller, request_id)
                .map_err(service_error)?;
            if result.operation_id != request_id {
                return Err(service_error(floe_app::ServiceError::Internal));
            }
            Ok(AppQueryResultDto::VaultOperation {
                result: vault_result(result),
            })
        }
        AppProductQueryDto::VaultReadResult {
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
        AppProductQueryDto::DayRefreshGet { .. } => {
            Err(agent_failure(AgentFailure::CapabilityUnavailable))
        }
        _ => Err(validation("query")),
    }
}
pub(crate) fn events(
    handle: &FloeHandle,
    request: AppEventsRequestDto,
) -> AppWireResult<AppEventsResultDto> {
    request.validate().map_err(request_validation)?;
    let host = handle.app();
    let host_request = host
        .request(request.request_id.get())
        .map_err(host_failure)?;
    let caller = host_request.caller();
    let services = host_request.services();
    let owners = services.ready_owners(caller).map_err(|failure| {
        crate::conversation_wire::failure_dto(failure, request.request_id.get())
    })?;
    let actor = caller.owner_actor();
    let scope = floe_app::host_scope(
        request.request_id.get(),
        floe_execution::Cancellation::default(),
        Duration::from_secs(5),
    );
    services.execute_owner(crate::conversation_wire::events(
        &owners, &actor, request, &scope,
    ))
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
        owner_failure: None,
    }
}

fn day_error(error: floe_app::CoreError) -> AppWireErrorDto {
    structural_error(crate::bridge::core_error(error))
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
        candidates: result
            .candidates
            .map(|catalog| floe_protocol::ExpertCandidateCatalogDto {
                assignment_id: catalog.assignment_id,
                requirement_key: catalog.requirement_key,
                binding_revision: catalog.binding_revision,
                candidates: catalog
                    .candidates
                    .into_iter()
                    .map(|candidate| floe_protocol::ExpertSourceCandidateDto {
                        candidate_id: candidate.candidate_id,
                        title: candidate.title,
                        detail: candidate.detail,
                        availability: candidate.availability,
                        selected: candidate.selected,
                    })
                    .collect(),
            }),
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
        owner_failure: None,
    }
}
