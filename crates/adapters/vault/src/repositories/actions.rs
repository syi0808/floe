#![cfg(unix)]

use std::sync::Arc;

use floe_access::{OperationAuthorizationPolicy, OperationPolicyChange};
use floe_calendar_operations::{
    ActionDecision, ActionPage, ActionReconciliation, ActionRecord, AdmittedOperation,
    CalendarOperationStoreError, CalendarOperationsRepository, CollectionAck, CollectionTicket,
    DispatchAdmission, DispatchIntent, ExecutionSettlement, OperationAdmission, PreDispatchStop,
    RecoveryPage,
};
use floe_execution::BoxFuture;
use floe_kernel::CommandFailure;
use floe_kernel::PersonId;
use uuid::Uuid;

use crate::{EncryptedAgentVault, VaultKeyProvider};

impl floe_calendar_operations::OperationSourceReader for crate::TursoStore {
    fn load<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a floe_context_contract::ConnectionId,
    ) -> BoxFuture<'a, Result<Option<floe_connections::SourceConnection>, floe_kernel::AgentFailure>>
    {
        Box::pin(async move {
            floe_connections::SourceRepository::load(self, person_id, connection_id)
                .await
                .map_err(|_| floe_kernel::AgentFailure::StorageUnavailable)
        })
    }
    fn list_calendar_sources<'a>(
        &'a self,
        person_id: PersonId,
    ) -> BoxFuture<'a, Result<Vec<floe_connections::SourceConnection>, floe_kernel::AgentFailure>>
    {
        Box::pin(async move {
            let connector = floe_context_contract::ConnectorId::try_new("calendar.event_kit")
                .map_err(|_| floe_kernel::AgentFailure::InvalidInput)?;
            let mut sources =
                floe_connections::SourceRepository::list_current(self, person_id, &connector)
                    .await
                    .map_err(|_| floe_kernel::AgentFailure::StorageUnavailable)?;
            #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
            {
                let fixture = floe_context_contract::ConnectorId::try_new("calendar.fixture")
                    .map_err(|_| floe_kernel::AgentFailure::InvalidInput)?;
                sources.extend(
                    floe_connections::SourceRepository::list_current(self, person_id, &fixture)
                        .await
                        .map_err(|_| floe_kernel::AgentFailure::StorageUnavailable)?,
                );
            }
            Ok(sources)
        })
    }
    fn read_reservation_fence<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a floe_context_contract::ConnectionId,
    ) -> BoxFuture<'a, Result<floe_connections::SourceReservationFence, floe_kernel::AgentFailure>>
    {
        Box::pin(async move {
            floe_connections::SourceOperationRepository::read_reservation_fence(
                self,
                person_id,
                connection_id,
            )
            .await
            .map_err(|_| floe_kernel::AgentFailure::StorageUnavailable)
        })
    }
    fn source_is_fenced<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a floe_context_contract::ConnectionId,
    ) -> BoxFuture<'a, Result<bool, floe_kernel::AgentFailure>> {
        Box::pin(async move {
            floe_connections::SourceOperationRepository::source_is_fenced(
                self,
                person_id,
                connection_id,
            )
            .await
            .map_err(|_| floe_kernel::AgentFailure::StorageUnavailable)
        })
    }
}

/// The one Actions repository, backed by the Person's encrypted Vault.
pub struct VaultActionsRepository<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
}

impl<Keys: VaultKeyProvider> VaultActionsRepository<Keys> {
    pub fn new(vault: Arc<EncryptedAgentVault<Keys>>) -> Self {
        Self { vault }
    }
}

