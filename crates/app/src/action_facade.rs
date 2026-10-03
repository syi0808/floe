//! Constructor-only assembly for the Actions owner in one ready host generation.

use floe_actions::{ActionsDependencies, ActionsService, SystemActionsClock};
use floe_day::DayService;
use floe_experts::TaskRepository;
use floe_kernel::{AgentFailure, OwnerActor};
use floe_provider_adapters::sources::NativeCalendarExecutor;
use floe_vault::{
    EncryptedAgentVault, TursoStore, VaultActionsRepository, VaultExpertProposalReader,
    VaultKeyProvider,
};
use std::sync::Arc;

pub(crate) fn build_actions<Keys: VaultKeyProvider + 'static>(
    actor: OwnerActor,
    vault: Arc<EncryptedAgentVault<Keys>>,
    store: Arc<TursoStore>,
    day: Arc<DayService>,
    tasks: Arc<dyn TaskRepository>,
) -> Result<Arc<ActionsService>, AgentFailure> {
    let repository = Arc::new(VaultActionsRepository::new(vault));
    let proposals = Arc::new(VaultExpertProposalReader::new(tasks));
    let executor = Arc::new(NativeCalendarExecutor::new(actor.clone(), store.clone())?);
    Ok(Arc::new(ActionsService::new(
        actor,
        ActionsDependencies {
            repository,
            sources: store,
            proposals,
            day,
            executor,
            clock: Arc::new(SystemActionsClock),
        },
    )?))
}
