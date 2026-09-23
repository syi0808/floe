use std::num::NonZeroU64;

use floe_access::{
    ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority, GrantConsumer,
    GrantDataCategory, GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding,
    ProcessingRestriction, ResourceHandle, SourceAuthority,
};
use floe_access::{DataAccessGrant, GrantState};
use floe_agent_contract::CalendarProvider;
use floe_experts::AgentRegistry;
use floe_experts::{CalendarAccessChange, CalendarAccessConfiguration, RegistrySnapshot};
use floe_experts::{CalendarExpertSetup, CalendarExpertSetupReceipt};
use serde::{Deserialize, Serialize};
use turso::{Row, transaction::TransactionBehavior};
use uuid::Uuid;

use super::*;

const CALENDAR_GRANT_SCHEMA_VERSION: i64 = 3;
const MAX_CALENDAR_GRANT_MAPPINGS: usize = 128;
const MAX_MAPPING_PAYLOAD_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarGrantAdmission {
    pub grant_id: GrantId,
    pub authority: GrantAuthority,
    pub source: GrantSourceBinding,
    pub scope: GrantScope,
    pub grant_scope: GrantScope,
    pub consumer_policy: ConsumerPolicyAuthority,
    pub reviewed_native_subject_fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct CalendarGrantMapping {
    setup_id: Uuid,
    view_handle: Uuid,
    grant_id: GrantId,
    source: GrantSourceBinding,
    expert_assignment_id: Uuid,
    tool_assignment_id: Uuid,
    expert_installation_id: Uuid,
    tool_installation_id: Uuid,
    consumer_policy: ConsumerPolicyAuthority,
    reviewed_native_subject_fingerprint: String,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    #[allow(clippy::too_many_arguments)]
    pub async fn authorize_current_native_calendar_grant(
        &self,
        connection_id: &str,
        provider: CalendarProvider,
        device_id: &str,
        calendar_ids: &[String],
        source_authority: SourceAuthority,
        operation: GrantOperation,
        purpose: GrantPurpose,
        consumer: GrantConsumer,
        processing: ProcessingRestriction,
        native_subject_fingerprint: Option<&str>,
    ) -> Result<CalendarGrantAdmission, AgentFailure> {
        if !matches!(
            provider,
            CalendarProvider::EventKit | CalendarProvider::Android
        ) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT setup_id, view_handle, grant_id, person_id, connection_id, connector, execution_owner, source_incarnation, source_epoch, expert_assignment_id, tool_assignment_id, expert_installation_id, tool_installation_id, policy_incarnation, policy_epoch, reviewed_native_subject_fingerprint, payload FROM calendar_grant_mappings WHERE connection_id = ? AND person_id = ?",
                (connection_id, self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        let mapping = decode_mapping(&row)?;
        if rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::AccessReviewRequired);
        }
        drop(rows);
        drop(connection);
        self.authorize_calendar_grant(
            mapping.setup_id,
            mapping.view_handle,
            connection_id,
            provider,
            device_id,
            calendar_ids,
            source_authority,
            operation,
            purpose,
            consumer,
            processing,
            native_subject_fingerprint,
        )
        .await
    }

    pub(super) async fn initialize_calendar_grant_store(&self) -> Result<(), AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('calendar_grant_schema', 'calendar_grant_mappings')",
                (),
            )
            .await
            .map_err(storage)?;
        let mut names = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            names.push(row.get::<String>(0).map_err(storage)?);
        }
        if names.is_empty() {
            let mut connection = self.connection()?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(storage)?;
            let result = async {
                transaction
                    .execute(
                        "CREATE TABLE calendar_grant_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 3))",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "INSERT INTO calendar_grant_schema (id, version) VALUES (1, 3)",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "CREATE TABLE calendar_grant_mappings (setup_id TEXT PRIMARY KEY, view_handle TEXT NOT NULL UNIQUE, grant_id TEXT NOT NULL UNIQUE, person_id TEXT NOT NULL, connection_id TEXT NOT NULL, connector TEXT NOT NULL, execution_owner TEXT NOT NULL, source_incarnation TEXT NOT NULL, source_epoch INTEGER NOT NULL, expert_assignment_id TEXT NOT NULL, tool_assignment_id TEXT NOT NULL, expert_installation_id TEXT NOT NULL, tool_installation_id TEXT NOT NULL, policy_incarnation TEXT NOT NULL, policy_epoch INTEGER NOT NULL, reviewed_native_subject_fingerprint TEXT NOT NULL, payload TEXT NOT NULL)",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "CREATE INDEX calendar_grant_mappings_person ON calendar_grant_mappings (person_id, setup_id)",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                Ok(())
            }
            .await;
            self.finish_access_grant_transaction(transaction, result)
                .await?;
        } else if names.len() != 2
            || !names.iter().any(|name| name == "calendar_grant_schema")
            || !names.iter().any(|name| name == "calendar_grant_mappings")
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let connection = self.connection()?;
        let mut marker = connection
            .query("SELECT version FROM calendar_grant_schema WHERE id = 1", ())
            .await
            .map_err(storage)?;
        let marker = marker
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        if marker.get::<i64>(0).map_err(storage)? != CALENDAR_GRANT_SCHEMA_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        let mut marker_rows = connection
            .query("SELECT id FROM calendar_grant_schema", ())
            .await
            .map_err(storage)?;
        if marker_rows.next().await.map_err(storage)?.is_none()
            || marker_rows.next().await.map_err(storage)?.is_some()
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut mappings = connection
            .query("SELECT setup_id, view_handle, grant_id, person_id, connection_id, connector, execution_owner, source_incarnation, source_epoch, expert_assignment_id, tool_assignment_id, expert_installation_id, tool_installation_id, policy_incarnation, policy_epoch, reviewed_native_subject_fingerprint, payload FROM calendar_grant_mappings", ())
            .await
            .map_err(storage)?;
        let mut count = 0;
        let mut decoded_mappings = Vec::new();
        while let Some(row) = mappings.next().await.map_err(storage)? {
            count += 1;
            if count > MAX_CALENDAR_GRANT_MAPPINGS {
                return Err(AgentFailure::BudgetExceeded);
            }
            let mapping = decode_mapping(&row)?;
            if mapping.source.person_id() != self.person_id {
                return Err(AgentFailure::VaultUnavailable);
            }
            let grant = self
                .get_data_access_grant(mapping.grant_id)
                .await
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            if grant.authority_owner() != self.vault_id || grant.source() != &mapping.source {
                return Err(AgentFailure::VaultUnavailable);
            }
            decoded_mappings.push(mapping);
        }
        if !decoded_mappings.is_empty() {
            let registry_snapshot = self
                .expert_registry()
                .await?
                .ok_or(AgentFailure::VaultUnavailable)?;
            let registry = AgentRegistry::restore(registry_snapshot, self.vault_id)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            let snapshot = registry.snapshot();
            for mapping in decoded_mappings {
                let setup = snapshot
                    .calendar_setups
                    .iter()
                    .find(|setup| setup.setup_id == mapping.setup_id)
                    .ok_or(AgentFailure::VaultUnavailable)?;
                let view = snapshot
                    .calendar_views
                    .iter()
                    .find(|view| view.handle == mapping.view_handle)
                    .ok_or(AgentFailure::VaultUnavailable)?;
                if setup.view_handle != mapping.view_handle
                    || setup.expert_assignment_id != mapping.expert_assignment_id
                    || setup.tool_assignment_id != mapping.tool_assignment_id
                    || setup.expert_installation_id != mapping.expert_installation_id
                    || setup.tool_installation_id != mapping.tool_installation_id
                    || view.person_id != self.person_id
                {
                    return Err(AgentFailure::VaultUnavailable);
                }
            }
        }
        Ok(())
    }

    pub async fn authorize_calendar_grant(
        &self,
        setup_id: Uuid,
        view_handle: Uuid,
        connection_id: &str,
        provider: CalendarProvider,
        device_id: &str,
        calendar_ids: &[String],
        source_authority: SourceAuthority,
        operation: GrantOperation,
        purpose: GrantPurpose,
        consumer: GrantConsumer,
        processing: ProcessingRestriction,
        native_subject_fingerprint: Option<&str>,
    ) -> Result<CalendarGrantAdmission, AgentFailure> {
        if setup_id.is_nil()
            || view_handle.is_nil()
            || device_id.trim() != device_id
            || device_id.is_empty()
            || !source_authority.is_valid()
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT setup_id, view_handle, grant_id, person_id, connection_id, connector, execution_owner, source_incarnation, source_epoch, expert_assignment_id, tool_assignment_id, expert_installation_id, tool_installation_id, policy_incarnation, policy_epoch, reviewed_native_subject_fingerprint, payload FROM calendar_grant_mappings WHERE setup_id = ? AND view_handle = ? AND person_id = ?",
                (setup_id.to_string(), view_handle.to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        let mapping = decode_mapping(&row)?;
        let native_subject_fingerprint = native_subject_fingerprint
            .ok_or(AgentFailure::AccessReviewRequired)
            .and_then(validate_native_subject_fingerprint)?;
        if mapping.reviewed_native_subject_fingerprint != native_subject_fingerprint {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let grant = self.get_data_access_grant(mapping.grant_id).await?;
        if grant.state() != GrantState::Active {
            return Err(if grant.state() == GrantState::Revoked {
                AgentFailure::PolicyDenied
            } else {
                AgentFailure::AccessReviewRequired
            });
        }
        let expected_source = calendar_source(
            self.person_id,
            connection_id,
            provider,
            device_id,
            source_authority,
        )?;
        if mapping.setup_id != setup_id
            || mapping.view_handle != view_handle
            || grant.source() != &mapping.source
            || grant.source() != &expected_source
            || grant.authority_owner() != self.vault_id
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let registry_snapshot = self
            .expert_registry()
            .await?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        let registry = AgentRegistry::restore(registry_snapshot, self.vault_id)?;
        let registry_snapshot = registry.snapshot();
        let setup = registry_snapshot
            .calendar_setups
            .iter()
            .find(|setup| setup.setup_id == mapping.setup_id)
            .ok_or(AgentFailure::AccessReviewRequired)?;
        let view = registry
            .calendar_view(self.person_id, mapping.view_handle)
            .map_err(|_| AgentFailure::AccessReviewRequired)?;
        let assignments = &registry_snapshot.assignments;
        let installations = &registry_snapshot.installations;
        let expert_assignment = assignments
            .iter()
            .find(|assignment| assignment.id == mapping.expert_assignment_id);
        let tool_assignment = assignments
            .iter()
            .find(|assignment| assignment.id == mapping.tool_assignment_id);
        let expert_installation = installations
            .iter()
            .find(|installation| installation.id == mapping.expert_installation_id);
        let tool_installation = installations
            .iter()
            .find(|installation| installation.id == mapping.tool_installation_id);
        if setup.view_handle != mapping.view_handle
            || setup.expert_assignment_id != mapping.expert_assignment_id
            || setup.tool_assignment_id != mapping.tool_assignment_id
            || setup.expert_installation_id != mapping.expert_installation_id
            || setup.tool_installation_id != mapping.tool_installation_id
            || view.provider != provider
            || view.device_id != device_id
            || calendar_ids
                .iter()
                .any(|calendar_id| !view.calendar_ids.contains(calendar_id))
            || expert_assignment.is_none_or(|assignment| {
                assignment.person_id != self.person_id || !assignment.enabled
            })
            || tool_assignment.is_none_or(|assignment| {
                assignment.person_id != self.person_id || !assignment.enabled
            })
            || expert_installation.is_none_or(|installation| !installation.enabled)
            || tool_installation.is_none_or(|installation| !installation.enabled)
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let requested_scope =
            calendar_scope(calendar_ids.to_vec(), consumer.clone(), purpose, processing)?;
        if !grant.scope().consumers().contains(&consumer) {
            return Err(AgentFailure::AccessReviewRequired);
        }
        if requested_scope
            .resources()
            .iter()
            .any(|resource| !grant.scope().resources().contains(resource))
            || requested_scope.categories() != grant.scope().categories()
            || requested_scope.operations() != grant.scope().operations()
            || requested_scope.purposes() != grant.scope().purposes()
            || requested_scope.processing() != grant.scope().processing()
            || !grant.scope().operations().contains(&operation)
            || !grant.scope().purposes().contains(&purpose)
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        Ok(CalendarGrantAdmission {
            grant_id: grant.id(),
            authority: grant.authority(),
            source: grant.source().clone(),
            scope: requested_scope,
            grant_scope: grant.scope().clone(),
            consumer_policy: mapping.consumer_policy,
            reviewed_native_subject_fingerprint: mapping.reviewed_native_subject_fingerprint,
        })
    }

    pub async fn calendar_grant_connection_id(
        &self,
        setup_id: Uuid,
    ) -> Result<String, AgentFailure> {
        if setup_id.is_nil() {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT setup_id, view_handle, grant_id, person_id, connection_id, connector, execution_owner, source_incarnation, source_epoch, expert_assignment_id, tool_assignment_id, expert_installation_id, tool_installation_id, policy_incarnation, policy_epoch, reviewed_native_subject_fingerprint, payload FROM calendar_grant_mappings WHERE setup_id = ? AND person_id = ?",
                (setup_id.to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        let mapping = decode_mapping(&row)?;
        if mapping.setup_id != setup_id || mapping.source.person_id() != self.person_id {
            return Err(AgentFailure::VaultUnavailable);
        }
        let grant = self
            .get_data_access_grant(mapping.grant_id)
            .await
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        if grant.authority_owner() != self.vault_id || grant.source() != &mapping.source {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(mapping.source.connection_id().as_str().to_owned())
    }

    pub(super) async fn persist_calendar_install(
        &self,
        previous: Option<&RegistrySnapshot>,
        snapshot: &RegistrySnapshot,
        request: &CalendarExpertSetup,
        setup: &CalendarExpertSetupReceipt,
        connection_id: &str,
        consumers: &[GrantConsumer],
        check: impl Fn() -> Result<(), AgentFailure> + Sync,
    ) -> Result<(), AgentFailure> {
        if !is_native_provider(request.provider) {
            return match previous {
                Some(previous) => {
                    self.save_expert_registry_checked(previous.revision, snapshot, check)
                        .await
                }
                None => {
                    self.initialize_expert_registry_checked(snapshot, check)
                        .await
                }
            };
        }
        let source_authority = request
            .source_authority
            .ok_or(AgentFailure::AccessReviewRequired)?;
        let (source, scope) = calendar_binding(
            self.person_id,
            request,
            connection_id,
            source_authority,
            consumers,
        )?;
        let reviewed_native_subject_fingerprint = request
            .reviewed_native_subject_fingerprint
            .as_deref()
            .ok_or(AgentFailure::AccessReviewRequired)
            .and_then(validate_native_subject_fingerprint)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let previous_in_transaction = self.registry_on(&transaction).await?;
            if previous_in_transaction
                .as_ref()
                .map(|snapshot| snapshot.revision)
                != previous.map(|snapshot| snapshot.revision)
            {
                return Err(AgentFailure::Conflict);
            }
            self.write_registry_snapshot_in_transaction(
                &transaction,
                previous.map(|snapshot| snapshot.revision),
                snapshot,
            )
            .await?;
            let grant = self
                .create_data_access_grant_in_transaction(
                    &transaction,
                    GrantId::new(),
                    source.clone(),
                    scope.clone(),
                )
                .await?;
            let reviewed = self
                .mutate_data_access_grant_in_transaction(
                    &transaction,
                    grant.id(),
                    grant.authority(),
                    super::access_grants::AccessGrantMutation::Review { source, scope },
                )
                .await?;
            self.insert_calendar_mapping(
                &transaction,
                setup,
                &reviewed,
                reviewed_native_subject_fingerprint,
            )
            .await?;
            self.check_access()?;
            check()?;
            Ok(())
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await?;
        self.check_access()?;
        Ok(())
    }

    pub(super) async fn persist_calendar_change(
        &self,
        configuration: &CalendarAccessConfiguration,
        previous: &RegistrySnapshot,
        snapshot: &RegistrySnapshot,
        connection_id: &str,
        consumers: &[GrantConsumer],
        check: impl Fn() -> Result<(), AgentFailure> + Sync,
    ) -> Result<(), AgentFailure> {
        let setup = previous
            .calendar_setups
            .iter()
            .find(|setup| setup.setup_id == configuration.setup_id)
            .ok_or(AgentFailure::NotFound)?;
        let binding = previous
            .calendar_views
            .iter()
            .find(|binding| binding.handle == setup.view_handle)
            .ok_or(AgentFailure::NotFound)?;
        if !is_native_provider(binding.provider) {
            return self
                .save_expert_registry_checked(previous.revision, snapshot, check)
                .await;
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let stored_previous = self
                .registry_on(&transaction)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if stored_previous.revision != previous.revision {
                return Err(AgentFailure::Conflict);
            }
            let mapping = self
                .mapping_for_setup(&transaction, configuration.setup_id)
                .await?;
            let current = self
                .read_data_access_grant_in_transaction(&transaction, mapping.grant_id)
                .await?;
            let (source, scope, reviewed_native_subject_fingerprint, mutation) =
                match &configuration.change {
                    CalendarAccessChange::SetEnabled { enabled: true } => {
                        let mutation = super::access_grants::AccessGrantMutation::Activate {
                            source: current.source().clone(),
                            scope: current.scope().clone(),
                        };
                        (
                            current.source().clone(),
                            current.scope().clone(),
                            mapping.reviewed_native_subject_fingerprint.clone(),
                            mutation,
                        )
                    }
                    CalendarAccessChange::SetEnabled { enabled: false } => (
                        current.source().clone(),
                        current.scope().clone(),
                        mapping.reviewed_native_subject_fingerprint.clone(),
                        super::access_grants::AccessGrantMutation::Pause,
                    ),
                    CalendarAccessChange::SetScope {
                        provider,
                        device_id,
                        calendar_ids,
                        source_authority,
                        reviewed_native_subject_fingerprint,
                        ..
                    } => {
                        let authority =
                            source_authority.ok_or(AgentFailure::AccessReviewRequired)?;
                        let reviewed_native_subject_fingerprint =
                            reviewed_native_subject_fingerprint
                                .as_deref()
                                .ok_or(AgentFailure::AccessReviewRequired)
                                .and_then(validate_native_subject_fingerprint)?;
                        let request = CalendarExpertSetup {
                            instance_id: self.vault_id,
                            expected_revision: previous.revision,
                            setup_id: configuration.setup_id,
                            provider: *provider,
                            device_id: device_id.clone(),
                            calendar_ids: calendar_ids.clone(),
                            connection_scope: binding.connection_scope,
                            connection_revision: 1,
                            source_authority: Some(authority),
                            reviewed_native_subject_fingerprint: Some(
                                reviewed_native_subject_fingerprint.to_owned(),
                            ),
                        };
                        let (source, scope) = calendar_binding(
                            self.person_id,
                            &request,
                            connection_id,
                            authority,
                            consumers,
                        )?;
                        let mutation = if current.state() == GrantState::Active
                            && mapping.reviewed_native_subject_fingerprint
                                != reviewed_native_subject_fingerprint
                        {
                            super::access_grants::AccessGrantMutation::ReviewActive {
                                source: source.clone(),
                                scope: scope.clone(),
                            }
                        } else if current.state() == GrantState::Active {
                            super::access_grants::AccessGrantMutation::Activate {
                                source: source.clone(),
                                scope: scope.clone(),
                            }
                        } else {
                            super::access_grants::AccessGrantMutation::Review {
                                source: source.clone(),
                                scope: scope.clone(),
                            }
                        };
                        (
                            source,
                            scope,
                            reviewed_native_subject_fingerprint.to_owned(),
                            mutation,
                        )
                    }
                    CalendarAccessChange::Remove {} => (
                        current.source().clone(),
                        current.scope().clone(),
                        mapping.reviewed_native_subject_fingerprint.clone(),
                        super::access_grants::AccessGrantMutation::Revoke,
                    ),
                };
            let updated = self
                .mutate_data_access_grant_in_transaction(
                    &transaction,
                    current.id(),
                    current.authority(),
                    mutation,
                )
                .await?;
            self.write_registry_snapshot_in_transaction(
                &transaction,
                Some(previous.revision),
                snapshot,
            )
            .await?;
            let consumer_policy = if updated != current {
                mapping
                    .consumer_policy
                    .advance()
                    .ok_or(AgentFailure::BudgetExceeded)?
            } else {
                mapping.consumer_policy
            };
            self.update_calendar_mapping(
                &transaction,
                &mapping,
                &updated,
                source,
                scope,
                reviewed_native_subject_fingerprint,
                consumer_policy,
            )
            .await?;
            self.check_access()?;
            check()?;
            Ok(())
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    async fn insert_calendar_mapping(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        setup: &CalendarExpertSetupReceipt,
        grant: &DataAccessGrant,
        reviewed_native_subject_fingerprint: String,
    ) -> Result<(), AgentFailure> {
        let mapping = CalendarGrantMapping {
            setup_id: setup.setup_id,
            view_handle: setup.view_handle,
            grant_id: grant.id(),
            source: grant.source().clone(),
            expert_assignment_id: setup.expert_assignment_id,
            tool_assignment_id: setup.tool_assignment_id,
            expert_installation_id: setup.expert_installation_id,
            tool_installation_id: setup.tool_installation_id,
            consumer_policy: ConsumerPolicyAuthority::new(),
            reviewed_native_subject_fingerprint,
        };
        let payload =
            serde_json::to_string(&mapping).map_err(|_| AgentFailure::StorageUnavailable)?;
        if payload.len() > MAX_MAPPING_PAYLOAD_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        transaction
            .execute(
                "INSERT INTO calendar_grant_mappings (setup_id, view_handle, grant_id, person_id, connection_id, connector, execution_owner, source_incarnation, source_epoch, expert_assignment_id, tool_assignment_id, expert_installation_id, tool_installation_id, policy_incarnation, policy_epoch, reviewed_native_subject_fingerprint, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                turso::params![
                    mapping.setup_id.to_string(),
                    mapping.view_handle.to_string(),
                    mapping.grant_id.as_uuid().to_string(),
                    self.person_id.to_string(),
                    mapping.source.connection_id().as_str().to_owned(),
                    mapping.source.connector().as_str().to_string(),
                    mapping.source.execution_owner().as_str().to_string(),
                    mapping.source.source_authority().incarnation().to_string(),
                    i64::try_from(mapping.source.source_authority().epoch().get())
                        .map_err(|_| AgentFailure::Conflict)?,
                    mapping.expert_assignment_id.to_string(),
                    mapping.tool_assignment_id.to_string(),
                    mapping.expert_installation_id.to_string(),
                    mapping.tool_installation_id.to_string(),
                    mapping.consumer_policy.incarnation().to_string(),
                    i64::try_from(mapping.consumer_policy.epoch().get())
                        .map_err(|_| AgentFailure::Conflict)?,
                    mapping.reviewed_native_subject_fingerprint.clone(),
                    payload,
                ],
            )
            .await
            .map_err(storage)?;
        Ok(())
    }

    async fn update_calendar_mapping(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        previous: &CalendarGrantMapping,
        grant: &DataAccessGrant,
        source: GrantSourceBinding,
        _scope: GrantScope,
        reviewed_native_subject_fingerprint: String,
        consumer_policy: ConsumerPolicyAuthority,
    ) -> Result<(), AgentFailure> {
        let mapping = CalendarGrantMapping {
            setup_id: previous.setup_id,
            view_handle: previous.view_handle,
            grant_id: grant.id(),
            source,
            expert_assignment_id: previous.expert_assignment_id,
            tool_assignment_id: previous.tool_assignment_id,
            expert_installation_id: previous.expert_installation_id,
            tool_installation_id: previous.tool_installation_id,
            consumer_policy,
            reviewed_native_subject_fingerprint,
        };
        let payload =
            serde_json::to_string(&mapping).map_err(|_| AgentFailure::StorageUnavailable)?;
        let changed = transaction
            .execute(
                "UPDATE calendar_grant_mappings SET grant_id = ?, person_id = ?, connection_id = ?, connector = ?, execution_owner = ?, source_incarnation = ?, source_epoch = ?, expert_assignment_id = ?, tool_assignment_id = ?, expert_installation_id = ?, tool_installation_id = ?, policy_incarnation = ?, policy_epoch = ?, reviewed_native_subject_fingerprint = ?, payload = ? WHERE setup_id = ? AND person_id = ? AND grant_id = ?",
                turso::params![
                    mapping.grant_id.as_uuid().to_string(),
                    self.person_id.to_string(),
                    mapping.source.connection_id().as_str().to_owned(),
                    mapping.source.connector().as_str().to_string(),
                    mapping.source.execution_owner().as_str().to_string(),
                    mapping.source.source_authority().incarnation().to_string(),
                    i64::try_from(mapping.source.source_authority().epoch().get()).map_err(|_| AgentFailure::Conflict)?,
                    mapping.expert_assignment_id.to_string(),
                    mapping.tool_assignment_id.to_string(),
                    mapping.expert_installation_id.to_string(),
                    mapping.tool_installation_id.to_string(),
                    mapping.consumer_policy.incarnation().to_string(),
                    i64::try_from(mapping.consumer_policy.epoch().get()).map_err(|_| AgentFailure::Conflict)?,
                    mapping.reviewed_native_subject_fingerprint.clone(),
                    payload,
                    mapping.setup_id.to_string(),
                    self.person_id.to_string(),
                    previous.grant_id.as_uuid().to_string(),
                ],
            )
            .await
            .map_err(storage)?;
        if changed != 1 {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }

    async fn mapping_for_setup(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        setup_id: Uuid,
    ) -> Result<CalendarGrantMapping, AgentFailure> {
        let mut rows = transaction
            .query("SELECT setup_id, view_handle, grant_id, person_id, connection_id, connector, execution_owner, source_incarnation, source_epoch, expert_assignment_id, tool_assignment_id, expert_installation_id, tool_installation_id, policy_incarnation, policy_epoch, reviewed_native_subject_fingerprint, payload FROM calendar_grant_mappings WHERE setup_id = ? AND person_id = ?", (setup_id.to_string(), self.person_id.to_string()))
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        decode_mapping(&row)
    }

    pub(super) async fn bump_calendar_policies_for_registry_change(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        previous: &RegistrySnapshot,
        next: &RegistrySnapshot,
    ) -> Result<(), AgentFailure> {
        let mut rows = transaction
            .query("SELECT setup_id, view_handle, grant_id, person_id, connection_id, connector, execution_owner, source_incarnation, source_epoch, expert_assignment_id, tool_assignment_id, expert_installation_id, tool_installation_id, policy_incarnation, policy_epoch, reviewed_native_subject_fingerprint, payload FROM calendar_grant_mappings WHERE person_id = ?", [self.person_id.to_string()])
            .await
            .map_err(storage)?;
        while let Some(row) = rows.next().await.map_err(storage)? {
            let mapping = decode_mapping(&row)?;
            if registry_policy_identity_changed(previous, next, &mapping) {
                let policy = mapping
                    .consumer_policy
                    .advance()
                    .ok_or(AgentFailure::BudgetExceeded)?;
                let mut updated = mapping.clone();
                updated.consumer_policy = policy;
                let payload = serde_json::to_string(&updated)
                    .map_err(|_| AgentFailure::StorageUnavailable)?;
                if payload.len() > MAX_MAPPING_PAYLOAD_BYTES {
                    return Err(AgentFailure::BudgetExceeded);
                }
                let changed = transaction
                    .execute(
                        "UPDATE calendar_grant_mappings SET policy_incarnation = ?, policy_epoch = ?, payload = ? WHERE setup_id = ? AND person_id = ?",
                        (
                            policy.incarnation().to_string(),
                            i64::try_from(policy.epoch().get())
                                .map_err(|_| AgentFailure::Conflict)?,
                            payload,
                            mapping.setup_id.to_string(),
                            self.person_id.to_string(),
                        ),
                    )
                    .await
                    .map_err(storage)?;
                if changed != 1 {
                    return Err(AgentFailure::Conflict);
                }
            }
        }
        Ok(())
    }
}

fn registry_policy_identity_changed(
    previous: &RegistrySnapshot,
    next: &RegistrySnapshot,
    mapping: &CalendarGrantMapping,
) -> bool {
    let previous_setup = previous
        .calendar_setups
        .iter()
        .find(|setup| setup.setup_id == mapping.setup_id);
    let next_setup = next
        .calendar_setups
        .iter()
        .find(|setup| setup.setup_id == mapping.setup_id);
    let Some(previous_setup) = previous_setup else {
        return true;
    };
    let Some(next_setup) = next_setup else {
        return true;
    };
    if previous_setup.person_id != next_setup.person_id
        || previous_setup.view_handle != next_setup.view_handle
        || previous_setup.tool_installation_id != next_setup.tool_installation_id
        || previous_setup.expert_installation_id != next_setup.expert_installation_id
        || previous_setup.tool_assignment_id != next_setup.tool_assignment_id
        || previous_setup.expert_assignment_id != next_setup.expert_assignment_id
        || previous_setup.connection_scope != next_setup.connection_scope
        || previous_setup.source_authority != next_setup.source_authority
    {
        return true;
    }
    let previous_view = previous
        .calendar_views
        .iter()
        .find(|view| view.handle == mapping.view_handle);
    let next_view = next
        .calendar_views
        .iter()
        .find(|view| view.handle == mapping.view_handle);
    if previous_view.map(|view| view.provider) != next_view.map(|view| view.provider)
        || previous_view.map(|view| &view.device_id) != next_view.map(|view| &view.device_id)
        || previous_view.map(|view| &view.calendar_ids) != next_view.map(|view| &view.calendar_ids)
        || previous_view.map(|view| view.connection_scope)
            != next_view.map(|view| view.connection_scope)
        || previous_view.map(|view| view.source_authority)
            != next_view.map(|view| view.source_authority)
        || previous_view.map(|view| view.enabled) != next_view.map(|view| view.enabled)
    {
        return true;
    }
    for assignment_id in [mapping.expert_assignment_id, mapping.tool_assignment_id] {
        let previous_assignment = previous
            .assignments
            .iter()
            .find(|assignment| assignment.id == assignment_id);
        let next_assignment = next
            .assignments
            .iter()
            .find(|assignment| assignment.id == assignment_id);
        if previous_assignment.map(|assignment| assignment.person_id)
            != next_assignment.map(|assignment| assignment.person_id)
            || previous_assignment.map(|assignment| assignment.installation_id)
                != next_assignment.map(|assignment| assignment.installation_id)
            || previous_assignment.map(|assignment| assignment.enabled)
                != next_assignment.map(|assignment| assignment.enabled)
            || previous_assignment.map(|assignment| &assignment.granted_tool_assignments)
                != next_assignment.map(|assignment| &assignment.granted_tool_assignments)
            || previous_assignment.map(|assignment| &assignment.granted_view_handles)
                != next_assignment.map(|assignment| &assignment.granted_view_handles)
        {
            return true;
        }
    }
    for installation_id in [mapping.expert_installation_id, mapping.tool_installation_id] {
        let previous_installation = previous
            .installations
            .iter()
            .find(|installation| installation.id == installation_id);
        let next_installation = next
            .installations
            .iter()
            .find(|installation| installation.id == installation_id);
        if previous_installation.map(|installation| installation.enabled)
            != next_installation.map(|installation| installation.enabled)
            || previous_installation.map(|installation| &installation.package)
                != next_installation.map(|installation| &installation.package)
        {
            return true;
        }
    }
    false
}

fn is_native_provider(provider: CalendarProvider) -> bool {
    matches!(
        provider,
        CalendarProvider::EventKit | CalendarProvider::Android
    )
}

fn validate_native_subject_fingerprint(value: &str) -> Result<String, AgentFailure> {
    if value.len() != 64
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(value.to_owned())
}

fn connector(provider: CalendarProvider) -> Result<ConnectorId, AgentFailure> {
    ConnectorId::try_new(
        floe_access::native_calendar_connector(provider)
            .ok_or(AgentFailure::CapabilityUnavailable)?,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

fn calendar_source(
    person_id: floe_kernel::PersonId,
    connection_id: &str,
    provider: CalendarProvider,
    device_id: &str,
    source_authority: SourceAuthority,
) -> Result<GrantSourceBinding, AgentFailure> {
    let connection_id = floe_access::ConnectionId::try_new(connection_id.to_owned())
        .map_err(|_| AgentFailure::InvalidInput)?;
    let execution_owner =
        ExecutionOwnerId::try_new(device_id.to_owned()).map_err(|_| AgentFailure::InvalidInput)?;
    GrantSourceBinding::try_new(
        person_id,
        connection_id,
        connector(provider)?,
        execution_owner,
        source_authority,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

fn calendar_binding(
    person_id: floe_kernel::PersonId,
    request: &CalendarExpertSetup,
    connection_id: &str,
    source_authority: SourceAuthority,
    consumers: &[GrantConsumer],
) -> Result<(GrantSourceBinding, GrantScope), AgentFailure> {
    let source = calendar_source(
        person_id,
        connection_id,
        request.provider,
        &request.device_id,
        source_authority,
    )?;
    let resources = request
        .calendar_ids
        .iter()
        .cloned()
        .map(ResourceHandle::try_new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let scope = GrantScope::try_new(
        resources,
        vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
        vec![GrantOperation::Read],
        vec![GrantPurpose::Assistant],
        consumers.to_vec(),
        ProcessingRestriction::LocalOnly,
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    Ok((source, scope))
}

fn calendar_scope(
    calendar_ids: Vec<String>,
    consumer: GrantConsumer,
    purpose: GrantPurpose,
    processing: ProcessingRestriction,
) -> Result<GrantScope, AgentFailure> {
    let resources = calendar_ids
        .into_iter()
        .map(ResourceHandle::try_new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AgentFailure::InvalidInput)?;
    GrantScope::try_new(
        resources,
        vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
        vec![GrantOperation::Read],
        vec![purpose],
        vec![consumer],
        processing,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

fn decode_mapping(row: &Row) -> Result<CalendarGrantMapping, AgentFailure> {
    let reviewed_native_subject_fingerprint = row.get::<String>(15).map_err(storage)?;
    validate_native_subject_fingerprint(&reviewed_native_subject_fingerprint)?;
    let payload = row.get::<String>(16).map_err(storage)?;
    if payload.len() > MAX_MAPPING_PAYLOAD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    let mapping: CalendarGrantMapping =
        serde_json::from_str(&payload).map_err(|_| AgentFailure::VaultUnavailable)?;
    let setup_id = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let view_handle = Uuid::parse_str(&row.get::<String>(1).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let grant_id = GrantId::from_uuid(
        Uuid::parse_str(&row.get::<String>(2).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
    )
    .ok_or(AgentFailure::VaultUnavailable)?;
    let person_id = floe_kernel::PersonId(
        Uuid::parse_str(&row.get::<String>(3).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
    );
    let connection_id = floe_access::ConnectionId::try_new(row.get::<String>(4).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let connector = ConnectorId::try_new(row.get::<String>(5).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let execution_owner = ExecutionOwnerId::try_new(row.get::<String>(6).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let source_incarnation = Uuid::parse_str(&row.get::<String>(7).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let source_epoch = NonZeroU64::new(
        u64::try_from(row.get::<i64>(8).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
    )
    .ok_or(AgentFailure::VaultUnavailable)?;
    let expert_assignment_id = Uuid::parse_str(&row.get::<String>(9).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let tool_assignment_id = Uuid::parse_str(&row.get::<String>(10).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let expert_installation_id = Uuid::parse_str(&row.get::<String>(11).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let tool_installation_id = Uuid::parse_str(&row.get::<String>(12).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let policy_incarnation = Uuid::parse_str(&row.get::<String>(13).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let policy_epoch = NonZeroU64::new(
        u64::try_from(row.get::<i64>(14).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
    )
    .ok_or(AgentFailure::VaultUnavailable)?;
    let indexed_source = GrantSourceBinding::try_new(
        person_id,
        connection_id,
        connector,
        execution_owner,
        SourceAuthority::from_parts(source_incarnation, source_epoch)
            .ok_or(AgentFailure::VaultUnavailable)?,
    )
    .map_err(|_| AgentFailure::VaultUnavailable)?;
    if mapping.setup_id != setup_id
        || mapping.view_handle != view_handle
        || mapping.grant_id != grant_id
        || mapping.source != indexed_source
        || mapping.expert_assignment_id != expert_assignment_id
        || mapping.tool_assignment_id != tool_assignment_id
        || mapping.expert_installation_id != expert_installation_id
        || mapping.tool_installation_id != tool_installation_id
        || mapping.consumer_policy
            != ConsumerPolicyAuthority::from_parts(policy_incarnation, policy_epoch)
                .ok_or(AgentFailure::VaultUnavailable)?
        || mapping.reviewed_native_subject_fingerprint != reviewed_native_subject_fingerprint
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(mapping)
}

#[cfg(test)]
mod tests {
    /// How an Expert is packaged when its calendar setup is installed.
    ///
    /// The builtin catalogue is the App's; what the registry takes is this.
    fn test_packaging() -> floe_experts::ExpertPackaging {
        floe_experts::ExpertPackaging {
            expert: floe_experts::AgentId::try_new("floe.builtin.schedule").unwrap(),
            tool_id: "floe.builtin.schedule.timeline".into(),
            version: "1.0.0".into(),
            publisher: "floe".into(),
            metadata: floe_experts::ExpertMetadata {
                name: "Schedule Expert".into(),
                description: "Reads a bounded calendar timeline.".into(),
                domain_tags: vec!["schedule".into()],
                skills: vec!["propose a focus window".into()],
                supported_placements: vec![
                    floe_agent_contract::ModelPlacement::DeviceLocal,
                    floe_agent_contract::ModelPlacement::Remote,
                ],
            },
            state_schema_version: 1,
        }
    }

    use std::{collections::HashMap, os::unix::fs::PermissionsExt, sync::Mutex};

    use floe_access::ConnectionId;

    use super::*;

    #[derive(Clone, Default)]
    struct TestKeys(std::sync::Arc<Mutex<HashMap<(floe_kernel::PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for TestKeys {
        fn load(
            &self,
            person_id: floe_kernel::PersonId,
            vault_id: Uuid,
        ) -> Result<VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .get(&(person_id, vault_id))
                .copied()
                .map(VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: floe_kernel::PersonId,
            vault_id: Uuid,
            key: &VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    fn setup_request(vault: &EncryptedAgentVault<TestKeys>) -> CalendarExpertSetup {
        CalendarExpertSetup {
            instance_id: vault.registry_instance_id(),
            expected_revision: 0,
            setup_id: Uuid::new_v4(),
            provider: CalendarProvider::EventKit,
            device_id: "test-device".into(),
            calendar_ids: vec!["home".into(), "work".into()],
            connection_scope: floe_agent_contract::CalendarScope::Selected,
            connection_revision: 1,
            source_authority: Some(SourceAuthority::new()),
            reviewed_native_subject_fingerprint: Some("a".repeat(64)),
        }
    }

    fn calendar_consumers() -> Vec<GrantConsumer> {
        [
            "floe.builtin.schedule",
            "floe.builtin.commitments",
            "floe.builtin.focus-attention",
            "floe.builtin.wellbeing",
        ]
        .into_iter()
        .map(|consumer| GrantConsumer::builtin(consumer).unwrap())
        .collect()
    }

    #[tokio::test]
    async fn native_calendar_grant_requires_reviewed_activation_and_terminal_removal() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, TestKeys::default())
            .await
            .unwrap();
        let request = setup_request(&vault);
        let source_authority = request.source_authority.unwrap();
        let installed = vault
            .install_calendar_expert_with_connection(
                request.clone(),
                &test_packaging(),
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        let denied = vault
            .authorize_calendar_grant(
                installed.setup.setup_id,
                installed.setup.view_handle,
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &request.calendar_ids,
                source_authority,
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                ProcessingRestriction::LocalOnly,
                Some("a".repeat(64).as_str()),
            )
            .await;
        assert_eq!(denied, Err(AgentFailure::AccessReviewRequired));
        let active = vault
            .configure_calendar_access_with_connection(
                CalendarAccessConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: installed.registry.revision,
                    setup_id: installed.setup.setup_id,
                    change: CalendarAccessChange::SetEnabled { enabled: true },
                },
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        let admission = vault
            .authorize_calendar_grant(
                installed.setup.setup_id,
                installed.setup.view_handle,
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &["home".into()],
                source_authority,
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                ProcessingRestriction::LocalOnly,
                Some("a".repeat(64).as_str()),
            )
            .await
            .unwrap();
        assert_eq!(admission.scope.resources().len(), 1);
        assert_eq!(admission.grant_scope.resources().len(), 2);
        let current = vault
            .authorize_current_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &["home".into()],
                source_authority,
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                ProcessingRestriction::LocalOnly,
                Some("a".repeat(64).as_str()),
            )
            .await
            .unwrap();
        assert_eq!(current, admission);
        for consumer in [
            "floe.builtin.commitments",
            "floe.builtin.focus-attention",
            "floe.builtin.wellbeing",
        ] {
            vault
                .authorize_current_native_calendar_grant(
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &["home".into()],
                    source_authority,
                    GrantOperation::Read,
                    GrantPurpose::Assistant,
                    GrantConsumer::builtin(consumer).unwrap(),
                    ProcessingRestriction::LocalOnly,
                    Some("a".repeat(64).as_str()),
                )
                .await
                .unwrap();
        }
        assert_eq!(
            vault
                .authorize_current_native_calendar_grant(
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &["home".into()],
                    source_authority,
                    GrantOperation::Read,
                    GrantPurpose::Assistant,
                    GrantConsumer::builtin("calendar.expert").unwrap(),
                    ProcessingRestriction::LocalOnly,
                    Some("a".repeat(64).as_str()),
                )
                .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        assert_eq!(
            vault
                .authorize_current_native_calendar_grant(
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &["home".into()],
                    source_authority,
                    GrantOperation::Read,
                    GrantPurpose::Assistant,
                    GrantConsumer::extension("third-party.schedule").unwrap(),
                    ProcessingRestriction::LocalOnly,
                    Some("a".repeat(64).as_str()),
                )
                .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        assert_eq!(
            vault
                .authorize_current_native_calendar_grant(
                    "another-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &["home".into()],
                    source_authority,
                    GrantOperation::Read,
                    GrantPurpose::Assistant,
                    GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                    ProcessingRestriction::LocalOnly,
                    Some("a".repeat(64).as_str()),
                )
                .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        let subject_mismatch = vault
            .authorize_calendar_grant(
                installed.setup.setup_id,
                installed.setup.view_handle,
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &["home".into()],
                source_authority,
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                ProcessingRestriction::LocalOnly,
                Some("b".repeat(64).as_str()),
            )
            .await;
        assert_eq!(subject_mismatch, Err(AgentFailure::AccessReviewRequired));
        let stable_policy = admission.consumer_policy;
        let retry = vault
            .configure_calendar_access_with_connection(
                CalendarAccessConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: active.registry.revision,
                    setup_id: installed.setup.setup_id,
                    change: CalendarAccessChange::SetEnabled { enabled: true },
                },
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        assert!(retry.registry.revision > active.registry.revision);
        let retry_admission = vault
            .authorize_calendar_grant(
                installed.setup.setup_id,
                installed.setup.view_handle,
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &["home".into()],
                source_authority,
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                ProcessingRestriction::LocalOnly,
                Some("a".repeat(64).as_str()),
            )
            .await
            .unwrap();
        assert_eq!(retry_admission.consumer_policy, stable_policy);
        let disabled = vault
            .configure_registry(
                floe_experts::RegistryConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: retry.registry.revision,
                    target: floe_experts::RegistryConfigurationTarget::Assignment {
                        id: installed.setup.expert_assignment_id,
                        enabled: false,
                    },
                },
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        let reenabled = vault
            .configure_registry(
                floe_experts::RegistryConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: disabled.revision,
                    target: floe_experts::RegistryConfigurationTarget::Assignment {
                        id: installed.setup.expert_assignment_id,
                        enabled: true,
                    },
                },
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        let policy_aba = vault
            .authorize_calendar_grant(
                installed.setup.setup_id,
                installed.setup.view_handle,
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &["home".into()],
                source_authority,
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                ProcessingRestriction::LocalOnly,
                Some("a".repeat(64).as_str()),
            )
            .await
            .unwrap();
        assert_ne!(policy_aba.consumer_policy, stable_policy);
        let paused = vault
            .configure_calendar_access_with_connection(
                CalendarAccessConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: reenabled.revision,
                    setup_id: installed.setup.setup_id,
                    change: CalendarAccessChange::SetEnabled { enabled: false },
                },
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            vault
                .authorize_calendar_grant(
                    installed.setup.setup_id,
                    installed.setup.view_handle,
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &request.calendar_ids,
                    source_authority,
                    GrantOperation::Read,
                    GrantPurpose::Assistant,
                    GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                    ProcessingRestriction::LocalOnly,
                    Some("a".repeat(64).as_str()),
                )
                .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        let reactivated = vault
            .configure_calendar_access_with_connection(
                CalendarAccessConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: paused.registry.revision,
                    setup_id: installed.setup.setup_id,
                    change: CalendarAccessChange::SetEnabled { enabled: true },
                },
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        let reactivated_admission = vault
            .authorize_calendar_grant(
                installed.setup.setup_id,
                installed.setup.view_handle,
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &request.calendar_ids,
                source_authority,
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                ProcessingRestriction::LocalOnly,
                Some("a".repeat(64).as_str()),
            )
            .await
            .unwrap();
        assert_ne!(reactivated_admission.consumer_policy, stable_policy);
        vault
            .configure_calendar_access_with_connection(
                CalendarAccessConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: reactivated.registry.revision,
                    setup_id: installed.setup.setup_id,
                    change: CalendarAccessChange::Remove {},
                },
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            vault
                .authorize_calendar_grant(
                    installed.setup.setup_id,
                    installed.setup.view_handle,
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &request.calendar_ids,
                    source_authority,
                    GrantOperation::Read,
                    GrantPurpose::Assistant,
                    GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                    ProcessingRestriction::LocalOnly,
                    Some("a".repeat(64).as_str()),
                )
                .await,
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn native_subject_rereview_advances_active_authority_and_rejects_old_fingerprint() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, TestKeys::default())
            .await
            .unwrap();
        let request = setup_request(&vault);
        let source_authority = request.source_authority.unwrap();
        let installed = vault
            .install_calendar_expert_with_connection(
                request.clone(),
                &test_packaging(),
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        let enabled = vault
            .configure_calendar_access_with_connection(
                CalendarAccessConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: installed.registry.revision,
                    setup_id: installed.setup.setup_id,
                    change: CalendarAccessChange::SetEnabled { enabled: true },
                },
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        let old = vault
            .authorize_calendar_grant(
                installed.setup.setup_id,
                installed.setup.view_handle,
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &request.calendar_ids,
                source_authority,
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                ProcessingRestriction::LocalOnly,
                Some("a".repeat(64).as_str()),
            )
            .await
            .unwrap();
        let reviewed = vault
            .configure_calendar_access_with_connection(
                CalendarAccessConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: enabled.registry.revision,
                    setup_id: installed.setup.setup_id,
                    change: CalendarAccessChange::SetScope {
                        provider: CalendarProvider::EventKit,
                        device_id: "test-device".into(),
                        calendar_ids: request.calendar_ids.clone(),
                        connection_scope: floe_agent_contract::CalendarScope::Selected,
                        connection_revision: 1,
                        source_authority: Some(source_authority),
                        reviewed_native_subject_fingerprint: Some("b".repeat(64)),
                    },
                },
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        let new = vault
            .authorize_calendar_grant(
                installed.setup.setup_id,
                installed.setup.view_handle,
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &request.calendar_ids,
                source_authority,
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                ProcessingRestriction::LocalOnly,
                Some("b".repeat(64).as_str()),
            )
            .await
            .unwrap();
        assert_ne!(old.authority, new.authority);
        assert!(reviewed.registry.revision > enabled.registry.revision);
        assert_eq!(
            vault
                .authorize_calendar_grant(
                    installed.setup.setup_id,
                    installed.setup.view_handle,
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &request.calendar_ids,
                    source_authority,
                    GrantOperation::Read,
                    GrantPurpose::Assistant,
                    GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                    ProcessingRestriction::LocalOnly,
                    Some("a".repeat(64).as_str()),
                )
                .await,
            Err(AgentFailure::AccessReviewRequired)
        );
    }

    #[tokio::test]
    async fn native_pause_constraint_rolls_back_registry_grant_mapping_and_cleanup() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, TestKeys::default())
            .await
            .unwrap();
        let request = setup_request(&vault);
        let installed = vault
            .install_calendar_expert_with_connection(
                request.clone(),
                &test_packaging(),
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        let active = vault
            .configure_calendar_access_with_connection(
                CalendarAccessConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: installed.registry.revision,
                    setup_id: installed.setup.setup_id,
                    change: CalendarAccessChange::SetEnabled { enabled: true },
                },
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("secondary").unwrap(),
            ConnectorId::try_new("calendar.event_kit").unwrap(),
            ExecutionOwnerId::try_new("test-device").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let scope = GrantScope::try_new(
            vec![ResourceHandle::try_new("secondary").unwrap()],
            vec![GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("floe.builtin.schedule").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        vault.create_data_access_grant(source, scope).await.unwrap();
        let raw = vault.database.connect().unwrap();
        raw.execute(
            "CREATE UNIQUE INDEX calendar_grant_state_fault ON data_access_grants (state)",
            (),
        )
        .await
        .unwrap();
        assert!(
            vault
                .configure_calendar_access_with_connection(
                    CalendarAccessConfiguration {
                        instance_id: request.instance_id,
                        expected_revision: active.registry.revision,
                        setup_id: installed.setup.setup_id,
                        change: CalendarAccessChange::SetEnabled { enabled: false },
                    },
                    "opaque-eventkit-connection".into(),
                    &calendar_consumers(),
                    floe_execution::Cancellation::default(),
                )
                .await
                .is_err()
        );
        assert_eq!(vault.check_access(), Err(AgentFailure::VaultUnavailable));
        let mut mapping = raw
            .query(
                "SELECT grant_id FROM calendar_grant_mappings WHERE setup_id = ?",
                [installed.setup.setup_id.to_string()],
            )
            .await
            .unwrap();
        let grant_id = mapping
            .next()
            .await
            .unwrap()
            .unwrap()
            .get::<String>(0)
            .unwrap();
        let mut state = raw
            .query(
                "SELECT state FROM data_access_grants WHERE grant_id = ?",
                [grant_id],
            )
            .await
            .unwrap();
        assert_eq!(
            state
                .next()
                .await
                .unwrap()
                .unwrap()
                .get::<String>(0)
                .unwrap(),
            "active"
        );
        let mut registry = raw
            .query(
                "SELECT revision FROM agent_expert_registry WHERE id = 1",
                (),
            )
            .await
            .unwrap();
        assert_eq!(
            registry
                .next()
                .await
                .unwrap()
                .unwrap()
                .get::<i64>(0)
                .unwrap(),
            2
        );
        let mut cleanup = raw
            .query("SELECT COUNT(*) FROM data_access_grant_cleanup", ())
            .await
            .unwrap();
        assert_eq!(
            cleanup
                .next()
                .await
                .unwrap()
                .unwrap()
                .get::<i64>(0)
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn mapping_identity_corruption_fails_closed_on_reopen() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let request = setup_request(&vault);
        let installed = vault
            .install_calendar_expert_with_connection(
                request,
                &test_packaging(),
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        let raw = vault.database.connect().unwrap();
        raw.execute(
            "UPDATE calendar_grant_mappings SET person_id = ? WHERE setup_id = ?",
            (
                floe_kernel::PersonId::new().to_string(),
                installed.setup.setup_id.to_string(),
            ),
        )
        .await
        .unwrap();
        drop(raw);
        drop(vault);
        assert!(matches!(
            EncryptedAgentVault::open(root.path(), person_id, keys).await,
            Err(AgentFailure::VaultUnavailable)
        ));
    }
}