impl<Keys: VaultKeyProvider> CalendarOperationsRepository for VaultActionsRepository<Keys> {
    fn validate_proposal_coverage<'a>(
        &'a self,
        person_id: PersonId,
        coverage: &'a floe_agent_contract::DependencyCoverage,
    ) -> BoxFuture<'a, Result<(), CalendarOperationStoreError>> {
        Box::pin(async move {
            self.vault
                .actions_validate_proposal_coverage(person_id, coverage)
                .await
        })
    }

    fn get<'a>(
        &'a self,
        person_id: PersonId,
        action_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<ActionRecord>, CalendarOperationStoreError>> {
        Box::pin(async move { self.vault.actions_get(person_id, action_id).await })
    }

    fn list<'a>(
        &'a self,
        person_id: PersonId,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> BoxFuture<'a, Result<ActionPage, CalendarOperationStoreError>> {
        Box::pin(async move { self.vault.actions_list(person_id, cursor, limit).await })
    }

    fn list_direct<'a>(
        &'a self,
        person_id: PersonId,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> BoxFuture<'a, Result<ActionPage, CalendarOperationStoreError>> {
        Box::pin(async move {
            self.vault
                .actions_list_direct(person_id, cursor, limit)
                .await
        })
    }

    fn find_admission<'a>(
        &'a self,
        person_id: PersonId,
        command_id: Uuid,
        request_digest: [u8; 32],
    ) -> BoxFuture<'a, Result<Option<ActionRecord>, CalendarOperationStoreError>> {
        Box::pin(async move {
            self.vault
                .actions_find_admission(person_id, command_id, request_digest)
                .await
        })
    }

    fn admit<'a>(
        &'a self,
        admission: OperationAdmission,
    ) -> BoxFuture<'a, Result<AdmittedOperation, CommandFailure<CalendarOperationStoreError>>> {
        Box::pin(async move { self.vault.actions_admit(admission).await })
    }

    fn record_decision<'a>(
        &'a self,
        decision: ActionDecision,
    ) -> BoxFuture<'a, Result<ActionRecord, CommandFailure<CalendarOperationStoreError>>> {
        Box::pin(async move { self.vault.actions_record_decision(decision).await })
    }

    fn admit_reconciliation<'a>(
        &'a self,
        command: ActionReconciliation,
    ) -> BoxFuture<'a, Result<ActionRecord, CommandFailure<CalendarOperationStoreError>>> {
        Box::pin(async move { self.vault.actions_admit_reconciliation(command).await })
    }

    fn stop_before_dispatch<'a>(
        &'a self,
        stop: PreDispatchStop,
    ) -> BoxFuture<'a, Result<ActionRecord, CalendarOperationStoreError>> {
        Box::pin(async move { self.vault.actions_stop_before_dispatch(stop).await })
    }

    fn prepare_dispatch<'a>(
        &'a self,
        intent: DispatchIntent,
    ) -> BoxFuture<'a, Result<DispatchAdmission, CalendarOperationStoreError>> {
        Box::pin(async move { self.vault.actions_prepare_dispatch(intent).await })
    }

    fn load_execution<'a>(
        &'a self,
        person_id: PersonId,
        execution_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<DispatchAdmission>, CalendarOperationStoreError>> {
        Box::pin(async move {
            self.vault
                .actions_load_execution(person_id, execution_id)
                .await
        })
    }

    fn settle_execution<'a>(
        &'a self,
        settlement: ExecutionSettlement,
    ) -> BoxFuture<'a, Result<ActionRecord, CalendarOperationStoreError>> {
        Box::pin(async move { self.vault.actions_settle_execution(settlement).await })
    }

    fn pending_recovery<'a>(
        &'a self,
        person_id: PersonId,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> BoxFuture<'a, Result<RecoveryPage, CalendarOperationStoreError>> {
        Box::pin(async move {
            self.vault
                .actions_pending_recovery(person_id, cursor, limit)
                .await
        })
    }

    fn ack_collection<'a>(
        &'a self,
        ack: CollectionAck,
    ) -> BoxFuture<'a, Result<CollectionTicket, CalendarOperationStoreError>> {
        Box::pin(async move { self.vault.actions_ack_collection(ack).await })
    }

    fn read_authority<'a>(
        &'a self,
        person_id: PersonId,
    ) -> BoxFuture<'a, Result<OperationAuthorizationPolicy, CalendarOperationStoreError>> {
        Box::pin(async move { self.vault.actions_read_authority(person_id).await })
    }

    fn compare_and_set_authority<'a>(
        &'a self,
        change: OperationPolicyChange,
    ) -> BoxFuture<
        'a,
        Result<OperationAuthorizationPolicy, CommandFailure<CalendarOperationStoreError>>,
    > {
        Box::pin(async move { self.vault.actions_compare_and_set_authority(change).await })
    }
}
