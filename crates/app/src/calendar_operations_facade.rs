//! Constructor-only assembly for Calendar Operations in one ready generation.

use floe_calendar_operations::{
    CalendarOperationsDependencies, CalendarOperationsService, SystemCalendarOperationsClock,
};
use floe_day::DayService;
use floe_experts::TaskRepository;
use floe_kernel::{AgentFailure, OwnerActor};
use floe_provider_adapters::sources::NativeCalendarExecutor;
use floe_vault::{
    EncryptedAgentVault, TursoStore, VaultActionsRepository, VaultExpertProposalReader,
    VaultKeyProvider,
};
use std::sync::Arc;

pub(crate) fn build_calendar_operations<Keys: VaultKeyProvider + 'static>(
    actor: OwnerActor,
    vault: Arc<EncryptedAgentVault<Keys>>,
    store: Arc<TursoStore>,
    day: Arc<DayService>,
    tasks: Arc<dyn TaskRepository>,
    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))] qa_executor: Option<
        Arc<dyn floe_calendar_operations::CalendarOperationExecutor>,
    >,
) -> Result<Arc<CalendarOperationsService>, AgentFailure> {
    let repository = Arc::new(VaultActionsRepository::new(vault));
    let proposals = Arc::new(VaultExpertProposalReader::new(tasks));
    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
    let executor: Arc<dyn floe_calendar_operations::CalendarOperationExecutor> = match qa_executor {
        Some(executor) => executor,
        None => Arc::new(NativeCalendarExecutor::new(actor.clone(), store.clone())?),
    };
    #[cfg(not(all(feature = "qa-fixtures", target_os = "linux")))]
    let executor: Arc<dyn floe_calendar_operations::CalendarOperationExecutor> =
        Arc::new(NativeCalendarExecutor::new(actor.clone(), store.clone())?);
    let service = Arc::new(CalendarOperationsService::new(
        actor,
        CalendarOperationsDependencies {
            repository,
            sources: store,
            proposals,
            day: day.clone(),
            executor,
            clock: Arc::new(SystemCalendarOperationsClock),
        },
    )?);
    Ok(service)
}
