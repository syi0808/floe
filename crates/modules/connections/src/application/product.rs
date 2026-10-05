//! Product projections and commands live with Connections. App supplies admitted
//! actors and wires ports; it never reconstructs reviewed source authority.
use super::product_records::ProductRecordScan;
use super::source_configuration::{SourceConfigurationOutcome, aborted_reason, terminal_outcome};
use crate::*;
use chrono::{Duration, Utc};
use floe_access::{ReviewRef, SourceObserveStatus, SourceProcessingChoice};
use floe_context_contract::GrantSourceBinding;
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, OwnerActor, PersonId};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

impl ConnectionsService {
    pub async fn start_pairing(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        address: &str,
        scope: &ExecutionScope,
    ) -> Result<PairingSnapshot, ConnectionsCommandFailure> {
        self.ensure_open()
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        check(actor, scope).map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let result = self
            .pairing
            .start_pairing(actor, command_id, address, scope)
            .await?;
        let id = super::source_operation::derived_id(
            b"floe.pairing.operation.v1",
            actor.person_id,
            command_id,
        );
        self.spawn_pairing(actor.clone(), id, scope)
            .map_err(ConnectionsCommandFailure::Admitted)?;
        Ok(result)
    }
    pub async fn get_pairing(
        &self,
        actor: &OwnerActor,
        operation_ref: Uuid,
        scope: &ExecutionScope,
    ) -> Result<PairingSnapshot, AgentFailure> {
        self.ensure_open()?;
        self.pairing
            .get_pairing(actor, operation_ref, scope)
            .await
            .map_err(pairing_error)
    }
    pub async fn cancel_pairing(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        operation_ref: Uuid,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<PairingSnapshot, ConnectionsCommandFailure> {
        self.ensure_open()
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        check(actor, scope).map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let result = self
            .pairing
            .cancel_pairing(actor, command_id, operation_ref, expected_revision, scope)
            .await?;
        self.spawn_pairing(actor.clone(), operation_ref, scope)
            .map_err(ConnectionsCommandFailure::Admitted)?;
        Ok(result)
    }
    pub async fn reconcile_pairing(
        &self,
        actor: &OwnerActor,
        operation_ref: Uuid,
        scope: &ExecutionScope,
    ) -> Result<PairingSnapshot, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        let snapshot = self
            .pairing
            .reconcile_pairing(actor, operation_ref, scope)
            .await
            .map_err(pairing_error)?;
        if snapshot.state == PairingState::Paired {
            self.spawn_catalog_refresh(actor.clone(), scope)?;
        }
        Ok(snapshot)
    }
    pub async fn get_gateway(
        &self,
        actor: &OwnerActor,
        gateway_ref: Uuid,
        scope: &ExecutionScope,
    ) -> Result<GatewaySummary, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        if let Some(observed) = self
            .gateways
            .current(actor.person_id, &actor.device_id)
            .await
            .map_err(pairing_error)?
        {
            if observed.summary().gateway_ref == gateway_ref {
                return Ok(self.project_gateway(observed.summary().clone())?);
            }
        }
        let mut records = ProductRecordScan::new(self.products.as_ref(), actor, scope);
        while let Some(record) = records.next().await? {
            if let ConnectionsPayload::GatewayForgotten(summary) = record.payload {
                if summary.gateway_ref == gateway_ref {
                    return Ok(summary);
                }
            }
        }
        Err(AgentFailure::NotFound)
    }
    pub async fn forget_gateway(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        gateway_ref: Uuid,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<GatewaySummary, ConnectionsCommandFailure> {
        let mut classify: fn(AgentFailure) -> ConnectionsCommandFailure =
            ConnectionsCommandFailure::NotAdmitted;
        let mut command_identity = None;
        let result: Result<GatewaySummary, AgentFailure> = async {
            self.ensure_open()?;
            check(actor, scope)?;
            let intent = digest(&("forget_gateway", gateway_ref, expected_revision))?;
            let id = command_ref(actor.person_id, command_id);
            command_identity = Some(ConnectionsCommandIdentity {
                journal: crate::ConnectionsCommandJournal::Product,
                record_ref: id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                command_id,
                intent_digest: intent,
            });
            if let Some(record) = self.command(actor, id, command_id, intent).await? {
                classify = ConnectionsCommandFailure::Admitted;
                return match record.payload {
                    ConnectionsPayload::GatewayForgotten(summary) => Ok(summary),
                    _ => Err(AgentFailure::Conflict),
                };
            }
            let observed = self
                .gateways
                .current(actor.person_id, &actor.device_id)
                .await
                .map_err(pairing_error)?
                .ok_or(AgentFailure::NotFound)?;
            if observed.summary().gateway_ref != gateway_ref
                || observed.summary().revision != expected_revision
            {
                return Err(AgentFailure::Conflict);
            }
            let observed_summary = observed.summary();
            let summary = GatewaySummary {
                display_address: None,
                gateway_ref,
                revision: expected_revision
                    .checked_add(1)
                    .ok_or(AgentFailure::Conflict)?,
                display_name: observed_summary.display_name.clone(),
                state: GatewayState::Forgotten,
                remote_revocation_pending: observed_summary.state == GatewayState::Paired
                    || observed_summary.remote_revocation_pending,
                allowed_actions: vec![ConnectionAction::Pair],
                failure: None,
            };
            let expected = match observed {
                GatewayObservation::Forgotten { .. } => return Err(AgentFailure::Conflict),
                GatewayObservation::Paired { binding, .. } => {
                    GatewayForgetExpectation::Paired(binding)
                }
                GatewayObservation::RepairRequired { expectation, .. } => {
                    GatewayForgetExpectation::RepairRequired {
                        gateway_ref,
                        revision: expected_revision,
                        expectation,
                    }
                }
            };
            classify = ConnectionsCommandFailure::Indeterminate;
            let summary = self
                .gateways
                .forget(
                    record(
                        actor,
                        id,
                        command_id,
                        intent,
                        ConnectionsPayload::GatewayForgotten(summary),
                    ),
                    expected,
                )
                .await
                .map_err(pairing_error)?;
            classify = ConnectionsCommandFailure::Admitted;
            *self
                .catalog_gateway
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)? = None;
            Ok(summary)
        }
        .await;
        super::product_commands::settle_product_command(
            self.products.as_ref(),
            result,
            classify,
            command_identity,
        )
        .await
    }
    pub async fn overview(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<ConnectionsOverview, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        let mut records = ProductRecordScan::new(self.products.as_ref(), actor, scope);
        let mut gateways = Vec::new();
        if let Some(observed) = self
            .gateways
            .current(actor.person_id, &actor.device_id)
            .await
            .map_err(pairing_error)?
        {
            gateways.push(self.project_gateway(observed.summary().clone())?)
        }
        let live_gateway_ref = gateways
            .iter()
            .find(|gateway| gateway.state != GatewayState::Forgotten)
            .map(|gateway| gateway.gateway_ref);
        let mut integrations = Vec::new();
        while let Some(record) = records.next().await? {
            match record.payload {
                ConnectionsPayload::Integration(integration) => {
                    let current_target = match &integration.target {
                        IntegrationBinding::Device { device_id } => device_id == &actor.device_id,
                        IntegrationBinding::Gateway { gateway_ref, .. } => {
                            Some(*gateway_ref) == live_gateway_ref
                        }
                    };
                    if current_target {
                        integrations
                            .push(self.integration_summary(actor, &integration, scope).await?)
                    }
                }
                _ => {}
            }
        }
        for connector in [
            "calendar.event_kit",
            "contacts.apple",
            "attention.macos",
            "health.apple",
        ] {
            let id = native_integration_ref(actor, connector)?;
            if let Some(integration) = self.native_integration(actor, id).await? {
                integrations.push(self.integration_summary(actor, &integration, scope).await?)
            }
        }
        let mut sources = Vec::new();
        for source in self
            .sources
            .list_sources(actor.person_id, 512)
            .await
            .map_err(source_error)?
        {
            sources.push(self.source_summary(actor, &source, scope).await?)
        }
        Ok(ConnectionsOverview {
            gateways,
            integrations,
            sources,
        })
    }
    /// Background owner reconciliation publishes catalog aggregates, preserving
    /// metadata/get queries as pure reads and local revisions as local CAS.
    pub async fn refresh_integrations(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        let result = self.refresh_integrations_inner(actor, scope).await;
        if result.is_err() {
            *self
                .catalog_gateway
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)? = None;
        }
        *self
            .catalog_status
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)? = Some(result);
        result
    }
    async fn refresh_integrations_inner(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        let Some(GatewayObservation::Paired {
            summary: gateway,
            binding,
        }) = self
            .gateways
            .current(actor.person_id, &actor.device_id)
            .await
            .map_err(pairing_error)?
        else {
            *self
                .catalog_gateway
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)? = None;
            return Ok(());
        };
        let catalog = self
            .remote_integrations
            .list(
                IntegrationCatalogQuery {
                    gateway_ref: gateway.gateway_ref,
                    expected: binding.clone(),
                },
                scope,
            )
            .await
            .map_err(integration_error)?;
        if catalog.gateway_ref != gateway.gateway_ref || catalog.binding != binding {
            return Err(AgentFailure::PolicyDenied);
        }
        for descriptor in catalog.entries {
            let id = opaque_ref(
                b"floe.integration.ref.v1",
                actor.person_id,
                &(gateway.gateway_ref, descriptor.connector_id.as_str()),
            )?;
            let value = IntegrationRecord {
                integration_ref: id,
                target: IntegrationBinding::Gateway {
                    gateway_ref: gateway.gateway_ref,
                    binding: binding.clone(),
                },
                revision: 1,
                descriptor,
            };
            if let Some(mut stored) = self.products.load(actor.person_id, id).await? {
                let ConnectionsPayload::Integration(current) = &stored.payload else {
                    return Err(AgentFailure::Conflict);
                };
                if current.target
                    != (IntegrationBinding::Gateway {
                        gateway_ref: gateway.gateway_ref,
                        binding: binding.clone(),
                    })
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                if current.descriptor == value.descriptor {
                    continue;
                }
                let mut value = value;
                value.revision = current
                    .revision
                    .checked_add(1)
                    .ok_or(AgentFailure::Conflict)?;
                let previous = stored.revision;
                stored.revision += 1;
                stored.payload = ConnectionsPayload::Integration(value);
                self.products.compare_and_swap(previous, stored).await?;
            } else {
                let command = opaque_ref(b"floe.integration.admission.v1", actor.person_id, &id)?;
                self.products
                    .insert(record(
                        actor,
                        id,
                        command,
                        digest(&(gateway.gateway_ref, &value.descriptor.connector_id))?,
                        ConnectionsPayload::Integration(value),
                    ))
                    .await?;
            }
        }
        *self
            .catalog_gateway
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)? = Some(gateway.gateway_ref);
        Ok(())
    }
    pub async fn prepare_integration_review(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        integration_ref: Uuid,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<IntegrationReview, ConnectionsCommandFailure> {
        let mut classify: fn(AgentFailure) -> ConnectionsCommandFailure =
            ConnectionsCommandFailure::NotAdmitted;
        let mut command_identity = None;
        let result: Result<IntegrationReview, AgentFailure> = async {
            self.ensure_open()?;
            check(actor, scope)?;
            let intent = digest(&("integration_review", integration_ref, expected_revision))?;
            let id = command_ref(actor.person_id, command_id);
            command_identity = Some(ConnectionsCommandIdentity {
                journal: crate::ConnectionsCommandJournal::Product,
                record_ref: id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                command_id,
                intent_digest: intent,
            });
            if let Some(record) = self.command(actor, id, command_id, intent).await? {
                classify = ConnectionsCommandFailure::Admitted;
                return match record.payload {
                    ConnectionsPayload::IntegrationReview(value) => {
                        Ok(project_integration_review(value.summary))
                    }
                    _ => Err(AgentFailure::Conflict),
                };
            }
            let integration = match self.products.load(actor.person_id, integration_ref).await? {
                Some(stored) => {
                    let ConnectionsPayload::Integration(integration) = stored.payload else {
                        return Err(AgentFailure::InvalidInput);
                    };
                    integration
                }
                None => self
                    .native_integration(actor, integration_ref)
                    .await?
                    .ok_or(AgentFailure::NotFound)?,
            };
            if integration.revision != expected_revision
                || integration.descriptor.state != IntegrationState::Available
            {
                return Err(AgentFailure::Conflict);
            }
            self.require_integration_current(actor, &integration, scope)
                .await?;
            let mut summary = IntegrationReview {
                review_ref: ReviewRef {
                    id,
                    revision: 1,
                    digest: [0; 32],
                },
                integration_ref,
                catalog_revision: integration.descriptor.catalog_revision,
                target: integration.target.public_target(),
                display_name: integration.descriptor.display_name.clone(),
                setup_kind: integration.descriptor.setup_kind,
                expires_at: Utc::now() + Duration::minutes(15),
                allowed_actions: vec![ConnectionAction::Start],
            };
            summary.review_ref.digest = digest(&(id, &integration, summary.expires_at))?;
            let descriptor = IntegrationReviewDescriptor {
                summary: summary.clone(),
                integration,
            };
            classify = ConnectionsCommandFailure::Indeterminate;
            self.products
                .insert(record(
                    actor,
                    id,
                    command_id,
                    intent,
                    ConnectionsPayload::IntegrationReview(descriptor),
                ))
                .await?;
            Ok(summary)
        }
        .await;
        super::product_commands::settle_product_command(
            self.products.as_ref(),
            result,
            classify,
            command_identity,
        )
        .await
    }
    pub async fn inspect_integration_review(
        &self,
        actor: &OwnerActor,
        reference: ReviewRef,
        scope: &ExecutionScope,
    ) -> Result<IntegrationReview, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        let record = self.product(actor, reference.id).await?;
        let ConnectionsPayload::IntegrationReview(value) = record.payload else {
            return Err(AgentFailure::InvalidInput);
        };
        if value.summary.review_ref != reference {
            return Err(AgentFailure::Conflict);
        }
        Ok(project_integration_review(value.summary))
    }
    pub async fn start_integration(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        integration_ref: Uuid,
        review_ref: ReviewRef,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<ConnectionOperationSnapshot, ConnectionsCommandFailure> {
        let mut classify: fn(AgentFailure) -> ConnectionsCommandFailure =
            ConnectionsCommandFailure::NotAdmitted;
        let mut command_identity = None;
        let result: Result<ConnectionOperationSnapshot, AgentFailure> = async {
            self.ensure_open()?;
            check(actor, scope)?;
            let intent = digest(&(
                "integration_start",
                integration_ref,
                &review_ref,
                expected_revision,
            ))?;
            let id = command_ref(actor.person_id, command_id);
            command_identity = Some(ConnectionsCommandIdentity {
                journal: crate::ConnectionsCommandJournal::Product,
                record_ref: id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                command_id,
                intent_digest: intent,
            });
            if let Some(record) = self.command(actor, id, command_id, intent).await? {
                classify = ConnectionsCommandFailure::Admitted;
                return match record.payload {
                    ConnectionsPayload::IntegrationOperation(value) => Ok(value.snapshot),
                    ConnectionsPayload::NativeSetup { snapshot, .. } => Ok(snapshot),
                    _ => Err(AgentFailure::Conflict),
                };
            }
            let review = self.product(actor, review_ref.id).await?;
            let ConnectionsPayload::IntegrationReview(review) = review.payload else {
                return Err(AgentFailure::InvalidInput);
            };
            if review.summary.review_ref != review_ref
                || review.summary.expires_at <= Utc::now()
                || review.integration.integration_ref != integration_ref
                || review.integration.revision != expected_revision
            {
                return Err(AgentFailure::Conflict);
            }
            self.require_integration_current(actor, &review.integration, scope)
                .await?;
            if matches!(review.integration.target, IntegrationBinding::Device { .. }) {
                let snapshot = ConnectionOperationSnapshot {
                    operation_ref: id,
                    revision: 1,
                    state: ConnectionOperationState::Pending,
                    launch_action: None,
                    display_code: None,
                    source: None,
                    allowed_actions: vec![ConnectionAction::Reobserve],
                    failure: None,
                    next_observation_after_ms: Some(2000),
                };
                classify = ConnectionsCommandFailure::Indeterminate;
                let pending = self
                    .products
                    .insert(record(
                        actor,
                        id,
                        command_id,
                        intent,
                        ConnectionsPayload::NativeSetup {
                            snapshot,
                            source: self
                                .prepare_native_pending(actor, &review.integration)
                                .await?,
                            reviewed: review,
                            dispatched: false,
                        },
                    ))
                    .await?;
                classify = ConnectionsCommandFailure::Admitted;
                let ConnectionsPayload::NativeSetup { snapshot, .. } = &pending.payload else {
                    return Err(AgentFailure::Conflict);
                };
                let snapshot = snapshot.clone();
                self.spawn_integration(actor.clone(), pending, true, scope)?;
                return Ok(snapshot);
            }
            let (gateway_ref, binding) = review.integration.target.gateway()?;
            let remote = IntegrationOperationRef {
                operation_id: id,
                gateway_ref,
                expected: binding.clone(),
                connector_id: review.integration.descriptor.connector_id.clone(),
                remote_operation_ref: id,
                remote_revision: 0,
            };
            let snapshot = ConnectionOperationSnapshot {
                operation_ref: id,
                revision: 1,
                state: ConnectionOperationState::Pending,
                launch_action: None,
                display_code: None,
                source: None,
                allowed_actions: vec![ConnectionAction::Reobserve],
                failure: None,
                next_observation_after_ms: Some(2000),
            };
            let operation = IntegrationOperationRecord {
                snapshot,
                remote,
                reviewed: review,
                connection_id: None,
                cancellation_command: None,
            };
            classify = ConnectionsCommandFailure::Indeterminate;
            let pending = self
                .products
                .insert(record(
                    actor,
                    id,
                    command_id,
                    intent,
                    ConnectionsPayload::IntegrationOperation(operation),
                ))
                .await?;
            classify = ConnectionsCommandFailure::Admitted;
            let ConnectionsPayload::IntegrationOperation(operation) = &pending.payload else {
                return Err(AgentFailure::Conflict);
            };
            let snapshot = operation.snapshot.clone();
            self.spawn_integration(actor.clone(), pending, true, scope)?;
            Ok(snapshot)
        }
        .await;
        super::product_commands::settle_product_command(
            self.products.as_ref(),
            result,
            classify,
            command_identity,
        )
        .await
    }
    pub async fn get_operation(
        &self,
        actor: &OwnerActor,
        operation_ref: Uuid,
        scope: &ExecutionScope,
    ) -> Result<ConnectionOperationSnapshot, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        if let Some(record) = self.products.load(actor.person_id, operation_ref).await? {
            if record.device_id != actor.device_id {
                return Err(AgentFailure::PolicyDenied);
            }
            match record.payload {
                ConnectionsPayload::IntegrationOperation(operation) => {
                    return Ok(operation.snapshot);
                }
                ConnectionsPayload::NativeSetup { snapshot, .. } => return Ok(snapshot),
                _ => {}
            }
        }
        let operation = self
            .sources
            .load_operation(operation_ref)
            .await
            .map_err(source_error)?
            .ok_or(AgentFailure::NotFound)?;
        if operation.device_id != actor.device_id
            || operation.expected.source.person_id() != actor.person_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        self.source_operation_snapshot(actor, &operation, scope)
            .await
    }
    pub async fn reconcile_operation(
        &self,
        actor: &OwnerActor,
        operation_ref: Uuid,
        scope: &ExecutionScope,
    ) -> Result<ConnectionOperationSnapshot, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        if let Some(record) = self.products.load(actor.person_id, operation_ref).await? {
            return if matches!(record.payload, ConnectionsPayload::NativeSetup { .. }) {
                self.drive_native_setup(actor, record, scope).await
            } else {
                self.drive_integration(actor, record, false, scope).await
            };
        }
        let operation = self.reconcile(actor, operation_ref, None, scope).await?;
        self.source_operation_snapshot(actor, &operation, scope)
            .await
    }
    pub async fn cancel_operation(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        operation_ref: Uuid,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<ConnectionOperationSnapshot, ConnectionsCommandFailure> {
        let mut classify: fn(AgentFailure) -> ConnectionsCommandFailure =
            ConnectionsCommandFailure::NotAdmitted;
        let mut command_identity = None;
        let result: Result<ConnectionOperationSnapshot, AgentFailure> = async {
            self.ensure_open()?;
            check(actor, scope)?;
            let intent = digest(&("cancel_operation", operation_ref, expected_revision))?;
            let id = command_ref(actor.person_id, command_id);
            command_identity = Some(ConnectionsCommandIdentity {
                journal: ConnectionsCommandJournal::Product,
                record_ref: id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                command_id,
                intent_digest: intent,
            });
            if let Some(record) = self.command(actor, id, command_id, intent).await? {
                classify = ConnectionsCommandFailure::Admitted;
                return self
                    .observe_cancellation_admission(actor, record, scope)
                    .await;
            }
            let observed = self.get_operation(actor, operation_ref, scope).await?;
            if command_id.is_nil()
                || expected_revision == 0
                || expected_revision > observed.revision
                || !observed.allowed_actions.contains(&ConnectionAction::Cancel)
            {
                return Err(AgentFailure::Conflict);
            }
            classify = ConnectionsCommandFailure::Indeterminate;
            let pending = self
                .products
                .admit_cancellation(record(
                    actor,
                    id,
                    command_id,
                    intent,
                    ConnectionsPayload::CancellationIntent {
                        operation_ref,
                        expected_revision,
                    },
                ))
                .await?;
            classify = ConnectionsCommandFailure::Admitted;
            self.observe_cancellation_admission(actor, pending, scope)
                .await
        }
        .await;
        super::product_commands::settle_product_command(
            self.products.as_ref(),
            result,
            classify,
            command_identity,
        )
        .await
    }
    async fn observe_cancellation_admission(
        &self,
        actor: &OwnerActor,
        receipt: ConnectionsRecord,
        scope: &ExecutionScope,
    ) -> Result<ConnectionOperationSnapshot, AgentFailure> {
        let operation_ref = match &receipt.payload {
            ConnectionsPayload::CancellationReceipt(snapshot) => return Ok(snapshot.clone()),
            ConnectionsPayload::CancellationIntent { operation_ref, .. } => *operation_ref,
            _ => return Err(AgentFailure::Conflict),
        };
        // Admission survives observer cancellation. Only the registered owner
        // job executes the cancellation; queries never enact it.
        self.spawn_integration(actor.clone(), receipt, false, scope)?;
        let mut snapshot = self.get_operation(actor, operation_ref, scope).await?;
        snapshot
            .allowed_actions
            .retain(|action| *action != ConnectionAction::Cancel);
        Ok(snapshot)
    }
    async fn drive_cancellation(
        &self,
        actor: &OwnerActor,
        mut receipt: ConnectionsRecord,
        scope: &ExecutionScope,
    ) -> Result<ConnectionOperationSnapshot, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        if receipt.person_id != actor.person_id || receipt.device_id != actor.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let (operation_ref, expected_revision) = match receipt.payload {
            ConnectionsPayload::CancellationReceipt(ref snapshot) => return Ok(snapshot.clone()),
            ConnectionsPayload::CancellationIntent {
                operation_ref,
                expected_revision,
            } => (operation_ref, expected_revision),
            _ => return Err(AgentFailure::Conflict),
        };
        let stored = self
            .products
            .load(actor.person_id, operation_ref)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if stored.device_id != actor.device_id || stored.revision < expected_revision {
            return Err(AgentFailure::PolicyDenied);
        }
        let ConnectionsPayload::IntegrationOperation(operation) = &stored.payload else {
            return Err(AgentFailure::InvalidInput);
        };
        if operation.cancellation_command != Some(receipt.command_id) {
            return Err(AgentFailure::Conflict);
        }
        let snapshot = operation.snapshot.clone();
        if !matches!(
            snapshot.state,
            ConnectionOperationState::Completed
                | ConnectionOperationState::Cancelled
                | ConnectionOperationState::Failed
        ) {
            self.spawn_integration(actor.clone(), stored, false, scope)?;
        }
        let previous = receipt.revision;
        receipt.revision = previous.checked_add(1).ok_or(AgentFailure::Conflict)?;
        receipt.payload = ConnectionsPayload::CancellationReceipt(snapshot.clone());
        self.products.compare_and_swap(previous, receipt).await?;
        Ok(snapshot)
    }
    async fn drive_integration(
        &self,
        actor: &OwnerActor,
        mut record: ConnectionsRecord,
        start: bool,
        scope: &ExecutionScope,
    ) -> Result<ConnectionOperationSnapshot, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        if record.person_id != actor.person_id || record.device_id != actor.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let ConnectionsPayload::IntegrationOperation(mut operation) = record.payload.clone() else {
            return Err(AgentFailure::InvalidInput);
        };
        if matches!(
            operation.snapshot.state,
            ConnectionOperationState::Completed
                | ConnectionOperationState::Cancelled
                | ConnectionOperationState::Failed
        ) {
            return Ok(operation.snapshot);
        }
        let _lease = match NativeDriveLease::acquire(self.native_active.clone(), record.record_ref)?
        {
            Some(lease) => lease,
            None => return Ok(operation.snapshot),
        };
        let begin = BeginIntegration {
            operation_id: operation.remote.operation_id,
            gateway_ref: operation.remote.gateway_ref,
            expected: operation.remote.expected.clone(),
            connector_id: operation.remote.connector_id.clone(),
            expected_catalog_revision: operation.reviewed.summary.catalog_revision,
            selection: operation
                .reviewed
                .integration
                .descriptor
                .initial_selection
                .clone(),
        };
        // The stored command is the sole replay identity. A missing remote
        // receipt may rejoin that exact command; it never creates a new attempt.
        let mut observed = if start && operation.cancellation_command.is_none() {
            self.remote_integrations.begin(begin.clone(), scope).await
        } else {
            self.remote_integrations
                .observe(&operation.remote, scope)
                .await
        };
        if matches!(&observed, Err(IntegrationError::NotFound))
            && operation.remote.remote_revision == 0
            && operation.cancellation_command.is_none()
        {
            observed = self.remote_integrations.begin(begin, scope).await;
        }
        if operation.cancellation_command.is_some()
            && matches!(observed, Err(IntegrationError::NotFound))
        {
            // Absence does not exclude a delayed pre-cancel request. Preserve
            // uncertainty and poll; never create a remote attempt to cancel it.
            let notice = failure(
                record.record_ref,
                ConnectionFailureReason::OperationUncertain,
                ConnectionRecovery::Reobserve,
            );
            if operation.snapshot.failure.as_ref() != Some(&notice) {
                let previous = record.revision;
                record.revision = previous.checked_add(1).ok_or(AgentFailure::Conflict)?;
                operation.snapshot.revision = record.revision;
                operation.snapshot.failure = Some(notice);
                operation.snapshot.allowed_actions = vec![ConnectionAction::Reobserve];
                operation.snapshot.next_observation_after_ms = Some(2000);
                record.payload = ConnectionsPayload::IntegrationOperation(operation.clone());
                self.products.compare_and_swap(previous, record).await?;
            }
            return Ok(operation.snapshot);
        }
        let observed = if operation.cancellation_command.is_some() {
            let remote = observed.map_err(integration_error)?;
            operation.remote = remote.reference;
            self.remote_integrations
                .cancel(
                    CancelIntegration {
                        expected_remote_revision: operation.remote.remote_revision,
                        operation: operation.remote.clone(),
                    },
                    scope,
                )
                .await
        } else {
            observed
        }
        .map_err(integration_error)?;
        if observed.reference.operation_id != operation.remote.operation_id
            || observed.reference.expected != operation.remote.expected
            || observed.reference.connector_id != operation.remote.connector_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        operation.remote = observed.reference;
        operation.connection_id = Some(observed.connection_id.clone());
        let management_launch = retain_valid_launch(
            operation.snapshot.launch_action.as_ref(),
            observed.management_launch,
            Utc::now(),
        );
        operation.snapshot.launch_action = if operation.cancellation_command.is_none() {
            management_launch
        } else {
            None
        };
        if operation.cancellation_command.is_some() {
            operation.snapshot.display_code = None;
        }
        operation.snapshot.state = match observed.state {
            RemoteIntegrationState::AwaitingUser => ConnectionOperationState::AwaitingUser,
            RemoteIntegrationState::Pending => ConnectionOperationState::Running,
            RemoteIntegrationState::Connected => ConnectionOperationState::Completed,
            RemoteIntegrationState::Failed => ConnectionOperationState::Failed,
            RemoteIntegrationState::Cancelled => ConnectionOperationState::Cancelled,
        };
        if observed.state == RemoteIntegrationState::Connected {
            let source = observed.source.ok_or(AgentFailure::PolicyDenied)?;
            let proposed = SourceConnection::establish_remote(
                actor.person_id,
                operation.remote.connector_id.clone(),
                observed.connection_id.clone(),
                source.execution_owner_id,
                source.resources,
                source.source_authority,
            )
            .map_err(|_| AgentFailure::PolicyDenied)?;
            let source = if let Some(current) = self
                .sources
                .load(actor.person_id, &observed.connection_id)
                .await
                .map_err(source_error)?
            {
                if current.connector_id() != proposed.connector_id()
                    || current.execution_owner_id() != proposed.execution_owner_id()
                    || current.source_authority() != proposed.source_authority()
                    || current.resources() != proposed.resources()
                {
                    return Err(AgentFailure::Conflict);
                }
                current
            } else {
                self.sources.create(&proposed).await.map_err(source_error)?;
                proposed
            };
            operation.snapshot.source = Some(self.source_summary(actor, &source, scope).await?);
        }
        operation.snapshot.failure = None;
        if observed.state == RemoteIntegrationState::Failed {
            operation.snapshot.failure = Some(failure(
                operation.remote.operation_id,
                ConnectionFailureReason::Rejected,
                ConnectionRecovery::NewReview,
            ));
        }
        let active = matches!(
            operation.snapshot.state,
            ConnectionOperationState::Pending
                | ConnectionOperationState::Running
                | ConnectionOperationState::AwaitingUser
        );
        operation.snapshot.allowed_actions = if active && operation.cancellation_command.is_none() {
            vec![ConnectionAction::Cancel, ConnectionAction::Reobserve]
        } else if active {
            vec![ConnectionAction::Reobserve]
        } else {
            vec![]
        };
        operation.snapshot.next_observation_after_ms = active.then_some(2000);
        if matches!(&record.payload,ConnectionsPayload::IntegrationOperation(previous) if previous == &operation)
        {
            return Ok(operation.snapshot);
        }
        let previous = record.revision;
        record.revision = record
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::Conflict)?;
        operation.snapshot.revision = record.revision;
        let snapshot = operation.snapshot.clone();
        record.payload = ConnectionsPayload::IntegrationOperation(operation);
        self.products.compare_and_swap(previous, record).await?;
        if snapshot.state == ConnectionOperationState::Completed {
            self.spawn_catalog_refresh(actor.clone(), scope)?;
        }
        Ok(snapshot)
    }
    pub async fn prepare_source_review(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        source_ref: Uuid,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<SourceReview, ConnectionsCommandFailure> {
        let mut classify: fn(AgentFailure) -> ConnectionsCommandFailure =
            ConnectionsCommandFailure::NotAdmitted;
        let mut command_identity = None;
        let result: Result<SourceReview, AgentFailure> = async {
            self.ensure_open()?;
            check(actor, scope)?;
            let intent = digest(&("source_review", source_ref, expected_revision))?;
            let id = command_ref(actor.person_id, command_id);
            command_identity = Some(ConnectionsCommandIdentity {
                journal: crate::ConnectionsCommandJournal::Product,
                record_ref: id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                command_id,
                intent_digest: intent,
            });
            if let Some(record) = self.command(actor, id, command_id, intent).await? {
                classify = ConnectionsCommandFailure::Admitted;
                return match record.payload {
                    ConnectionsPayload::SourceReview(value) => {
                        Ok(project_source_review(value.summary))
                    }
                    _ => Err(AgentFailure::Conflict),
                };
            }
            let source = self.resolve_source(actor, source_ref).await?;
            if source.revision() != expected_revision || !native_source(&source) {
                return Err(AgentFailure::Conflict);
            }
            if self
                .sources
                .source_is_fenced(actor.person_id, source.connection_id())
                .await
                .map_err(source_error)?
            {
                return Err(AgentFailure::Conflict);
            }
            let expected = if source.state() == SourceState::Pending {
                None
            } else {
                Some(self.evidence.observe(actor, &source, scope).await?)
            };
            let catalog = self.source_catalog.inspect(actor, &source, scope).await?;
            if catalog.source != source
                || !catalog.catalog_complete
                || catalog.resources.len() > 256
                || catalog.catalog_digest == [0; 32]
            {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            let mut resources = Vec::new();
            let mut permitted = Vec::new();
            for resource in catalog.resources {
                let reference =
                    resource_ref(actor.person_id, source.connection_id(), resource.handle())?;
                permitted.push(PermittedResource {
                    resource_ref: reference,
                    label: resource.label().to_owned(),
                    group: resource_group(actor.person_id, source.connection_id(), &resource)?,
                    selected: source
                        .resources()
                        .iter()
                        .any(|selected| selected.handle() == resource.handle()),
                });
                resources.push((reference, resource));
            }
            let source_binding = GrantSourceBinding::try_new(
                actor.person_id,
                source.connection_id().clone(),
                source.connector_id().clone(),
                source.execution_owner_id().clone(),
            )
            .map_err(|_| AgentFailure::InvalidInput)?;
            let current = self
                .access
                .source_observe_state(actor, &source_binding, scope)
                .await?;
            let processing_views = current
                .views
                .into_iter()
                .map(|view| {
                    ViewProcessingDisclosure {
                        view_id: view.view_id,
                        data_class: view.data_class,
                        data_categories: view.categories,
                        current: view.processing,
                        // Resource configuration invalidates old Observe grants; it
                        // cannot create or expand a processing permission.
                        requested: None,
                    }
                })
                .collect();
            let mut summary = SourceReview {
                review_ref: ReviewRef {
                    id,
                    revision: 1,
                    digest: [0; 32],
                },
                source_ref,
                source_revision: source.revision(),
                labels: vec![source_label(source.connector_id().as_str()).into()],
                permitted_choices: permitted,
                processing_disclosure: ProcessingDisclosure {
                    views: processing_views,
                },
                expires_at: Utc::now() + Duration::minutes(15),
                allowed_actions: vec![ConnectionAction::Configure],
            };
            summary.review_ref.digest = digest(&(
                id,
                &source,
                &expected,
                &resources,
                catalog.catalog_digest,
                summary.expires_at,
            ))?;
            let descriptor = SourceReviewDescriptor {
                summary: summary.clone(),
                source,
                expected,
                resources,
                catalog_digest: catalog.catalog_digest,
            };
            classify = ConnectionsCommandFailure::Indeterminate;
            self.products
                .insert(record(
                    actor,
                    id,
                    command_id,
                    intent,
                    ConnectionsPayload::SourceReview(descriptor),
                ))
                .await?;
            Ok(summary)
        }
        .await;
        super::product_commands::settle_product_command(
            self.products.as_ref(),
            result,
            classify,
            command_identity,
        )
        .await
    }
    pub async fn inspect_source_review(
        &self,
        actor: &OwnerActor,
        reference: ReviewRef,
        scope: &ExecutionScope,
    ) -> Result<SourceReview, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        let record = self.product(actor, reference.id).await?;
        let ConnectionsPayload::SourceReview(value) = record.payload else {
            return Err(AgentFailure::InvalidInput);
        };
        if value.summary.review_ref != reference {
            return Err(AgentFailure::Conflict);
        }
        Ok(project_source_review(value.summary))
    }
    pub async fn configure_source(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        source_ref: Uuid,
        reference: ReviewRef,
        selected_resources: Vec<Uuid>,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<SourceSummary, ConnectionsCommandFailure> {
        let mut classify: fn(AgentFailure) -> ConnectionsCommandFailure =
            ConnectionsCommandFailure::NotAdmitted;
        let mut command_identity = None;
        let result: Result<SourceSummary, AgentFailure> = async {
            self.ensure_open()?;
            check(actor, scope)?;
            let intent = digest(&(
                "source_configure",
                source_ref,
                &reference,
                &selected_resources,
                expected_revision,
            ))?;
            let id = command_ref(actor.person_id, command_id);
            command_identity = Some(ConnectionsCommandIdentity {
                journal: crate::ConnectionsCommandJournal::Product,
                record_ref: id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                command_id,
                intent_digest: intent,
            });
            if let Some(record) = self.command(actor, id, command_id, intent).await? {
                classify = ConnectionsCommandFailure::Admitted;
                return match record.payload.clone() {
                    ConnectionsPayload::SourceMutation { summary, .. } => Ok(summary),
                    ConnectionsPayload::SourceConfigurationAborted { reason, .. } => {
                        classify = ConnectionsCommandFailure::NotApplied;
                        Err(aborted_reason(reason))
                    }
                    ConnectionsPayload::SourceConfiguration { .. } => {
                        match self
                            .execute_source_configuration(actor, record, scope)
                            .await?
                        {
                            SourceConfigurationOutcome::Applied(summary) => Ok(summary),
                            SourceConfigurationOutcome::NotApplied(reason) => {
                                classify = ConnectionsCommandFailure::NotApplied;
                                Err(reason)
                            }
                        }
                    }
                    _ => Err(AgentFailure::Conflict),
                };
            }
            let reviewed = self.product(actor, reference.id).await?;
            let ConnectionsPayload::SourceReview(descriptor) = reviewed.payload else {
                return Err(AgentFailure::InvalidInput);
            };
            if descriptor.summary.review_ref != reference
                || descriptor.summary.source_ref != source_ref
                || descriptor.summary.source_revision != expected_revision
                || descriptor.summary.expires_at <= Utc::now()
            {
                return Err(AgentFailure::Conflict);
            }
            let source = self.resolve_source(actor, source_ref).await?;
            if source.revision() != expected_revision || !native_source(&source) {
                return Err(AgentFailure::Conflict);
            }
            if source != descriptor.source {
                return Err(AgentFailure::Conflict);
            }
            if let Some(expected) = &descriptor.expected {
                if &self.evidence.observe(actor, &source, scope).await? != expected {
                    return Err(AgentFailure::Conflict);
                }
            }
            let catalog = self.source_catalog.inspect(actor, &source, scope).await?;
            if !catalog.catalog_complete || catalog.catalog_digest != descriptor.catalog_digest {
                return Err(AgentFailure::Conflict);
            }
            if selected_resources.is_empty()
                || selected_resources.len() > 256
                || selected_resources
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != selected_resources.len()
            {
                return Err(AgentFailure::InvalidInput);
            }
            let resources = selected_resources
                .iter()
                .map(|id| {
                    descriptor
                        .resources
                        .iter()
                        .find(|(reference, _)| reference == id)
                        .map(|(_, resource)| resource.clone())
                        .ok_or(AgentFailure::PolicyDenied)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let fingerprint = self
                .evidence
                .inspect_selection(actor, &source, &resources, scope)
                .await?;
            let mut successor = source.clone();
            let changed = successor
                .configure_reviewed_native(
                    expected_revision,
                    source.resource_mode(),
                    resources,
                    fingerprint,
                )
                .map_err(|_| AgentFailure::Conflict)?;
            if !changed {
                let summary = self.source_summary(actor, &source, scope).await?;
                classify = ConnectionsCommandFailure::Indeterminate;
                self.products
                    .insert(record(
                        actor,
                        id,
                        command_id,
                        intent,
                        ConnectionsPayload::SourceMutation {
                            source,
                            summary: summary.clone(),
                        },
                    ))
                    .await?;
                return Ok(summary);
            }
            let operation_id = super::source_operation::derived_id(
                b"floe.source.operation.v1",
                actor.person_id,
                command_id,
            );
            classify = ConnectionsCommandFailure::Indeterminate;
            let pending = self
                .products
                .insert(record(
                    actor,
                    id,
                    command_id,
                    intent,
                    ConnectionsPayload::SourceConfiguration {
                        descriptor,
                        selected_resources,
                        successor,
                        operation_id,
                    },
                ))
                .await?;
            classify = ConnectionsCommandFailure::Admitted;
            match self
                .execute_source_configuration(actor, pending, scope)
                .await?
            {
                SourceConfigurationOutcome::Applied(summary) => Ok(summary),
                SourceConfigurationOutcome::NotApplied(reason) => {
                    classify = ConnectionsCommandFailure::NotApplied;
                    Err(reason)
                }
            }
        }
        .await;
        super::product_commands::settle_product_command(
            self.products.as_ref(),
            result,
            classify,
            command_identity,
        )
        .await
    }
    pub async fn prepare_observe_review(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        source_ref: Uuid,
        expected_revision: u64,
        requested_processing: ProcessingChoice,
        scope: &ExecutionScope,
    ) -> Result<ObserveReview, ConnectionsCommandFailure> {
        let mut classify: fn(AgentFailure) -> ConnectionsCommandFailure =
            ConnectionsCommandFailure::NotAdmitted;
        let mut command_identity = None;
        let result: Result<ObserveReview, AgentFailure> = async {
            self.ensure_open()?;
            check(actor, scope)?;
            let intent = digest(&(
                "observe_review",
                source_ref,
                expected_revision,
                requested_processing,
            ))?;
            let id = command_ref(actor.person_id, command_id);
            command_identity = Some(ConnectionsCommandIdentity {
                journal: crate::ConnectionsCommandJournal::Product,
                record_ref: id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                command_id,
                intent_digest: intent,
            });
            if let Some(record) = self.command(actor, id, command_id, intent).await? {
                classify = ConnectionsCommandFailure::Admitted;
                return match record.payload {
                    ConnectionsPayload::ObserveReview { reference, .. } => {
                        self.inspect_observe_review(actor, reference, scope).await
                    }
                    _ => Err(AgentFailure::Conflict),
                };
            }
            let choice = match requested_processing {
                ProcessingChoice::DeviceOnly => SourceProcessingChoice::DeviceOnly,
                ProcessingChoice::GatewayAllowed => SourceProcessingChoice::GatewayAllowed,
            };
            if let Some(review) = self
                .access
                .find_source_processing_review(
                    actor,
                    command_id,
                    source_ref,
                    expected_revision,
                    choice,
                    scope,
                )
                .await?
            {
                classify = ConnectionsCommandFailure::Admitted;
                self.products
                    .insert(record(
                        actor,
                        id,
                        command_id,
                        intent,
                        ConnectionsPayload::ObserveReview {
                            reference: review.reference.clone(),
                            source_ref,
                            source_revision: expected_revision,
                            choice: requested_processing,
                        },
                    ))
                    .await?;
                return self.project_observe(actor, review, scope).await;
            }
            let source = self.resolve_source(actor, source_ref).await?;
            if source.revision() != expected_revision
                || self
                    .sources
                    .source_is_fenced(actor.person_id, source.connection_id())
                    .await
                    .map_err(source_error)?
            {
                return Err(AgentFailure::Conflict);
            }
            let expected = self.evidence.observe(actor, &source, scope).await?;
            let choice = match requested_processing {
                ProcessingChoice::DeviceOnly => SourceProcessingChoice::DeviceOnly,
                ProcessingChoice::GatewayAllowed => SourceProcessingChoice::GatewayAllowed,
            };
            classify = ConnectionsCommandFailure::Indeterminate;
            let review = self
                .access
                .prepare_source_processing_review(
                    actor, command_id, source_ref, expected, choice, scope,
                )
                .await?;
            classify = ConnectionsCommandFailure::Admitted;
            self.products
                .insert(record(
                    actor,
                    id,
                    command_id,
                    intent,
                    ConnectionsPayload::ObserveReview {
                        reference: review.reference.clone(),
                        source_ref,
                        source_revision: expected_revision,
                        choice: requested_processing,
                    },
                ))
                .await?;
            self.project_observe(actor, review, scope).await
        }
        .await;
        super::product_commands::settle_product_command(
            self.products.as_ref(),
            result,
            classify,
            command_identity,
        )
        .await
    }
    pub async fn inspect_observe_review(
        &self,
        actor: &OwnerActor,
        reference: ReviewRef,
        scope: &ExecutionScope,
    ) -> Result<ObserveReview, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        let review = self.access.inspect_review(actor, reference, scope).await?;
        self.project_observe(actor, review, scope).await
    }
    pub async fn apply_observe(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        source_ref: Uuid,
        expected_revision: u64,
        reference: ReviewRef,
        _decision: ObserveDecision,
        scope: &ExecutionScope,
    ) -> Result<SourceSummary, ConnectionsCommandFailure> {
        self.ensure_open()
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        check(actor, scope).map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let reviewed = self
            .access
            .inspect_review(actor, reference.clone(), scope)
            .await
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        if crate::source_ref(actor.person_id, &reviewed.source.source.connection_id())
            .map_err(ConnectionsCommandFailure::NotAdmitted)?
            != source_ref
            || reviewed.source.revision != Some(expected_revision)
        {
            return Err(ConnectionsCommandFailure::NotAdmitted(
                AgentFailure::Conflict,
            ));
        }
        let operation = self
            .apply_source_review(actor, command_id, reference, scope)
            .await?;
        if matches!(operation.phase, SourceOperationPhase::Aborted { .. }) {
            // The Access abort receipt proves that no grant commit for this
            // exact operation can appear later; all deliveries are settled.
            return Err(ConnectionsCommandFailure::NotApplied(
                AgentFailure::AccessReviewRequired,
            ));
        }
        let result: Result<SourceSummary, AgentFailure> = async {
            if !matches!(operation.phase, SourceOperationPhase::Completed { .. }) {
                return Err(AgentFailure::AccessReviewRequired);
            }
            let source = self
                .sources
                .load(actor.person_id, &operation.expected.source.connection_id())
                .await
                .map_err(source_error)?
                .ok_or(AgentFailure::NotFound)?;
            self.source_summary(actor, &source, scope).await
        }
        .await;
        result.map_err(ConnectionsCommandFailure::Admitted)
    }
    pub async fn pause_observe(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        source_ref: Uuid,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<SourceSummary, ConnectionsCommandFailure> {
        self.ensure_open()
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        check(actor, scope).map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let source = self
            .resolve_source(actor, source_ref)
            .await
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let operation = self
            .pause_observe_operation(
                actor,
                command_id,
                source.connection_id(),
                expected_revision,
                scope,
            )
            .await?;
        let result: Result<SourceSummary, AgentFailure> = async {
            if !matches!(operation.phase, SourceOperationPhase::Completed { .. }) {
                return Err(AgentFailure::AccessReviewRequired);
            }
            let current = self
                .sources
                .load(actor.person_id, source.connection_id())
                .await
                .map_err(source_error)?
                .ok_or(AgentFailure::NotFound)?;
            self.source_summary(actor, &current, scope).await
        }
        .await;
        result.map_err(ConnectionsCommandFailure::Admitted)
    }
    pub async fn disconnect(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        source_ref: Uuid,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<ConnectionOperationSnapshot, ConnectionsCommandFailure> {
        self.ensure_open()
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        check(actor, scope).map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let source = self
            .resolve_source(actor, source_ref)
            .await
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let operation = self
            .disconnect_source_operation(
                actor,
                command_id,
                source.connection_id(),
                expected_revision,
                scope,
            )
            .await?;
        let result: Result<ConnectionOperationSnapshot, AgentFailure> = async {
            self.source_operation_snapshot(actor, &operation, scope)
                .await
        }
        .await;
        result.map_err(ConnectionsCommandFailure::Admitted)
    }
    pub async fn request_management_launch(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        gateway_ref: Uuid,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<ValidatedManagementLaunch, ConnectionsCommandFailure> {
        let mut classify: fn(AgentFailure) -> ConnectionsCommandFailure =
            ConnectionsCommandFailure::NotAdmitted;
        let mut command_identity = None;
        let result: Result<ValidatedManagementLaunch, AgentFailure> = async {
            self.ensure_open()?;
            check(actor, scope)?;
            let intent = digest(&("management_launch", gateway_ref, expected_revision))?;
            let id = command_ref(actor.person_id, command_id);
            command_identity = Some(ConnectionsCommandIdentity {
                journal: crate::ConnectionsCommandJournal::Product,
                record_ref: id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                command_id,
                intent_digest: intent,
            });
            if let Some(record) = self.command(actor, id, command_id, intent).await? {
                classify = ConnectionsCommandFailure::Admitted;
                return match record.payload {
                    ConnectionsPayload::Launch(action) => Ok(action),
                    _ => Err(AgentFailure::Conflict),
                };
            }
            let (gateway, binding) = self.current_gateway(actor, gateway_ref).await?;
            if gateway.revision != expected_revision {
                return Err(AgentFailure::Conflict);
            }
            // The port only derives a launch action from current verified
            // connection metadata. It does not launch a browser or persist it.
            let action = self
                .remote_integrations
                .management_launch(
                    ManagementLaunchRequest {
                        operation_id: id,
                        gateway_ref,
                        expected: binding,
                        expected_binding_generation: expected_revision,
                        purpose: LaunchPurpose::ManageGateway,
                    },
                    scope,
                )
                .await
                .map_err(integration_error)?;
            if action.action_ref != id || action.purpose != LaunchPurpose::ManageGateway {
                return Err(AgentFailure::PolicyDenied);
            }
            classify = ConnectionsCommandFailure::Indeterminate;
            self.products
                .insert(record(
                    actor,
                    id,
                    command_id,
                    intent,
                    ConnectionsPayload::Launch(action.clone()),
                ))
                .await?;
            Ok(action)
        }
        .await;
        super::product_commands::settle_product_command(
            self.products.as_ref(),
            result,
            classify,
            command_identity,
        )
        .await
    }
    fn operation_scope(&self, scope: &ExecutionScope) -> ExecutionScope {
        ExecutionScope::root(
            floe_execution::Cancellation::new(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(300),
            scope.budget().child(0, 0),
            scope.trace_context(),
        )
    }
    async fn record_integration_repair(
        &self,
        actor: &OwnerActor,
        id: Uuid,
    ) -> Result<(), AgentFailure> {
        let mut record = self
            .products
            .load(actor.person_id, id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if record.device_id != actor.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let ConnectionsPayload::IntegrationOperation(ref mut operation) = record.payload else {
            return Err(AgentFailure::InvalidInput);
        };
        if matches!(
            operation.snapshot.state,
            ConnectionOperationState::Completed
                | ConnectionOperationState::Cancelled
                | ConnectionOperationState::Failed
                | ConnectionOperationState::RepairRequired
        ) {
            return Ok(());
        }
        let previous = record.revision;
        record.revision = previous.checked_add(1).ok_or(AgentFailure::Conflict)?;
        operation.snapshot.revision = record.revision;
        operation.snapshot.state = ConnectionOperationState::RepairRequired;
        operation.snapshot.failure = Some(failure(
            id,
            ConnectionFailureReason::OperationUncertain,
            ConnectionRecovery::None,
        ));
        operation.snapshot.allowed_actions.clear();
        operation.snapshot.next_observation_after_ms = None;
        operation.snapshot.launch_action = None;
        operation.snapshot.display_code = None;
        self.products.compare_and_swap(previous, record).await?;
        Ok(())
    }
    fn spawn_integration(
        &self,
        actor: OwnerActor,
        record: ConnectionsRecord,
        start: bool,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        let Some(lease) = NativeDriveLease::acquire(self.jobs_active.clone(), record.record_ref)?
        else {
            return Ok(());
        };
        let service = self.clone();
        let scope = self.operation_scope(scope);
        let id = record.record_ref;
        self.register_job(id, &scope)?;
        let cancellations = self.jobs_cancel.clone();
        tokio::spawn(async move {
            let _lease = JobLease {
                _active: lease,
                cancellations,
                id,
            };
            let mut first = start;
            let mut retry_delay = std::time::Duration::from_secs(2);
            loop {
                if service.ensure_open().is_err() || scope.cancellation().is_cancelled() {
                    break;
                }
                let round = ExecutionScope::root(
                    scope.cancellation().clone(),
                    tokio::time::Instant::now() + std::time::Duration::from_secs(30),
                    scope.budget().child(0, 0),
                    scope.trace_context(),
                );
                let result = async {
                    let current = service
                        .products
                        .load(actor.person_id, id)
                        .await?
                        .ok_or(AgentFailure::NotFound)?;
                    if matches!(current.payload, ConnectionsPayload::CancellationReceipt(_)) {
                        return Ok(None);
                    }
                    let snapshot = if matches!(
                        current.payload,
                        ConnectionsPayload::CancellationIntent { .. }
                    ) {
                        service.drive_cancellation(&actor, current, &round).await?
                    } else if matches!(current.payload, ConnectionsPayload::NativeSetup { .. }) {
                        service.drive_native_setup(&actor, current, &round).await?
                    } else {
                        service
                            .drive_integration(&actor, current, first, &round)
                            .await?
                    };
                    Ok::<_, AgentFailure>(Some(snapshot))
                }
                .await;
                first = false;
                if result.as_ref().is_ok_and(|value| {
                    value.as_ref().is_none_or(|snapshot| {
                        matches!(
                            snapshot.state,
                            ConnectionOperationState::Completed
                                | ConnectionOperationState::Failed
                                | ConnectionOperationState::Cancelled
                                | ConnectionOperationState::RepairRequired
                        )
                    })
                }) {
                    break;
                }
                if matches!(result, Err(AgentFailure::PolicyDenied)) {
                    if service.record_integration_repair(&actor, id).await.is_ok() {
                        break;
                    }
                }
                if matches!(
                    result,
                    Err(AgentFailure::VaultUnavailable | AgentFailure::VaultLocked)
                ) {
                    break;
                }
                retry_delay = if result.is_err() {
                    (retry_delay * 2).min(std::time::Duration::from_secs(30))
                } else {
                    std::time::Duration::from_secs(2)
                };
                tokio::select! { _=scope.cancellation().cancelled()=>break, _=tokio::time::sleep(retry_delay)=>{} }
            }
        });
        Ok(())
    }
    fn spawn_pairing(
        &self,
        actor: OwnerActor,
        id: Uuid,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        let Some(lease) = NativeDriveLease::acquire(self.jobs_active.clone(), id)? else {
            return Ok(());
        };
        let service = self.clone();
        let scope = self.operation_scope(scope);
        self.register_job(id, &scope)?;
        let cancellations = self.jobs_cancel.clone();
        tokio::spawn(async move {
            let _lease = JobLease {
                _active: lease,
                cancellations,
                id,
            };
            let mut retry_delay = std::time::Duration::from_secs(2);
            loop {
                if service.ensure_open().is_err() || scope.cancellation().is_cancelled() {
                    break;
                }
                // The owner job lives until terminalization or owner shutdown;
                // each transport round has its own bounded deadline. A query
                // never has to restart a silently expired mutation driver.
                let round = ExecutionScope::root(
                    scope.cancellation().clone(),
                    tokio::time::Instant::now() + std::time::Duration::from_secs(30),
                    scope.budget().child(0, 0),
                    scope.trace_context(),
                );
                let result = service.pairing.reconcile_pairing(&actor, id, &round).await;
                if result
                    .as_ref()
                    .is_ok_and(|snapshot| snapshot.state == PairingState::Paired)
                {
                    let _ = service.spawn_catalog_refresh(actor.clone(), &round);
                }
                if result.as_ref().is_ok_and(|snapshot| {
                    snapshot.state.terminal()
                        && !(snapshot.state == PairingState::RepairRequired
                            && snapshot
                                .allowed_actions
                                .contains(&ConnectionAction::Reobserve))
                }) {
                    break;
                }
                if matches!(
                    result,
                    Err(PairingError::StorageUnavailable
                        | PairingError::CredentialUnavailable
                        | PairingError::ForeignIdentity
                        | PairingError::ChangedProducer
                        | PairingError::InvalidInput)
                ) {
                    break;
                }
                retry_delay = if matches!(
                    result,
                    Err(PairingError::StorageBusy
                        | PairingError::Indeterminate
                        | PairingError::TransportUnavailable
                        | PairingError::DeadlineExceeded)
                ) {
                    (retry_delay * 2).min(std::time::Duration::from_secs(30))
                } else {
                    std::time::Duration::from_secs(2)
                };
                tokio::select! {
                    _ = scope.cancellation().cancelled() => break,
                    _ = tokio::time::sleep(retry_delay) => {}
                }
            }
        });
        Ok(())
    }
    fn spawn_catalog_refresh(
        &self,
        actor: OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        let id = opaque_ref(
            b"floe.connections.catalog.refresh.v1",
            actor.person_id,
            &actor.device_id,
        )?;
        self.catalog_dirty
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let Some(lease) = NativeDriveLease::acquire(self.jobs_active.clone(), id)? else {
            return Ok(());
        };
        let service = self.clone();
        let scope = self.operation_scope(scope);
        self.register_job(id, &scope)?;
        let cancellations = self.jobs_cancel.clone();
        tokio::spawn(async move {
            let mut lease = JobLease {
                _active: lease,
                cancellations,
                id,
            };
            loop {
                if service.ensure_open().is_err() || scope.cancellation().is_cancelled() {
                    break;
                }
                let observed_dirty = service
                    .catalog_dirty
                    .load(std::sync::atomic::Ordering::Acquire);
                let round = ExecutionScope::root(
                    scope.cancellation().clone(),
                    tokio::time::Instant::now() + std::time::Duration::from_secs(30),
                    scope.budget().child(0, 0),
                    scope.trace_context(),
                );
                match service.refresh_integrations(&actor, &round).await {
                    Ok(()) => {
                        match lease.finish_if_current(&service.catalog_dirty, observed_dirty) {
                            Ok(true) => break,
                            Ok(false) => continue,
                            Err(_) => {}
                        }
                    }
                    Err(
                        AgentFailure::VaultUnavailable
                        | AgentFailure::VaultLocked
                        | AgentFailure::PolicyDenied,
                    ) => match lease.finish_if_current(&service.catalog_dirty, observed_dirty) {
                        Ok(true) => break,
                        Ok(false) => continue,
                        Err(_) => {}
                    },
                    Err(_) => {}
                }
                tokio::select! {_=scope.cancellation().cancelled()=>break,_=tokio::time::sleep(std::time::Duration::from_secs(10))=>{}}
            }
        });
        Ok(())
    }
    fn project_gateway(&self, mut summary: GatewaySummary) -> Result<GatewaySummary, AgentFailure> {
        if summary.state == GatewayState::Paired {
            if let Some(Err(error)) = *self
                .catalog_status
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?
            {
                let reason = if matches!(
                    error,
                    AgentFailure::StorageUnavailable | AgentFailure::VaultUnavailable
                ) {
                    ConnectionFailureReason::StorageUnavailable
                } else {
                    ConnectionFailureReason::OperationUncertain
                };
                let mut notice = failure(summary.gateway_ref, reason, ConnectionRecovery::None);
                notice.category = floe_kernel::AgentFailureCategory::Transient;
                notice.reload_required = false;
                notice.safe_actions = summary.allowed_actions.clone();
                summary.failure = Some(notice);
            }
        }
        Ok(summary)
    }
    async fn execute_source_configuration(
        &self,
        actor: &OwnerActor,
        mut record: ConnectionsRecord,
        scope: &ExecutionScope,
    ) -> Result<SourceConfigurationOutcome, AgentFailure> {
        let ConnectionsPayload::SourceConfiguration { operation_id, .. } = record.payload else {
            return Err(AgentFailure::InvalidInput);
        };
        loop {
            self.ensure_open()?;
            check(actor, scope)?;
            let current = self
                .products
                .load(actor.person_id, record.record_ref)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.person_id != record.person_id
                || current.device_id != record.device_id
                || current.command_id != record.command_id
                || current.intent_digest != record.intent_digest
            {
                return Err(AgentFailure::Conflict);
            }
            if !matches!(
                current.payload,
                ConnectionsPayload::SourceConfiguration { .. }
            ) {
                return terminal_outcome(current);
            }
            record = current;
            if let Some(active) = NativeDriveLease::acquire(self.jobs_active.clone(), operation_id)?
            {
                // Owner shutdown cancels only this child; never mutate the
                // caller's cancellation token or an enclosing Run's token.
                let drive_scope = scope.child_scope(scope.deadline(), 0, 0, None);
                self.register_job(operation_id, &drive_scope)?;
                let mut handoff = SourceConfigurationForeground {
                    lease: Some(JobLease {
                        _active: active,
                        cancellations: self.jobs_cancel.clone(),
                        id: operation_id,
                    }),
                    service: self.clone(),
                    actor: actor.clone(),
                    pending: Some(record.clone()),
                    scope: scope.clone(),
                };
                let result = self
                    .drive_source_configuration(actor, record, &drive_scope)
                    .await;
                if result.is_ok() {
                    handoff.pending = None;
                }
                return result;
            }
            // Another driver owns this command. Observation is not a second drive
            // and its timeout does not cancel the admitted background operation.
            tokio::select! {
                _ = scope.cancellation().cancelled() => return Err(AgentFailure::Cancelled),
                _ = tokio::time::sleep_until(scope.deadline()) => return Err(AgentFailure::DeadlineExceeded),
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {},
            }
        }
    }

    async fn resume_source_configuration(
        &self,
        actor: &OwnerActor,
        record_ref: Uuid,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        let record = self
            .products
            .load(actor.person_id, record_ref)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if record.person_id != actor.person_id || record.device_id != actor.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        match &record.payload {
            ConnectionsPayload::SourceConfiguration { .. } => self
                .drive_source_configuration(actor, record, scope)
                .await
                .map(|_| ()),
            ConnectionsPayload::SourceMutation { .. }
            | ConnectionsPayload::SourceConfigurationAborted { .. } => Ok(()),
            _ => Err(AgentFailure::InvalidInput),
        }
    }

    fn spawn_source_configuration(
        &self,
        actor: OwnerActor,
        record: ConnectionsRecord,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        let ConnectionsPayload::SourceConfiguration { operation_id, .. } = &record.payload else {
            return Err(AgentFailure::InvalidInput);
        };
        let id = *operation_id;
        let Some(lease) = NativeDriveLease::acquire(self.jobs_active.clone(), id)? else {
            return Ok(());
        };
        let service = self.clone();
        let scope = self.operation_scope(scope);
        self.register_job(id, &scope)?;
        let cancellations = self.jobs_cancel.clone();
        tokio::spawn(async move {
            let _lease = JobLease {
                _active: lease,
                cancellations,
                id,
            };
            let _ = super::background::retry_storage_contention(&scope, || {
                service.resume_source_configuration(&actor, record.record_ref, &scope)
            })
            .await;
        });
        Ok(())
    }
    /// Rejoin saved owner work before handles are published. Native prompts
    /// already dispatched become uncertain; no new permission attempt is made.
    pub async fn activate(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        self.recover_pending(actor, scope).await?;
        self.spawn_catalog_refresh(actor.clone(), scope)?;
        Ok(())
    }
    pub async fn recover_pending(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        let mut records = ProductRecordScan::new(self.products.as_ref(), actor, scope);
        while let Some(record) = records.next().await? {
            if record.device_id != actor.device_id {
                continue;
            }
            if matches!(
                &record.payload,
                ConnectionsPayload::SourceConfiguration { .. }
            ) {
                self.spawn_source_configuration(actor.clone(), record, scope)?;
                continue;
            }
            let pending = match &record.payload {
                ConnectionsPayload::CancellationIntent { .. } => true,
                ConnectionsPayload::IntegrationOperation(operation) => matches!(
                    operation.snapshot.state,
                    ConnectionOperationState::Pending
                        | ConnectionOperationState::Running
                        | ConnectionOperationState::AwaitingUser
                ),
                ConnectionsPayload::NativeSetup { snapshot, .. } => matches!(
                    snapshot.state,
                    ConnectionOperationState::Pending
                        | ConnectionOperationState::Running
                        | ConnectionOperationState::AwaitingUser
                ),
                _ => false,
            };
            if pending {
                self.spawn_integration(actor.clone(), record, false, scope)?
            }
        }
        for operation in self
            .sources
            .list_nonterminal(actor.person_id, 128)
            .await
            .map_err(source_error)?
        {
            if operation.device_id != actor.device_id {
                continue;
            }
            let Some(lease) =
                NativeDriveLease::acquire(self.jobs_active.clone(), operation.operation_id)?
            else {
                continue;
            };
            let service = self.clone();
            let actor = actor.clone();
            let scope = self.operation_scope(scope);
            let id = operation.operation_id;
            self.register_job(id, &scope)?;
            let cancellations = self.jobs_cancel.clone();
            tokio::spawn(async move {
                let _lease = JobLease {
                    _active: lease,
                    cancellations,
                    id,
                };
                let _ = super::background::retry_storage_contention(&scope, || async {
                    if matches!(
                        operation.kind,
                        SourceOperationKind::ConnectionConfigure
                            | SourceOperationKind::ConnectionPresentation
                    ) {
                        service
                            .resume_source_configuration(
                                &actor,
                                command_ref(actor.person_id, operation.command_id),
                                &scope,
                            )
                            .await
                    } else {
                        service
                            .reconcile(&actor, operation.operation_id, None, &scope)
                            .await
                            .map(|_| ())
                    }
                })
                .await;
            });
        }
        for operation in self
            .pairing
            .pending(actor.person_id)
            .await
            .map_err(pairing_error)?
        {
            if operation.device_id == actor.device_id {
                self.spawn_pairing(actor.clone(), operation.operation_id, scope)?
            }
        }
        Ok(())
    }
    async fn native_integration(
        &self,
        actor: &OwnerActor,
        id: Uuid,
    ) -> Result<Option<IntegrationRecord>, AgentFailure> {
        for connector in [
            "calendar.event_kit",
            "contacts.apple",
            "attention.macos",
            "health.apple",
        ] {
            if native_integration_ref(actor, connector)? != id {
                continue;
            }
            let connection =
                floe_context_contract::ConnectionId::try_new(native_connection_id(connector)?)
                    .map_err(|_| AgentFailure::InvalidInput)?;
            let current = self
                .sources
                .load(actor.person_id, &connection)
                .await
                .map_err(source_error)?
                .filter(|source| {
                    source.execution_owner_id().as_str()
                        == floe_access::apple_execution_owner(&actor.device_id)
                });
            let revision = current.as_ref().map_or(1, SourceConnection::revision);
            let available = match connector {
                "health.apple" => cfg!(target_os = "ios"),
                "attention.macos" => false,
                _ => cfg!(any(target_os = "macos", target_os = "ios")),
            };
            return Ok(Some(IntegrationRecord {
                integration_ref: id,
                target: IntegrationBinding::Device {
                    device_id: actor.device_id.clone(),
                },
                revision,
                descriptor: IntegrationDescriptor {
                    connector_id: floe_context_contract::ConnectorId::try_new(connector)
                        .map_err(|_| AgentFailure::InvalidInput)?,
                    display_name: source_label(connector).into(),
                    category: match connector {
                        "calendar.event_kit" => "calendar",
                        "contacts.apple" => "contacts",
                        "health.apple" => "health",
                        _ => "attention",
                    }
                    .into(),
                    setup_kind: IntegrationSetupKind::NativePermission,
                    state: if current
                        .as_ref()
                        .is_some_and(|source| source.state() == SourceState::Ready)
                    {
                        IntegrationState::Connected
                    } else if available {
                        IntegrationState::Available
                    } else {
                        IntegrationState::Unavailable
                    },
                    source_identity: current.as_ref().map(|source| IntegrationSourceIdentity {
                        connection_id: source.connection_id().clone(),
                        execution_owner_id: source.execution_owner_id().clone(),
                        source_authority: source.source_authority(),
                    }),
                    catalog_revision: 1,
                    initial_selection: IntegrationSelection::GatewayManaged,
                },
            }));
        }
        Ok(None)
    }
    async fn prepare_native_pending(
        &self,
        actor: &OwnerActor,
        integration: &IntegrationRecord,
    ) -> Result<SourceConnection, AgentFailure> {
        let IntegrationBinding::Device { device_id } = &integration.target else {
            return Err(AgentFailure::InvalidInput);
        };
        if device_id != &actor.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let connector = integration.descriptor.connector_id.clone();
        let connection =
            floe_context_contract::ConnectionId::try_new(native_connection_id(connector.as_str())?)
                .map_err(|_| AgentFailure::InvalidInput)?;
        if let Some(mut source) = self
            .sources
            .load(actor.person_id, &connection)
            .await
            .map_err(source_error)?
        {
            if source.state() == SourceState::Pending {
                return Ok(source);
            }
            if source.state() != SourceState::Disconnected {
                return Err(AgentFailure::Conflict);
            }
            let previous = source.revision();
            source
                .restart_native_setup(previous)
                .map_err(|_| AgentFailure::Conflict)?;
            return Ok(source);
        }
        let mode = if matches!(connector.as_str(), "health.apple" | "attention.macos") {
            ResourceMode::AllAvailable
        } else {
            ResourceMode::Selected
        };
        let owner = floe_context_contract::ExecutionOwnerId::try_new(
            floe_access::apple_execution_owner(&actor.device_id),
        )
        .map_err(|_| AgentFailure::InvalidInput)?;
        let source = SourceConnection::establish(
            actor.person_id,
            connector,
            connection,
            owner,
            mode,
            vec![],
        )
        .map_err(|_| AgentFailure::InvalidInput)?;
        Ok(source)
    }
    async fn drive_native_setup(
        &self,
        actor: &OwnerActor,
        mut record: ConnectionsRecord,
        scope: &ExecutionScope,
    ) -> Result<ConnectionOperationSnapshot, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        let ConnectionsPayload::NativeSetup {
            mut snapshot,
            reviewed,
            source,
            dispatched,
        } = record.payload.clone()
        else {
            return Err(AgentFailure::InvalidInput);
        };
        if record.person_id != actor.person_id || record.device_id != actor.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        if matches!(
            snapshot.state,
            ConnectionOperationState::Completed
                | ConnectionOperationState::Failed
                | ConnectionOperationState::Cancelled
                | ConnectionOperationState::RepairRequired
        ) {
            return Ok(snapshot);
        }
        let _lease = match NativeDriveLease::acquire(self.native_active.clone(), record.record_ref)?
        {
            Some(lease) => lease,
            None => return Ok(snapshot),
        };
        if dispatched {
            // An interrupted prompt has no durable native receipt. Never open
            // another OS prompt merely because the prior observer disappeared.
            snapshot.state = ConnectionOperationState::RepairRequired;
            snapshot.failure = Some(failure(
                record.record_ref,
                ConnectionFailureReason::OperationUncertain,
                ConnectionRecovery::NewReview,
            ));
            snapshot.allowed_actions.clear();
            snapshot.next_observation_after_ms = None;
        } else {
            // The product admission is already durable before touching source
            // state. A rejected command can never leave a new Pending source.
            match self
                .sources
                .load(actor.person_id, source.connection_id())
                .await
                .map_err(source_error)?
            {
                None => {
                    if source.revision() != 1 {
                        return Err(AgentFailure::Conflict);
                    }
                    self.sources.create(&source).await.map_err(source_error)?;
                }
                Some(current) if current == source => {}
                Some(mut current) if current.state() == SourceState::Disconnected => {
                    let previous = current.revision();
                    current
                        .restart_native_setup(previous)
                        .map_err(|_| AgentFailure::Conflict)?;
                    if current != source {
                        return Err(AgentFailure::Conflict);
                    }
                    self.sources
                        .update(&current, previous)
                        .await
                        .map_err(source_error)?;
                }
                Some(_) => return Err(AgentFailure::Conflict),
            }
            let previous = record.revision;
            record.revision += 1;
            snapshot.revision = record.revision;
            snapshot.state = ConnectionOperationState::AwaitingUser;
            snapshot.allowed_actions = vec![ConnectionAction::Reobserve];
            record.payload = ConnectionsPayload::NativeSetup {
                snapshot: snapshot.clone(),
                reviewed: reviewed.clone(),
                source: source.clone(),
                dispatched: true,
            };
            record = self.products.compare_and_swap(previous, record).await?;
            let result = self
                .native_setup
                .request_permission(
                    actor,
                    NativeSetupRequest {
                        operation_id: record.record_ref,
                        connector_id: reviewed.integration.descriptor.connector_id.clone(),
                        connection_id: source.connection_id().clone(),
                        source_revision: source.revision(),
                    },
                    scope,
                )
                .await;
            match result {
                Ok(observed)
                    if observed.operation_id == record.record_ref
                        && observed.connector_id
                            == reviewed.integration.descriptor.connector_id =>
                {
                    match observed.state {
                        NativeSetupState::Completed => {
                            snapshot.state = ConnectionOperationState::Completed;
                            snapshot.source =
                                Some(self.source_summary(actor, &source, scope).await?);
                        }
                        NativeSetupState::Denied | NativeSetupState::Unavailable => {
                            snapshot.state = ConnectionOperationState::Failed;
                            snapshot.failure = Some(failure(
                                record.record_ref,
                                ConnectionFailureReason::Rejected,
                                ConnectionRecovery::NewReview,
                            ));
                        }
                    }
                }
                _ => {
                    snapshot.state = ConnectionOperationState::RepairRequired;
                    snapshot.failure = Some(failure(
                        record.record_ref,
                        ConnectionFailureReason::OperationUncertain,
                        ConnectionRecovery::NewReview,
                    ));
                }
            }
            snapshot.allowed_actions.clear();
            snapshot.next_observation_after_ms = None;
        }
        let previous = record.revision;
        record.revision += 1;
        snapshot.revision = record.revision;
        record.payload = ConnectionsPayload::NativeSetup {
            snapshot: snapshot.clone(),
            reviewed,
            source,
            dispatched: true,
        };
        self.products.compare_and_swap(previous, record).await?;
        Ok(snapshot)
    }
    async fn product(
        &self,
        actor: &OwnerActor,
        id: Uuid,
    ) -> Result<ConnectionsRecord, AgentFailure> {
        let record = self
            .products
            .load(actor.person_id, id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if record.person_id != actor.person_id || record.device_id != actor.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        record.validate()?;
        Ok(record)
    }
    async fn command(
        &self,
        actor: &OwnerActor,
        id: Uuid,
        command_id: Uuid,
        intent: [u8; 32],
    ) -> Result<Option<ConnectionsRecord>, AgentFailure> {
        if command_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        if let Some(reason) = self
            .products
            .rejected_command(ConnectionsCommandIdentity {
                journal: crate::ConnectionsCommandJournal::Product,
                record_ref: id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                command_id,
                intent_digest: intent,
            })
            .await?
        {
            return Err(reason);
        }
        let Some(record) = self.products.load(actor.person_id, id).await? else {
            return Ok(None);
        };
        if record.person_id != actor.person_id
            || record.device_id != actor.device_id
            || record.command_id != command_id
            || record.intent_digest != intent
        {
            return Err(AgentFailure::Conflict);
        }
        Ok(Some(record))
    }
    async fn current_gateway(
        &self,
        actor: &OwnerActor,
        id: Uuid,
    ) -> Result<(GatewaySummary, floe_access::VerifiedGatewayBinding), AgentFailure> {
        let Some(GatewayObservation::Paired { summary, binding }) = self
            .gateways
            .current(actor.person_id, &actor.device_id)
            .await
            .map_err(pairing_error)?
        else {
            return Err(AgentFailure::CapabilityUnavailable);
        };
        if summary.gateway_ref != id || summary.state != GatewayState::Paired {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok((summary, binding))
    }
    async fn require_integration_current(
        &self,
        actor: &OwnerActor,
        integration: &IntegrationRecord,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        if let IntegrationBinding::Device { device_id } = &integration.target {
            if device_id != &actor.device_id
                || self
                    .native_integration(actor, integration.integration_ref)
                    .await?
                    .as_ref()
                    != Some(integration)
            {
                return Err(AgentFailure::Conflict);
            }
            return Ok(());
        }
        let (gateway_ref, expected) = integration.target.gateway()?;
        let (_, binding) = self.current_gateway(actor, gateway_ref).await?;
        if &binding != expected {
            return Err(AgentFailure::PolicyDenied);
        }
        let current = self.product(actor, integration.integration_ref).await?;
        if !matches!(current.payload,ConnectionsPayload::Integration(ref stored) if stored==integration)
        {
            return Err(AgentFailure::Conflict);
        }
        let catalog = self
            .remote_integrations
            .list(
                IntegrationCatalogQuery {
                    gateway_ref,
                    expected: binding,
                },
                scope,
            )
            .await
            .map_err(integration_error)?;
        if !catalog
            .entries
            .iter()
            .any(|descriptor| descriptor == &integration.descriptor)
        {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }
    async fn resolve_source(
        &self,
        actor: &OwnerActor,
        id: Uuid,
    ) -> Result<SourceConnection, AgentFailure> {
        if id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        for source in self
            .sources
            .list_sources(actor.person_id, 512)
            .await
            .map_err(source_error)?
        {
            if source_ref(actor.person_id, source.connection_id())? == id {
                return Ok(source);
            }
        }
        Err(AgentFailure::NotFound)
    }
    pub(super) async fn source_summary(
        &self,
        actor: &OwnerActor,
        source: &SourceConnection,
        scope: &ExecutionScope,
    ) -> Result<SourceSummary, AgentFailure> {
        let binding = GrantSourceBinding::try_new(
            actor.person_id,
            source.connection_id().clone(),
            source.connector_id().clone(),
            source.execution_owner_id().clone(),
        )
        .map_err(|_| AgentFailure::InvalidInput)?;
        let observed = self
            .access
            .source_observe_state(actor, &binding, scope)
            .await?;
        let observe_state = match observed.status {
            SourceObserveStatus::Enabled => ObserveState::Enabled,
            SourceObserveStatus::Paused => ObserveState::Paused,
            SourceObserveStatus::ReviewRequired => ObserveState::ReviewRequired,
            SourceObserveStatus::Disabled => ObserveState::Disabled,
        };
        let fenced = self
            .sources
            .source_is_fenced(actor.person_id, source.connection_id())
            .await
            .map_err(source_error)?;
        let availability = if fenced {
            SourceAvailability::Unavailable
        } else {
            match source.state() {
                SourceState::Ready => SourceAvailability::Available,
                SourceState::Pending => SourceAvailability::PermissionRequired,
                SourceState::Disconnected => SourceAvailability::Disconnected,
            }
        };
        let mut allowed_actions = Vec::new();
        if !fenced && source.state() != SourceState::Disconnected {
            if native_source(source) {
                allowed_actions.push(ConnectionAction::Configure)
            }
            if source.state() == SourceState::Ready {
                allowed_actions.push(ConnectionAction::Disconnect);
                allowed_actions.push(ConnectionAction::PrepareObserveReview);
                if observe_state == ObserveState::Enabled {
                    allowed_actions.push(ConnectionAction::PauseObserve)
                }
            }
        }
        let selected_resources = source
            .resources()
            .iter()
            .map(|resource| {
                Ok(ResourceSummary {
                    resource_ref: resource_ref(
                        actor.person_id,
                        source.connection_id(),
                        resource.handle(),
                    )?,
                    label: resource.label().to_owned(),
                    group: resource_group(actor.person_id, source.connection_id(), &resource)?,
                })
            })
            .collect::<Result<Vec<_>, AgentFailure>>()?;
        Ok(SourceSummary {
            source_ref: source_ref(actor.person_id, source.connection_id())?,
            revision: source.revision(),
            display_labels: vec![source_label(source.connector_id().as_str()).into()],
            availability,
            last_observed_at: None,
            selected_resources,
            observe_state,
            allowed_actions,
        })
    }
    async fn integration_summary(
        &self,
        actor: &OwnerActor,
        integration: &IntegrationRecord,
        scope: &ExecutionScope,
    ) -> Result<IntegrationSummary, AgentFailure> {
        let catalog_ready = match &integration.target {
            IntegrationBinding::Device { device_id } => device_id == &actor.device_id,
            IntegrationBinding::Gateway {
                gateway_ref,
                binding,
            } => {
                let catalog_current = *self
                    .catalog_gateway
                    .lock()
                    .map_err(|_| AgentFailure::StorageUnavailable)?
                    == Some(*gateway_ref);
                catalog_current
                    && matches!(self.gateways.current(actor.person_id, &actor.device_id).await.map_err(pairing_error)?,
                    Some(GatewayObservation::Paired { summary, binding: current }) if summary.gateway_ref == *gateway_ref && current == *binding)
            }
        };
        let mut source = None;
        if catalog_ready {
            if let Some(identity) = &integration.descriptor.source_identity {
                if let Some(candidate) = self
                    .sources
                    .load(actor.person_id, &identity.connection_id)
                    .await
                    .map_err(source_error)?
                {
                    if integration_source_matches(integration, &candidate) {
                        source = Some(self.source_summary(actor, &candidate, scope).await?);
                    }
                }
            }
        }
        let state = if !catalog_ready {
            IntegrationState::Unavailable
        } else {
            integration.descriptor.state
        };
        let capabilities = if state == IntegrationState::Available {
            vec![IntegrationCapability::PrepareReview]
        } else if state == IntegrationState::Connected {
            vec![IntegrationCapability::Disconnect]
        } else {
            vec![]
        };
        let service_kind = match (
            &integration.target,
            integration.descriptor.connector_id.as_str(),
        ) {
            (IntegrationBinding::Device { .. }, "calendar.event_kit") => {
                IntegrationServiceKind::AppleCalendar
            }
            (IntegrationBinding::Device { .. }, "contacts.apple") => {
                IntegrationServiceKind::AppleContacts
            }
            (IntegrationBinding::Device { .. }, "health.apple") => {
                IntegrationServiceKind::AppleHealth
            }
            (IntegrationBinding::Device { .. }, "attention.macos") => {
                IntegrationServiceKind::AppleAttention
            }
            _ => IntegrationServiceKind::Hosted,
        };
        Ok(IntegrationSummary {
            service_kind,
            integration_ref: integration.integration_ref,
            revision: integration.revision,
            display_name: integration.descriptor.display_name.clone(),
            category: integration.descriptor.category.clone(),
            state,
            capabilities,
            source,
        })
    }
    async fn source_operation_snapshot(
        &self,
        actor: &OwnerActor,
        operation: &SourceOperationRecord,
        scope: &ExecutionScope,
    ) -> Result<ConnectionOperationSnapshot, AgentFailure> {
        let (state, recovery, reason) = match operation.phase {
            SourceOperationPhase::Completed { .. }
            | SourceOperationPhase::PresentationCommitted => (
                ConnectionOperationState::Completed,
                ConnectionRecovery::None,
                None,
            ),
            SourceOperationPhase::Aborted { .. } => (
                ConnectionOperationState::Cancelled,
                ConnectionRecovery::None,
                None,
            ),
            SourceOperationPhase::RepairRequired { .. } => (
                ConnectionOperationState::RepairRequired,
                ConnectionRecovery::Reconcile,
                Some(ConnectionFailureReason::OperationUncertain),
            ),
            _ => (
                ConnectionOperationState::Running,
                ConnectionRecovery::Reobserve,
                None,
            ),
        };
        let source = if state == ConnectionOperationState::Completed {
            match self
                .sources
                .load(actor.person_id, &operation.expected.source.connection_id())
                .await
                .map_err(source_error)?
            {
                Some(source) => Some(self.source_summary(actor, &source, scope).await?),
                None => return Err(AgentFailure::PolicyDenied),
            }
        } else {
            None
        };
        Ok(ConnectionOperationSnapshot {
            operation_ref: operation.operation_id,
            revision: operation.revision,
            state,
            launch_action: None,
            display_code: None,
            source,
            allowed_actions: if operation.phase.holds_fence() {
                vec![ConnectionAction::Reobserve]
            } else {
                vec![]
            },
            failure: reason.map(|reason| failure(operation.operation_id, reason, recovery)),
            next_observation_after_ms: matches!(state, ConnectionOperationState::Running)
                .then_some(2000),
        })
    }
    async fn project_observe(
        &self,
        actor: &OwnerActor,
        review: floe_access::ConnectionReview,
        scope: &ExecutionScope,
    ) -> Result<ObserveReview, AgentFailure> {
        check(actor, scope)?;
        // Product Observe reviews target a configured, stored source. Native
        // setup has already persisted its Pending row before configuration;
        // generic Access source absence is not a product revision.
        let source_revision = review
            .source
            .revision
            .filter(|revision| *revision > 0)
            .ok_or(AgentFailure::Conflict)?;
        let processing_views = review
            .views
            .iter()
            .map(|view| ViewProcessingDisclosure {
                view_id: view.view_id.clone(),
                data_class: view.data_class,
                data_categories: view.categories.clone(),
                current: view.current_processing.clone(),
                requested: Some(view.requested_processing.clone()),
            })
            .collect();
        let mut members = Vec::new();
        for view in &review.views {
            members.push(view.view_id.clone());
        }
        members.sort();
        members.dedup();
        Ok(ObserveReview {
            review_ref: review.reference,
            source_ref: source_ref(actor.person_id, &review.source.source.connection_id())?,
            source_revision,
            display_members: members.clone(),
            processing_disclosure: ProcessingDisclosure {
                views: processing_views,
            },
            expires_at: review.expires_at,
            allowed_actions: if review.expires_at > Utc::now() {
                vec![ConnectionAction::Allow, ConnectionAction::Decline]
            } else {
                vec![]
            },
        })
    }
}
fn record(
    actor: &OwnerActor,
    id: Uuid,
    command_id: Uuid,
    intent: [u8; 32],
    payload: ConnectionsPayload,
) -> ConnectionsRecord {
    ConnectionsRecord {
        record_ref: id,
        person_id: actor.person_id,
        device_id: actor.device_id.clone(),
        command_id,
        intent_digest: intent,
        revision: 1,
        payload,
    }
}
fn command_ref(person: PersonId, command: Uuid) -> Uuid {
    super::source_operation::derived_id(b"floe.connections.command.v1", person, command)
}
fn digest(value: &impl Serialize) -> Result<[u8; 32], AgentFailure> {
    Ok(Sha256::digest(serde_json::to_vec(value).map_err(|_| AgentFailure::InvalidInput)?).into())
}
pub fn source_ref(
    person: PersonId,
    connection: &floe_context_contract::ConnectionId,
) -> Result<Uuid, AgentFailure> {
    opaque_ref(b"floe.source.ref.v1", person, &connection.as_str())
}
fn resource_group(
    person: PersonId,
    connection: &floe_context_contract::ConnectionId,
    resource: &crate::ConnectionResource,
) -> Result<Option<crate::ResourceGroupSummary>, AgentFailure> {
    resource
        .group()
        .map(|group| {
            Ok(crate::ResourceGroupSummary {
                group_ref: opaque_ref(
                    b"floe.resource.group.ref.v1",
                    person,
                    &(connection.as_str(), group.handle.as_str()),
                )?,
                label: group.label.clone(),
            })
        })
        .transpose()
}
fn resource_ref(
    person: PersonId,
    connection: &floe_context_contract::ConnectionId,
    resource: &floe_context_contract::ResourceHandle,
) -> Result<Uuid, AgentFailure> {
    opaque_ref(
        b"floe.resource.ref.v1",
        person,
        &(connection.as_str(), resource.as_str()),
    )
}
fn opaque_ref(
    domain: &[u8],
    person: PersonId,
    value: &impl Serialize,
) -> Result<Uuid, AgentFailure> {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update([0]);
    hash.update(person.to_string().as_bytes());
    hash.update(serde_json::to_vec(value).map_err(|_| AgentFailure::InvalidInput)?);
    let bytes = hash.finalize();
    let mut id = [0u8; 16];
    id.copy_from_slice(&bytes[..16]);
    id[6] = (id[6] & 15) | 64;
    id[8] = (id[8] & 63) | 128;
    Ok(Uuid::from_bytes(id))
}
pub(super) fn native_source(source: &SourceConnection) -> bool {
    matches!(
        source.connector_id().as_str(),
        "calendar.event_kit"
            | "calendar.android"
            | "contacts.apple"
            | "contacts.android"
            | "attention.macos"
            | "health.apple"
    )
}
fn source_label(connector: &str) -> &str {
    match connector {
        "calendar.event_kit" => "Calendar",
        "contacts.apple" => "Contacts",
        "attention.macos" => "Attention",
        "health.apple" => "Health",
        "gmail" => "Gmail",
        "microsoft.mail" => "Microsoft Mail",
        "calendar.google" => "Google Calendar",
        "calendar.microsoft" => "Microsoft Calendar",
        _ => connector,
    }
}
fn project_source_review(mut summary: SourceReview) -> SourceReview {
    if summary.expires_at <= Utc::now() {
        summary.allowed_actions.clear()
    }
    summary
}
fn project_integration_review(mut summary: IntegrationReview) -> IntegrationReview {
    if summary.expires_at <= Utc::now() {
        summary.allowed_actions.clear()
    }
    summary
}
fn check(actor: &OwnerActor, scope: &ExecutionScope) -> Result<(), AgentFailure> {
    super::source_operation::check(actor, scope)
}
fn source_error(error: SourceRepositoryError) -> AgentFailure {
    super::source_operation::source_error(error)
}
pub(crate) fn pairing_error(error: PairingError) -> AgentFailure {
    match error {
        PairingError::ForeignIdentity
        | PairingError::ChangedProducer
        | PairingError::Rejected
        | PairingError::RepairRequired => AgentFailure::PolicyDenied,
        PairingError::Conflict | PairingError::Expired => AgentFailure::Conflict,
        PairingError::Cancelled => AgentFailure::Cancelled,
        PairingError::DeadlineExceeded => AgentFailure::DeadlineExceeded,
        PairingError::InvalidInput => AgentFailure::InvalidInput,
        PairingError::StorageBusy => AgentFailure::StorageBusy,
        PairingError::StorageUnavailable
        | PairingError::CredentialUnavailable
        | PairingError::Indeterminate => AgentFailure::StorageUnavailable,
        PairingError::TransportUnavailable => AgentFailure::CapabilityUnavailable,
    }
}
fn integration_error(error: IntegrationError) -> AgentFailure {
    match error {
        IntegrationError::InvalidInput => AgentFailure::InvalidInput,
        IntegrationError::ForeignIdentity | IntegrationError::InvalidResponse => {
            AgentFailure::PolicyDenied
        }
        IntegrationError::Conflict => AgentFailure::Conflict,
        IntegrationError::Cancelled => AgentFailure::Cancelled,
        IntegrationError::DeadlineExceeded => AgentFailure::DeadlineExceeded,
        IntegrationError::NotFound => AgentFailure::NotFound,
        IntegrationError::Unavailable => AgentFailure::CapabilityUnavailable,
    }
}
fn failure(
    id: Uuid,
    reason: ConnectionFailureReason,
    recovery: ConnectionRecovery,
) -> ConnectionFailure {
    ConnectionFailure {
        domain: ConnectionFailureDomain::Connections,
        category: floe_kernel::AgentFailureCategory::Integrity,
        reason,
        incident_id: id,
        correlation_id: id,
        reload_required: true,
        seal_session: false,
        recovery,
        safe_actions: vec![],
    }
}

fn native_integration_ref(actor: &OwnerActor, connector: &str) -> Result<Uuid, AgentFailure> {
    opaque_ref(
        b"floe.native.integration.v1",
        actor.person_id,
        &(&actor.device_id, connector),
    )
}
fn native_connection_id(connector: &str) -> Result<&'static str, AgentFailure> {
    match connector {
        "calendar.event_kit" => Ok("calendar.event_kit.local"),
        "contacts.apple" => Ok("contacts.apple.local"),
        "health.apple" => Ok("health.apple.local"),
        "attention.macos" => Ok("attention.macos.local"),
        _ => Err(AgentFailure::InvalidInput),
    }
}

struct NativeDriveLease {
    released: bool,
    active: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<Uuid>>>,
    id: Uuid,
}
impl NativeDriveLease {
    fn acquire(
        active: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<Uuid>>>,
        id: Uuid,
    ) -> Result<Option<Self>, AgentFailure> {
        let inserted = active
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .insert(id);
        Ok(inserted.then_some(Self {
            active,
            id,
            released: false,
        }))
    }
}
impl Drop for NativeDriveLease {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        if let Ok(mut active) = self.active.lock() {
            active.remove(&self.id);
        }
    }
}

struct JobLease {
    _active: NativeDriveLease,
    cancellations: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<Uuid, floe_execution::Cancellation>>,
    >,
    id: Uuid,
}
impl JobLease {
    /// Compare the wake generation and release ownership under the same active
    /// lock used by admission. A concurrent wake either keeps this driver alive
    /// or acquires a new lease after release; it cannot fall between them.
    fn finish_if_current(
        &mut self,
        dirty: &std::sync::atomic::AtomicU64,
        observed: u64,
    ) -> Result<bool, AgentFailure> {
        let mut active = self
            ._active
            .active
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        if dirty.load(std::sync::atomic::Ordering::Acquire) != observed {
            return Ok(false);
        }
        let mut jobs = self
            .cancellations
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        jobs.remove(&self.id);
        active.remove(&self.id);
        self._active.released = true;
        Ok(true)
    }
}
impl Drop for JobLease {
    fn drop(&mut self) {
        if self._active.released {
            return;
        }
        if let Ok(mut jobs) = self.cancellations.lock() {
            jobs.remove(&self.id);
        }
    }
}

fn retain_valid_launch(
    previous: Option<&ValidatedManagementLaunch>,
    next: Option<ValidatedManagementLaunch>,
    now: chrono::DateTime<Utc>,
) -> Option<ValidatedManagementLaunch> {
    match (previous, next) {
        (Some(previous), Some(next))
            if previous.action_ref == next.action_ref
                && previous.purpose == next.purpose
                && previous.validated_url == next.validated_url
                && previous.expires_at > now =>
        {
            Some(previous.clone())
        }
        (_, next) => next,
    }
}

/// Display correlation only; source authority admission remains with Access.
fn integration_source_matches(integration: &IntegrationRecord, source: &SourceConnection) -> bool {
    let Some(identity) = &integration.descriptor.source_identity else {
        return false;
    };
    let owner_matches = match &integration.target {
        IntegrationBinding::Device { device_id } => {
            identity.execution_owner_id.as_str() == floe_access::apple_execution_owner(device_id)
        }
        IntegrationBinding::Gateway { .. } => !native_source(source),
    };
    owner_matches
        && source.connection_id() == &identity.connection_id
        && source.connector_id() == &integration.descriptor.connector_id
        && source.execution_owner_id() == &identity.execution_owner_id
        && source.source_authority() == identity.source_authority
        && source.state() != SourceState::Disconnected
}

/// Drop-safe handoff of an admitted command, after releasing the same lease used
/// by recovery. This also covers the foreground future being abandoned mid-await.
struct SourceConfigurationForeground {
    lease: Option<JobLease>,
    service: ConnectionsService,
    actor: OwnerActor,
    pending: Option<ConnectionsRecord>,
    scope: ExecutionScope,
}
impl Drop for SourceConfigurationForeground {
    fn drop(&mut self) {
        drop(self.lease.take());
        if let Some(record) = self.pending.take() {
            if self.service.ensure_open().is_ok() && tokio::runtime::Handle::try_current().is_ok() {
                let _ = self.service.spawn_source_configuration(
                    self.actor.clone(),
                    record,
                    &self.scope,
                );
            }
        }
    }
}
