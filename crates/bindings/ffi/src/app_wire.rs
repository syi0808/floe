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
    Services: floe_app::HostServices + floe_app::ConversationCommands,
{
    request.validate().map_err(request_validation)?;
    let host_request = host.request(request.request_id).map_err(host_failure)?;
    let runtime_epoch = host_request.caller().runtime_epoch();
    match request.command {
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

fn query_with_host<Services: floe_app::HostServices + floe_app::ConversationQueries>(
    host: &floe_app::AppHost<Services>,
    request: AppQueryRequestDto,
) -> AppWireResult<AppQueryResultDto> {
    request.validate().map_err(request_validation)?;
    let host_request = host.request(request.request_id).map_err(host_failure)?;
    let caller = host_request.caller();
    let runtime_epoch = caller.runtime_epoch();
    let services = host_request.services();
    match request.query {
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
