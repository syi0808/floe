use crate::bridge::FloeHandle;
use floe_app::{
    DayCommands, DayQueries, NativeHostCommands, NativeHostQueries, VaultLifecycleCommands,
    VaultLifecycleQueries,
};
use floe_kernel::AgentFailure;
use floe_protocol::*;
use std::{collections::BTreeMap, time::Duration};
pub(crate) type AppWireResult<T> = Result<T, AppWireErrorDto>;
pub(crate) type AppCommandResult<T> = Result<T, AppCommandFailure>;

/// A command error carries admission evidence independently of its cause.
pub(crate) enum AppCommandFailure {
    NotAdmitted(AppWireErrorDto),
    NotApplied(AppWireErrorDto),
    Admitted(AppWireErrorDto),
    Indeterminate(AppWireErrorDto),
}

impl From<AppWireErrorDto> for AppCommandFailure {
    fn from(error: AppWireErrorDto) -> Self {
        Self::Indeterminate(error)
    }
}

impl AppCommandFailure {
    pub(crate) fn into_parts(self) -> (AppCommandDispositionDto, AppWireErrorDto) {
        match self {
            Self::NotApplied(error) => (AppCommandDispositionDto::NotApplied, error),
            Self::NotAdmitted(error) => (AppCommandDispositionDto::NotAdmitted, error),
            Self::Admitted(error) => (AppCommandDispositionDto::Admitted, error),
            Self::Indeterminate(error) => (AppCommandDispositionDto::Indeterminate, error),
        }
    }
}

