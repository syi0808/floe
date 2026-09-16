//! Host composition: the services the app host exposes, their lifetime and the
//! concrete dependencies injected into them.

use std::sync::Arc;

use tokio::runtime::{Builder, Runtime};

use crate::{AppHost, CallerContext, HostError, HostServices, agent_run, events, vault_host};
use floe_protocol::{AgentConversationTurnRequestDto, AppProfileSelectionDto};

pub struct AppComposition {
    runtime: Runtime,
    core: Arc<FloeCore>,
    agent_runs: agent_run::AgentRuns,
    local_context: Arc<local_context::LocalContextStore>,
    #[cfg(unix)]
    agent_vault: vault_host::VaultBridge,
    #[cfg(unix)]
    inference_routes: inference_routes::HostInferenceRoutes,
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
        let existing = self
            .agent_vault
            .conversation_query(person, vault_host::ConversationQuery::Command(command_id))
            .map_err(service_failure)?;
        let continuation = match &request.mode {
            crate::TurnMode::New => false,
            crate::TurnMode::Continue(reference) => {
                let run_id = floe_kernel::RunId::from_uuid(reference.run_id)
                    .ok_or(crate::ServiceError::InvalidInput)?;
                if let Some(receipt) = existing.as_ref() {
                    if receipt.session_id != request.session_id
                        || receipt.continuation_of != Some(run_id)
                        || receipt.continuation_executor_generation
                            != Some(reference.executor_generation)
                        || receipt.continuation_level != reference.level
                    {
                        return Err(crate::ServiceError::Conflict);
                    }
                } else {
                    let source = self
                        .agent_vault
                        .conversation_query(person, vault_host::ConversationQuery::Run(run_id))
                        .map_err(service_failure)?
                        .ok_or(crate::ServiceError::NotFound)?;
                    if source.session_id != request.session_id
                        || !source.state.is_terminal()
                        || source.executor_generation != reference.executor_generation
                        || source.continuation_level.checked_add(1) != Some(reference.level)
                    {
                        return Err(crate::ServiceError::Conflict);
                    }
                }
                true
            }
        };
        let receipt = self
            .agent_vault
            .start_conversation(
                person,
                command_id,
                AgentConversationTurnRequestDto {
                    session_id: request.session_id.to_string(),
                    expected_revision: request.expected_revision,
                    text: request.text,
                    device_id: caller.device_id().to_owned(),
                    profile: match request.profile {
                        crate::ProfileSelection::Auto => AppProfileSelectionDto::Auto,
                        crate::ProfileSelection::Explicit(profile_id) => {
                            AppProfileSelectionDto::Explicit { profile_id }
                        }
                    },
                    continuation,
                    retry_of: request.retry_of,
                    remote_route: self
                        .inference_routes
                        .resolve(&self.runtime, caller)
                        .map_err(service_failure)?,
                },
            )
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
/// worker and the route selector, bound to the verified local identity.
pub fn open(path: &str) -> Result<AppHost<AppComposition>, AppOpenError> {
    let identity =
        crate::local_identity_for_database(std::path::Path::new(path)).map_err(AppOpenError::Host)?;
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| AppOpenError::Runtime(error.to_string()))?;
    let core = Arc::new(
        runtime
            .block_on(crate::FloeCore::open(path))
            .map_err(|error| AppOpenError::Store(error.to_string()))?,
    );
    let local_context = Arc::new(crate::local_context::LocalContextStore::default());
    let services = AppComposition {
        runtime,
        core: core.clone(),
        agent_runs: Default::default(),
        local_context: local_context.clone(),
        #[cfg(unix)]
        agent_vault: vault_host::VaultBridge::new(path, core, local_context),
        #[cfg(unix)]
        inference_routes: crate::HostInferenceRoutes,
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

    pub fn local_context(&self) -> &crate::local_context::LocalContextStore {
        &self.local_context
    }

    #[cfg(unix)]
    pub fn agent_vault(&self) -> &vault_host::VaultBridge {
        &self.agent_vault
    }
}
