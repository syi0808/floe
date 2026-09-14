use std::collections::BTreeMap;

use floe_conversation::{RunReceipt, RunState};
use floe_kernel::{AgentFailure, CommandId, PersonId, RunId};
use floe_protocol::{
    AppCommandReceiptDto, AppCommandStatusDto, AppMessageDto, AppMessageRoleDto, AppQueryDto,
    AppQueryRequestDto, AppQueryResultDto, AppReplyStatusDto, AppRunSnapshotDto, AppRunStateDto,
    AppTurnExecutionDto, AppTurnReportDto, AppWireErrorCodeDto, AppWireErrorDto,
};

use crate::{FloeHandle, vault_host::ConversationQuery};

pub(crate) type AppWireResult<T> = Result<T, AppWireErrorDto>;

pub(crate) fn query(
    handle: &FloeHandle,
    request: AppQueryRequestDto,
) -> AppWireResult<AppQueryResultDto> {
    request.validate().map_err(request_validation)?;
    let host_request = handle
        .app
        .request(request.request_id)
        .map_err(host_failure)?;
    let caller = host_request.caller();
    let person = PersonId(caller.person_id());
    let runtime_epoch = caller.runtime_epoch();
    #[cfg(unix)]
    let services = host_request.services();
    match request.query {
        AppQueryDto::ConversationGetCommand { command_id } => {
            let command_id =
                CommandId::from_uuid(command_id).ok_or_else(|| validation("query.command_id"))?;
            let receipt = services
                .agent_vault
                .conversation_query(person, ConversationQuery::Command(command_id))
                .map_err(agent_failure)?;
            Ok(match receipt {
                Some(receipt) => AppQueryResultDto::CommandReceipt {
                    receipt: command_receipt(&receipt),
                },
                None => AppQueryResultDto::UnknownCommand {
                    command_id: command_id.as_uuid(),
                },
            })
        }
        AppQueryDto::ConversationGetRun { run_id } => {
            let run_id = RunId::from_uuid(run_id).ok_or_else(|| validation("query.run_id"))?;
            let receipt = services
                .agent_vault
                .conversation_query(person, ConversationQuery::Run(run_id))
                .map_err(agent_failure)?
                .ok_or_else(not_found)?;
            Ok(AppQueryResultDto::RunSnapshot {
                run: run_snapshot(receipt, runtime_epoch),
            })
        }
        AppQueryDto::ConversationGetMessage { message_id } => {
            let run_id =
                RunId::from_uuid(message_id).ok_or_else(|| validation("query.message_id"))?;
            let receipt = services
                .agent_vault
                .conversation_query(person, ConversationQuery::Message(run_id))
                .map_err(agent_failure)?
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

fn command_receipt(receipt: &RunReceipt) -> AppCommandReceiptDto {
    AppCommandReceiptDto {
        command_id: receipt.command_id.as_uuid(),
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

fn request_validation(field: &'static str) -> AppWireErrorDto {
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
    wire_error(code, "app request could not complete", None)
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
