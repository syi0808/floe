//! Host composition: the services the app host exposes, their lifetime and the
//! concrete dependencies injected into them.

use std::sync::Arc;

use tokio::runtime::{Builder, Runtime};

use crate::{AppHost, FloeCore, HostError, HostServices, agent_run, local_context, vault_host};

pub struct AppComposition {
    pub(crate) runtime: Runtime,
    pub(crate) core: Arc<FloeCore>,
    pub(crate) agent_runs: agent_run::AgentRuns,
    pub(crate) local_context: Arc<local_context::LocalContextHost>,
    #[cfg(unix)]
    pub(crate) agent_vault: vault_host::VaultBridge,
}

impl HostServices for AppComposition {
    fn shutdown(&self) -> Result<(), crate::HostError> {
        #[cfg(unix)]
        self.agent_vault.shutdown();
        self.agent_runs.close(&self.runtime);
        Ok(())
    }
}

#[cfg(unix)]
impl crate::ConversationCommands for AppComposition {
    fn start_turn(
        &self,
        caller: &crate::CallerContext,
        request: crate::StartTurn,
    ) -> Result<crate::CommandReceipt, crate::ServiceError> {
        request.validate()?;
        let command_id = floe_kernel::CommandId::from_uuid(request.command_id)
            .ok_or(crate::ServiceError::InvalidInput)?;
        let person = floe_kernel::PersonId(caller.person_id());
        let precheck = self
            .agent_vault
            .precheck_turn(
                person,
                floe_conversation::TurnPrecheckRequest {
                    principal: person.to_string(),
                    command_id,
                    session_id: request.session_id,
                    mode: continuation_mode(&request.mode)?,
                },
            )
            .map_err(service_failure)?;
        // Turn admission performs no model/source discovery: the command
        // carries intent only, and the canonical owners admit the stored
        // credential after admission. A failed paired server never blocks this.
        let retry_of = request
            .retry_of
            .map(|run_id| {
                floe_kernel::RunId::from_uuid(run_id).ok_or(crate::ServiceError::InvalidInput)
            })
            .transpose()?;
        let turn = crate::ConversationTurnRequest::new(
            request.session_id,
            request.expected_revision,
            request.text,
            caller.device_id().to_owned(),
            request.profile,
            precheck.continuation,
            retry_of,
        );
        let receipt = self
            .agent_vault
            .start_conversation(person, command_id, turn)
            .map_err(service_failure)?;
        Ok(crate::CommandReceipt {
            command_id: receipt.command_id.as_uuid(),
            run_id: receipt.run_id.as_uuid(),
            session_revision: receipt.session_revision,
        })
    }

    fn cancel_run(
        &self,
        caller: &crate::CallerContext,
        request: crate::CancelRun,
    ) -> Result<crate::CancelRunReceipt, crate::ServiceError> {
        request.validate()?;
        let run_id = floe_kernel::RunId::from_uuid(request.run_id)
            .ok_or(crate::ServiceError::InvalidInput)?;
        let command_id = floe_kernel::CommandId::from_uuid(request.command_id)
            .ok_or(crate::ServiceError::InvalidInput)?;
        match self
            .agent_vault
            .cancel_conversation(
                floe_kernel::PersonId(caller.person_id()),
                command_id,
                run_id,
            )
            .map_err(service_failure)?
        {
            floe_conversation::CancelRunStatus::Cancelled
            | floe_conversation::CancelRunStatus::Inactive => {}
            floe_conversation::CancelRunStatus::Unknown => {
                return Err(crate::ServiceError::NotFound);
            }
        }
        Ok(crate::CancelRunReceipt {
            command_id: request.command_id,
            run_id: request.run_id,
            outcome: crate::CancelRunOutcome::Accepted,
        })
    }
}

#[cfg(unix)]
impl crate::ConversationQueries for AppComposition {
    fn read_conversation(
        &self,
        caller: &crate::CallerContext,
        request: crate::ReadConversation,
    ) -> Result<Option<floe_conversation::RunReceipt>, crate::ServiceError> {
        use crate::vault_host::ConversationQuery;
        request.validate()?;
        let query = match request {
            crate::ReadConversation::Command { command_id } => ConversationQuery::Command(
                floe_kernel::CommandId::from_uuid(command_id)
                    .ok_or(crate::ServiceError::InvalidInput)?,
            ),
            crate::ReadConversation::Run { run_id } => ConversationQuery::Run(
                floe_kernel::RunId::from_uuid(run_id).ok_or(crate::ServiceError::InvalidInput)?,
            ),
            crate::ReadConversation::Message { message_id } => ConversationQuery::Message(
                floe_kernel::RunId::from_uuid(message_id)
                    .ok_or(crate::ServiceError::InvalidInput)?,
            ),
        };
        self.agent_vault
            .conversation_query(floe_kernel::PersonId(caller.person_id()), query)
            .map_err(service_failure)
    }
}

#[cfg(unix)]
impl crate::ConversationEvents for AppComposition {
    fn read_conversation_events(
        &self,
        caller: &crate::CallerContext,
        request: crate::ReadConversationEvents,
    ) -> Result<crate::EventRead, crate::ServiceError> {
        request.validate()?;
        Ok(self.agent_vault.app_events().read(
            &caller.person_id().to_string(),
            caller.runtime_epoch(),
            request.runtime_epoch,
            request.cursor,
            request.limit,
        ))
    }
}

