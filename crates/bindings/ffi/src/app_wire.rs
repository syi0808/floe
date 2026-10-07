use crate::bridge::FloeHandle;
use floe_app::{NativeHostCommands, NativeHostQueries};
use floe_kernel::AgentFailure;
use floe_protocol::*;
use std::collections::BTreeMap;
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
    match request.command {
        AppCommandDto::NativeHost(command) => {
            let result = services
                .apply_native_host(
                    caller,
                    crate::context_wire::command(command)
                        .map_err(|error| AppCommandFailure::NotAdmitted(structural_error(error)))?,
                )
                .map_err(agent_failure)?;
            crate::conversion::native::native_host_command_result(result)
                .map_err(|error| structural_error(error).into())
        }
        AppCommandDto::Runtime(command) => {
            // Runtime control is admitted by AppHost, then dispatched directly
            // to its owner without entering the Product command lane.
            if command_id.get_version() != Some(uuid::Version::Random)
                || command_id.get_variant() != uuid::Variant::RFC4122
            {
                return Err(AppCommandFailure::NotAdmitted(validation("command_id")));
            }
            match command {
                RuntimeCommandDto::Prepare {} => {
                    let result =
                        services
                            .prepare_runtime(caller, command_id)
                            .map_err(|failure| match failure {
                                floe_app::RuntimePreparationCommandFailure::NotAdmitted(reason) => {
                                    AppCommandFailure::NotAdmitted(agent_failure(reason))
                                }
                                floe_app::RuntimePreparationCommandFailure::Indeterminate(
                                    reason,
                                ) => AppCommandFailure::Indeterminate(agent_failure(reason)),
                            })?;
                    if result.operation_id != command_id {
                        return Err(internal_error().into());
                    }
                    Ok(AppCommandResultDto::RuntimePreparation {
                        result: runtime_preparation_result(result),
                    })
                }
                RuntimeCommandDto::PreparationAcknowledge {} => {
                    let result = services
                        .acknowledge_runtime_preparation(caller, command_id)
                        .map_err(|failure| match failure {
                            floe_app::RuntimePreparationCommandFailure::NotAdmitted(reason) => {
                                AppCommandFailure::NotAdmitted(agent_failure(reason))
                            }
                            floe_app::RuntimePreparationCommandFailure::Indeterminate(reason) => {
                                AppCommandFailure::Indeterminate(agent_failure(reason))
                            }
                        })?;
                    if result.operation_id != command_id {
                        return Err(internal_error().into());
                    }
                    Ok(AppCommandResultDto::RuntimePreparation {
                        result: runtime_preparation_result(result),
                    })
                }
            }
        }
        AppCommandDto::Product(command) => {
            // Product clients allocate random nonces. Derived v5 identities are
            // reserved for trusted owner-to-owner work, never client occupancy.
            if command_id.get_version() != Some(uuid::Version::Random)
                || command_id.get_variant() != uuid::Variant::RFC4122
            {
                return Err(AppCommandFailure::NotAdmitted(validation("command_id")));
            }
            crate::product_wire::command(&host_request, command_id, command)
        }
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
    match request.query {
        AppQueryDto::NativeHost(query) => {
            let result = services
                .query_native_host(
                    caller,
                    crate::context_wire::query(query).map_err(structural_error)?,
                )
                .map_err(agent_failure)?;
            crate::conversion::native::native_host_query_result(result).map_err(structural_error)
        }
        AppQueryDto::Runtime(query) => {
            // Runtime control is a first-class lane inside AppWire, distinct
            // from both Product commands and native callbacks.
            match query {
                RuntimeQueryDto::Readiness {} => {
                    let readiness = services
                        .runtime_readiness(caller, request_id)
                        .map_err(agent_failure)?;
                    Ok(AppQueryResultDto::RuntimeReadiness {
                        readiness: runtime_readiness(readiness),
                    })
                }
                RuntimeQueryDto::PreparationGet { operation_id } => {
                    let result = services
                        .get_runtime_preparation(caller, operation_id)
                        .map_err(agent_failure)?;
                    if result.operation_id != operation_id {
                        return Err(internal_error());
                    }
                    Ok(AppQueryResultDto::RuntimePreparation {
                        result: runtime_preparation_result(result),
                    })
                }
            }
        }
        AppQueryDto::Product(query) => crate::product_wire::query(&host_request, query),
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
    crate::product_wire::observe(&host_request, request)
}

fn runtime_preparation_result(
    result: floe_app::RuntimePreparationResult,
) -> floe_protocol::RuntimePreparationResultDto {
    floe_protocol::RuntimePreparationResultDto {
        operation_id: result.operation_id,
        done: result.done,
        failure: result.failure,
    }
}

fn runtime_readiness(readiness: floe_app::RuntimeReadiness) -> floe_protocol::RuntimeReadinessDto {
    floe_protocol::RuntimeReadinessDto {
        state: match readiness.state {
            floe_app::RuntimeReadinessState::Ready => {
                floe_protocol::RuntimeReadinessStateDto::Ready
            }
            floe_app::RuntimeReadinessState::PreparationRequired => {
                floe_protocol::RuntimeReadinessStateDto::PreparationRequired
            }
            floe_app::RuntimeReadinessState::Unavailable => {
                floe_protocol::RuntimeReadinessStateDto::Unavailable
            }
        },
        failure: readiness
            .failure
            .map(crate::conversion::owners::runtime_owner_failure),
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

pub(crate) fn day_error(error: floe_app::CoreError) -> AppWireErrorDto {
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
        | AgentFailure::StorageBusy
        | AgentFailure::VaultUnavailable
        | AgentFailure::VaultLocked
        | AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::LocalModelTimeout
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::ServerModelRequestRejected
        | AgentFailure::CredentialExpired
        | AgentFailure::QuotaExceeded
        | AgentFailure::CapabilityUnavailable
        | AgentFailure::StaleContext
        | AgentFailure::BudgetExceeded
        | AgentFailure::ModelInputCapacityExceeded
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
    let incident = uuid::Uuid::new_v4();
    if let Some(projection) =
        floe_app::RuntimeFailureProjection::access_failure(failure, incident, true)
    {
        error.owner_failure = Some(crate::conversion::owners::runtime_owner_failure(projection));
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

#[cfg(test)]
mod runtime_wire_tests {
    use super::*;

    #[test]
    fn runtime_queries_and_commands_keep_current_state_separate_from_history() {
        let operation_id = uuid::Uuid::new_v4();
        let command = AppCommandResultDto::RuntimePreparation {
            result: runtime_preparation_result(floe_app::RuntimePreparationResult {
                operation_id,
                done: true,
                failure: None,
            }),
        };
        let command = serde_json::to_value(command).expect("serialize Runtime command result");
        assert_eq!(command["kind"], "runtime.preparation");
        assert_eq!(command["operation_id"], operation_id.to_string());
        assert_eq!(command["done"], true);

        let failure = floe_app::RuntimeFailureProjection::access_failure(
            AgentFailure::VaultLocked,
            uuid::Uuid::new_v4(),
            true,
        )
        .expect("project intrinsic Vault readiness failure");
        let query = AppQueryResultDto::RuntimeReadiness {
            readiness: runtime_readiness(floe_app::RuntimeReadiness {
                state: floe_app::RuntimeReadinessState::PreparationRequired,
                failure: Some(failure),
            }),
        };
        let query = serde_json::to_value(query).expect("serialize current Runtime readiness");
        assert_eq!(query["kind"], "runtime.readiness");
        assert_eq!(query["state"], "preparation_required");
        assert_eq!(query["failure"]["domain"], "vault");
        assert_eq!(query["failure"]["reason"], "vault_locked");
    }
}