pub(crate) fn command(
    handle: &FloeHandle,
    request: AppCommandRequestDto,
) -> AppCommandResult<AppCommandResultDto> {
    request
        .validate()
        .map_err(|field| AppCommandFailure::NotAdmitted(request_validation(field)))?;
    let host = handle.app();
    let host_request = host
        .request(request.request_id.get())
        .map_err(|failure| AppCommandFailure::NotAdmitted(host_failure(failure)))?;
    let caller = host_request.caller();
    let services = host_request.services();
    let command_id = request.command_id.get();
    let command = match request.command {
        AppCommandDto::NativeHost(command) => {
            let result = services
                .apply_native_host(
                    caller,
                    crate::context_wire::command(command)
                        .map_err(|error| AppCommandFailure::NotAdmitted(structural_error(error)))?,
                )
                .map_err(agent_failure)?;
            return crate::conversion::native::native_host_command_result(result)
                .map_err(|error| structural_error(error).into());
        }
        AppCommandDto::Product(command) => command,
    };
    if crate::connections_wire::handles_command(&command)
        || crate::conversation_wire::handles_command(&command)
    {
        let owners = services.ready_owners(caller).map_err(|failure| {
            AppCommandFailure::NotAdmitted(if crate::conversation_wire::handles_command(&command) {
                crate::conversation_wire::failure_dto(failure, command_id)
            } else {
                agent_failure(failure)
            })
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
                    .map_err(Into::into)
            }
        });
    }
    if crate::actions_wire::handles_command(&command) {
        return crate::actions_wire::command(services, caller, command_id, command)
            .map_err(Into::into);
    }
    if crate::experts_wire::handles_command(&command) {
        return crate::experts_wire::command(services, caller, command_id, command)
            .map_err(Into::into);
    }
    if crate::knowledge_wire::handles_command(&command) {
        return crate::knowledge_wire::command(services, caller, command_id, command)
            .map_err(Into::into);
    }
    match command {
        AppProductCommandDto::DayMutate { day, mutation } => {
            let result = host_request
                .services()
                .mutate_day(
                    host_request.caller(),
                    floe_app::DayMutationRequest {
                        command_id: command_id,
                        day: crate::day_wire::read(day).map_err(|error| {
                            AppCommandFailure::NotAdmitted(structural_error(error))
                        })?,
                        mutation: crate::day_wire::mutation(mutation).map_err(|error| {
                            AppCommandFailure::NotAdmitted(structural_error(error))
                        })?,
                    },
                )
                .map_err(day_error)?;
            if result.command_id != command_id {
                return Err(internal_error().into());
            }
            Ok(AppCommandResultDto::DayMutation {
                command_id: result.command_id,
                mutation: floe_protocol::MutationResultDto {
                    snapshot: crate::conversion::day_snapshot_to_dto(result.snapshot).map_err(
                        |failure| structural_error(floe_protocol::wire::conversion_error(failure)),
                    )?,
                    changed_item: result
                        .changed_item
                        .map(crate::conversion::timeline_item_to_dto)
                        .transpose()
                        .map_err(|failure| {
                            structural_error(floe_protocol::wire::conversion_error(failure))
                        })?,
                    capture: result.capture.map(crate::conversion::capture_to_dto),
                },
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
                .map_err(agent_failure)?;
            if result.operation_id != command_id {
                return Err(internal_error().into());
            }
            Ok(AppCommandResultDto::VaultOperation {
                result: vault_result(result),
            })
        }
        AppProductCommandDto::DayRefresh { day } => {
            let refresh = services
                .refresh_day(
                    caller,
                    command_id,
                    crate::day_wire::read(day)
                        .map_err(|error| AppCommandFailure::NotAdmitted(structural_error(error)))?,
                )
                .map_err(day_error)?;
            Ok(AppCommandResultDto::DayRefresh {
                refresh: crate::day_wire::refresh(refresh).map_err(structural_error)?,
            })
        }
        _ => Err(AppCommandFailure::NotAdmitted(validation("command"))),
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
    if crate::actions_wire::handles_query(&query) {
        return crate::actions_wire::query(services, caller, request_id, query);
    }
    if crate::experts_wire::handles_query(&query) {
        return crate::experts_wire::query(services, caller, request_id, query);
    }
    if crate::knowledge_wire::handles_query(&query) {
        return crate::knowledge_wire::query(services, caller, request_id, query);
    }
    match query {
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
        AppProductQueryDto::VaultStatus {} => {
            let result = services
                .vault_status(caller, request_id)
                .map_err(agent_failure)?;
            if result.operation_id != request_id {
                return Err(internal_error());
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
                .map_err(agent_failure)?;
            if result.operation_id != operation_id {
                return Err(internal_error());
            }
            Ok(AppQueryResultDto::VaultOperation {
                result: vault_result(result),
            })
        }
        AppProductQueryDto::DayRefreshGet { operation_ref } => {
            let refresh = services
                .get_day_refresh(caller, operation_ref.get())
                .map_err(day_error)?;
            Ok(AppQueryResultDto::DayRefresh {
                refresh: crate::day_wire::refresh(refresh).map_err(structural_error)?,
            })
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
        failure: result.failure_projection().map(|failure| {
            crate::conversion::owners::failure_envelope(
                failure,
                &result.stage,
                &result.operation_id.to_string(),
            )
        }),
    }
}

pub(crate) fn structural_error(error: floe_protocol::ErrorDto) -> AppWireErrorDto {
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
        AgentFailure::IncompleteCreation
        | AgentFailure::StorageUnavailable
        | AgentFailure::VaultUnavailable
        | AgentFailure::VaultLocked
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
    if let Some(projection) = floe_app::VaultLifecycleFailureProjection::access_failure(failure) {
        let incident = UuidRefDto::new(uuid::Uuid::new_v4()).expect("fresh incident identity");
        error.owner_failure = Some(OwnerFailureDto {
            domain: projection.domain,
            category: projection.category,
            reason: projection.failure,
            incident_id: incident.clone(),
            correlation_id: incident,
            reload_required: projection.reload_required,
            seal_session: projection.seal_session,
            recovery: match projection.recovery {
                floe_app::VaultLifecycleRecovery::None => OwnerRecoveryDto::None,
                floe_app::VaultLifecycleRecovery::ReopenVault => OwnerRecoveryDto::Reopen,
            },
            safe_actions: projection.safe_actions,
        });
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
        owner_failure: None,
    }
}