/// Restate the caller's turn mode in Conversation's own terms; Conversation
/// decides whether the continuation it names is admissible.
#[cfg(unix)]
fn continuation_mode(
    mode: &crate::TurnMode,
) -> Result<floe_conversation::TurnMode, crate::ServiceError> {
    Ok(match mode {
        crate::TurnMode::New => floe_conversation::TurnMode::New,
        crate::TurnMode::Continue(reference) => {
            floe_conversation::TurnMode::Continue(floe_conversation::ContinuationRef {
                run_id: floe_kernel::RunId::from_uuid(reference.run_id)
                    .ok_or(crate::ServiceError::InvalidInput)?,
                executor_generation: reference.executor_generation,
                level: reference.level,
            })
        }
    })
}

#[cfg(unix)]
fn service_failure(failure: floe_kernel::AgentFailure) -> crate::ServiceError {
    use crate::ServiceError;
    use floe_kernel::AgentFailure;

    match failure {
        AgentFailure::InvalidInput | AgentFailure::UnsupportedVersion => ServiceError::InvalidInput,
        AgentFailure::NotFound => ServiceError::NotFound,
        AgentFailure::Conflict => ServiceError::Conflict,
        AgentFailure::PolicyDenied
        | AgentFailure::ConsentRequired
        | AgentFailure::CapabilityDenied
        | AgentFailure::AccessReviewRequired => ServiceError::AccessDenied,
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
        | AgentFailure::Interrupted => ServiceError::Unavailable,
        AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput
        | AgentFailure::Stalled => ServiceError::Internal,
    }
}

/// Create the host: one current-thread runtime, the local store, the Vault
/// worker, bound to the verified local identity.
pub fn open(path: &str) -> Result<AppHost<AppComposition>, AppOpenError> {
    let identity = crate::bootstrap::local_identity_for_database(std::path::Path::new(path))
        .map_err(AppOpenError::Host)?;
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| AppOpenError::Runtime(error.to_string()))?;
    let core = Arc::new(
        runtime
            .block_on(crate::FloeCore::open(path))
            .map_err(|error| AppOpenError::Store(error.to_string()))?,
    );
    let local_context = Arc::new(crate::local_context::LocalContextHost::default());
    let services = AppComposition {
        runtime,
        core: core.clone(),
        agent_runs: Default::default(),
        local_context: local_context.clone(),
        #[cfg(unix)]
        agent_vault: vault_host::VaultBridge::new(path, core, local_context),
    };
    match identity {
        Some(identity) => AppHost::bootstrap_claim(services, identity).map_err(AppOpenError::Host),
        None => Ok(AppHost::legacy(services)),
    }
}

#[derive(Clone, Debug)]
pub enum AppOpenError {
    Host(HostError),
    Runtime(String),
    Store(String),
}

impl AppComposition {
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    pub fn core(&self) -> &crate::FloeCore {
        &self.core
    }

    pub fn agent_runs(&self) -> &agent_run::AgentRuns {
        &self.agent_runs
    }

    pub fn local_context(&self) -> &crate::local_context::LocalContextHost {
        &self.local_context
    }

    #[cfg(unix)]
    pub fn agent_vault(&self) -> &vault_host::VaultBridge {
        &self.agent_vault
    }
}

#[cfg(unix)]
impl crate::RemotePairingCommands for AppComposition {
    fn remote_pairing(
        &self,
        caller: &crate::CallerContext,
        request_id: uuid::Uuid,
        command: crate::RemotePairingCommand,
    ) -> Result<crate::RemotePairingResult, crate::ServiceError> {
        command.validate()?;
        let result = self
            .agent_vault
            .remote_request(
                caller,
                request_id,
                Some(crate::WorkerAction::RemotePairing {
                    caller: caller.clone(),
                    command,
                }),
                true,
                false,
            )
            .map_err(service_failure)?;
        Ok(pairing_result(result))
    }

    fn read_pairing_result(
        &self,
        caller: &crate::CallerContext,
        operation_id: uuid::Uuid,
        release: bool,
    ) -> Result<crate::RemotePairingResult, crate::ServiceError> {
        self.agent_vault
            .remote_request(caller, operation_id, None, true, release)
            .map(pairing_result)
            .map_err(service_failure)
    }
}

fn pairing_result(result: crate::WorkerResult) -> crate::RemotePairingResult {
    crate::RemotePairingResult {
        operation_id: result.request_id,
        stage: result.stage,
        done: result.done,
        owner: result.remote_owner,
        pairing: result.remote_pairing,
        failure: result.failure,
    }
}

#[cfg(unix)]
impl crate::RemoteAccessCommands for AppComposition {
    fn remote_access(
        &self,
        caller: &crate::CallerContext,
        request_id: uuid::Uuid,
        command: crate::RemoteAccessCommand,
    ) -> Result<crate::RemoteAccessResult, crate::ServiceError> {
        let result = self
            .agent_vault
            .remote_request(
                caller,
                request_id,
                Some(crate::WorkerAction::RemoteAccess {
                    caller: caller.clone(),
                    command,
                }),
                false,
                false,
            )
            .map_err(service_failure)?;
        Ok(access_result(result))
    }

    fn read_access_result(
        &self,
        caller: &crate::CallerContext,
        operation_id: uuid::Uuid,
        release: bool,
    ) -> Result<crate::RemoteAccessResult, crate::ServiceError> {
        self.agent_vault
            .remote_request(caller, operation_id, None, false, release)
            .map(access_result)
            .map_err(service_failure)
    }
}

fn access_result(result: crate::WorkerResult) -> crate::RemoteAccessResult {
    crate::RemoteAccessResult {
        operation_id: result.request_id,
        stage: result.stage,
        done: result.done,
        person_id: result.person_id,
        producer: result.remote_producer,
        owner: result.remote_owner,
        enrollment: result.remote_enrollment,
        calendar_grant: result.remote_calendar_grant,
        calendar_preview: result.remote_calendar_preview,
        view_grant: result.remote_view_grant,
        view_preview: result.remote_view_preview,
        failure: result.failure,
    }
}
