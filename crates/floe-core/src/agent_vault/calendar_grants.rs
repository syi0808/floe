use std::num::NonZeroU64;

use floe_agent::{
    CalendarAccessChange, CalendarAccessConfiguration, CalendarExpertSetup,
    CalendarExpertSetupReceipt, RegistrySnapshot,
};
use floe_domain::{
    CalendarProvider, ConnectorId, DataAccessGrant, ExecutionOwnerId, GrantAuthority,
    GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose, GrantScope,
    GrantSourceBinding, GrantState, ProcessingRestriction, ResourceHandle, SourceAuthority,
};
use serde::{Deserialize, Serialize};
use turso::{Row, transaction::TransactionBehavior};
use uuid::Uuid;

use super::*;

const CALENDAR_GRANT_SCHEMA_VERSION: i64 = 1;
const MAX_CALENDAR_GRANT_MAPPINGS: usize = 128;
const MAX_MAPPING_PAYLOAD_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarGrantAdmission {
    pub grant_id: GrantId,
    pub authority: GrantAuthority,
    pub source: GrantSourceBinding,
    pub scope: GrantScope,
    pub grant_scope: GrantScope,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct CalendarGrantMapping {
    setup_id: Uuid,
    view_handle: Uuid,
    grant_id: GrantId,
    source: GrantSourceBinding,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
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
                        "CREATE TABLE calendar_grant_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "INSERT INTO calendar_grant_schema (id, version) VALUES (1, 1)",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "CREATE TABLE calendar_grant_mappings (setup_id TEXT PRIMARY KEY, view_handle TEXT NOT NULL UNIQUE, grant_id TEXT NOT NULL UNIQUE, person_id TEXT NOT NULL, connection_id TEXT NOT NULL, connector TEXT NOT NULL, execution_owner TEXT NOT NULL, source_incarnation TEXT NOT NULL, source_epoch INTEGER NOT NULL, payload TEXT NOT NULL)",
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
            .query("SELECT setup_id, view_handle, grant_id, person_id, connection_id, connector, execution_owner, source_incarnation, source_epoch, payload FROM calendar_grant_mappings", ())
            .await
            .map_err(storage)?;
        let mut count = 0;
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
                "SELECT setup_id, view_handle, grant_id, person_id, connection_id, connector, execution_owner, source_incarnation, source_epoch, payload FROM calendar_grant_mappings WHERE setup_id = ? AND view_handle = ? AND person_id = ?",
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
        let requested_scope =
            calendar_scope(calendar_ids.to_vec(), consumer.clone(), purpose, processing)?;
        if requested_scope
            .resources()
            .iter()
            .any(|resource| !grant.scope().resources().contains(resource))
            || requested_scope.categories() != grant.scope().categories()
            || requested_scope.operations() != grant.scope().operations()
            || requested_scope.purposes() != grant.scope().purposes()
            || requested_scope.consumers() != grant.scope().consumers()
            || requested_scope.processing() != grant.scope().processing()
            || !grant.scope().operations().contains(&operation)
            || !grant.scope().purposes().contains(&purpose)
            || !grant.scope().consumers().contains(&consumer)
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        Ok(CalendarGrantAdmission {
            grant_id: grant.id(),
            authority: grant.authority(),
            source: grant.source().clone(),
            scope: requested_scope,
            grant_scope: grant.scope().clone(),
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
                "SELECT setup_id, view_handle, grant_id, person_id, connection_id, connector, execution_owner, source_incarnation, source_epoch, payload FROM calendar_grant_mappings WHERE setup_id = ? AND person_id = ?",
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
        let (source, scope) =
            calendar_binding(self.person_id, request, connection_id, source_authority)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
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
            self.insert_calendar_mapping(&transaction, setup, &reviewed)
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
            let (source, scope, mutation) = match &configuration.change {
                CalendarAccessChange::SetEnabled { enabled: true } => {
                    let mutation = super::access_grants::AccessGrantMutation::Activate {
                        source: current.source().clone(),
                        scope: current.scope().clone(),
                    };
                    (current.source().clone(), current.scope().clone(), mutation)
                }
                CalendarAccessChange::SetEnabled { enabled: false } => (
                    current.source().clone(),
                    current.scope().clone(),
                    super::access_grants::AccessGrantMutation::Pause,
                ),
                CalendarAccessChange::SetScope {
                    provider,
                    device_id,
                    calendar_ids,
                    source_authority,
                    ..
                } => {
                    let authority = source_authority.ok_or(AgentFailure::AccessReviewRequired)?;
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
                    };
                    let (source, scope) =
                        calendar_binding(self.person_id, &request, connection_id, authority)?;
                    let mutation = if current.state() == GrantState::Active {
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
                    (source, scope, mutation)
                }
                CalendarAccessChange::Remove {} => (
                    current.source().clone(),
                    current.scope().clone(),
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
            self.update_calendar_mapping(&transaction, &mapping, &updated, source, scope)
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
    ) -> Result<(), AgentFailure> {
        let mapping = CalendarGrantMapping {
            setup_id: setup.setup_id,
            view_handle: setup.view_handle,
            grant_id: grant.id(),
            source: grant.source().clone(),
        };
        let payload =
            serde_json::to_string(&mapping).map_err(|_| AgentFailure::StorageUnavailable)?;
        if payload.len() > MAX_MAPPING_PAYLOAD_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        transaction
            .execute(
                "INSERT INTO calendar_grant_mappings (setup_id, view_handle, grant_id, person_id, connection_id, connector, execution_owner, source_incarnation, source_epoch, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                mapping_values(&mapping, payload)?,
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
    ) -> Result<(), AgentFailure> {
        let mapping = CalendarGrantMapping {
            setup_id: previous.setup_id,
            view_handle: previous.view_handle,
            grant_id: grant.id(),
            source,
        };
        let payload =
            serde_json::to_string(&mapping).map_err(|_| AgentFailure::StorageUnavailable)?;
        let changed = transaction
            .execute(
                "UPDATE calendar_grant_mappings SET grant_id = ?, person_id = ?, connection_id = ?, connector = ?, execution_owner = ?, source_incarnation = ?, source_epoch = ?, payload = ? WHERE setup_id = ? AND person_id = ? AND grant_id = ?",
                (
                    mapping.grant_id.as_uuid().to_string(),
                    self.person_id.to_string(),
                    mapping.source.connection_id().as_str().to_owned(),
                    mapping.source.connector().as_str().to_string(),
                    mapping.source.execution_owner().as_str().to_string(),
                    mapping.source.source_authority().incarnation().to_string(),
                    i64::try_from(mapping.source.source_authority().epoch().get()).map_err(|_| AgentFailure::Conflict)?,
                    payload,
                    mapping.setup_id.to_string(),
                    self.person_id.to_string(),
                    previous.grant_id.as_uuid().to_string(),
                ),
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
            .query("SELECT setup_id, view_handle, grant_id, person_id, connection_id, connector, execution_owner, source_incarnation, source_epoch, payload FROM calendar_grant_mappings WHERE setup_id = ? AND person_id = ?", (setup_id.to_string(), self.person_id.to_string()))
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        decode_mapping(&row)
    }
}

fn is_native_provider(provider: CalendarProvider) -> bool {
    matches!(
        provider,
        CalendarProvider::EventKit | CalendarProvider::Android
    )
}

fn connector(provider: CalendarProvider) -> Result<ConnectorId, AgentFailure> {
    ConnectorId::try_new(match provider {
        CalendarProvider::EventKit => "calendar.event_kit",
        CalendarProvider::Android => "calendar.android",
        _ => return Err(AgentFailure::CapabilityUnavailable),
    })
    .map_err(|_| AgentFailure::InvalidInput)
}

fn calendar_source(
    person_id: floe_domain::PersonId,
    connection_id: &str,
    provider: CalendarProvider,
    device_id: &str,
    source_authority: SourceAuthority,
) -> Result<GrantSourceBinding, AgentFailure> {
    let connection_id = floe_domain::ConnectionId::try_new(connection_id.to_owned())
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
    person_id: floe_domain::PersonId,
    request: &CalendarExpertSetup,
    connection_id: &str,
    source_authority: SourceAuthority,
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
        vec![GrantConsumer::builtin("calendar.expert").map_err(|_| AgentFailure::InvalidInput)?],
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

fn mapping_values(
    mapping: &CalendarGrantMapping,
    payload: String,
) -> Result<
    (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
    ),
    AgentFailure,
> {
    Ok((
        mapping.setup_id.to_string(),
        mapping.view_handle.to_string(),
        mapping.grant_id.as_uuid().to_string(),
        mapping.source.person_id().to_string(),
        mapping.source.connection_id().as_str().to_owned(),
        mapping.source.connector().as_str().to_string(),
        mapping.source.execution_owner().as_str().to_string(),
        mapping.source.source_authority().incarnation().to_string(),
        i64::try_from(mapping.source.source_authority().epoch().get())
            .map_err(|_| AgentFailure::Conflict)?,
        payload,
    ))
}

fn decode_mapping(row: &Row) -> Result<CalendarGrantMapping, AgentFailure> {
    let payload = row.get::<String>(9).map_err(storage)?;
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
    let person_id = floe_domain::PersonId(
        Uuid::parse_str(&row.get::<String>(3).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
    );
    let connection_id = floe_domain::ConnectionId::try_new(row.get::<String>(4).map_err(storage)?)
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
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(mapping)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, os::unix::fs::PermissionsExt, sync::Mutex};

    use floe_domain::ConnectionId;

    use super::*;

    #[derive(Clone, Default)]
    struct TestKeys(std::sync::Arc<Mutex<HashMap<(floe_domain::PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for TestKeys {
        fn load(
            &self,
            person_id: floe_domain::PersonId,
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
            person_id: floe_domain::PersonId,
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
            connection_scope: floe_domain::CalendarScope::Selected,
            connection_revision: 1,
            source_authority: Some(SourceAuthority::new()),
        }
    }

    #[tokio::test]
    async fn native_calendar_grant_requires_reviewed_activation_and_terminal_removal() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_domain::PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, TestKeys::default())
            .await
            .unwrap();
        let request = setup_request(&vault);
        let source_authority = request.source_authority.unwrap();
        let installed = vault
            .install_calendar_expert_with_connection(
                request.clone(),
                "opaque-eventkit-connection".into(),
                floe_agent::Cancellation::default(),
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
                GrantConsumer::builtin("calendar.expert").unwrap(),
                ProcessingRestriction::LocalOnly,
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
                floe_agent::Cancellation::default(),
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
                GrantConsumer::builtin("calendar.expert").unwrap(),
                ProcessingRestriction::LocalOnly,
            )
            .await
            .unwrap();
        assert_eq!(admission.scope.resources().len(), 1);
        assert_eq!(admission.grant_scope.resources().len(), 2);
        let paused = vault
            .configure_calendar_access_with_connection(
                CalendarAccessConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: active.registry.revision,
                    setup_id: installed.setup.setup_id,
                    change: CalendarAccessChange::SetEnabled { enabled: false },
                },
                "opaque-eventkit-connection".into(),
                floe_agent::Cancellation::default(),
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
                    GrantConsumer::builtin("calendar.expert").unwrap(),
                    ProcessingRestriction::LocalOnly,
                )
                .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        vault
            .configure_calendar_access_with_connection(
                CalendarAccessConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: paused.registry.revision,
                    setup_id: installed.setup.setup_id,
                    change: CalendarAccessChange::Remove {},
                },
                "opaque-eventkit-connection".into(),
                floe_agent::Cancellation::default(),
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
                    GrantConsumer::builtin("calendar.expert").unwrap(),
                    ProcessingRestriction::LocalOnly,
                )
                .await,
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn native_pause_constraint_rolls_back_registry_grant_mapping_and_cleanup() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_domain::PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, TestKeys::default())
            .await
            .unwrap();
        let request = setup_request(&vault);
        let installed = vault
            .install_calendar_expert_with_connection(
                request.clone(),
                "opaque-eventkit-connection".into(),
                floe_agent::Cancellation::default(),
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
                floe_agent::Cancellation::default(),
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
            vec![GrantConsumer::builtin("calendar.expert").unwrap()],
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
                    floe_agent::Cancellation::default(),
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
        let person_id = floe_domain::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let request = setup_request(&vault);
        let installed = vault
            .install_calendar_expert_with_connection(
                request,
                "opaque-eventkit-connection".into(),
                floe_agent::Cancellation::default(),
            )
            .await
            .unwrap();
        let raw = vault.database.connect().unwrap();
        raw.execute(
            "UPDATE calendar_grant_mappings SET person_id = ? WHERE setup_id = ?",
            (
                floe_domain::PersonId::new().to_string(),
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
