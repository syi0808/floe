use floe_access::{
    ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority, GrantConsumer,
    GrantDataCategory, GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding,
    ProcessingRestriction, ResourceHandle, SourceAuthority,
};
use floe_access::{DataAccessGrant, GrantState};
use floe_agent_contract::CalendarProvider;
use floe_experts::{CalendarAccessChange, CalendarAccessConfiguration, RegistrySnapshot};
use floe_experts::{CalendarExpertSetup, CalendarExpertSetupReceipt};
use turso::transaction::TransactionBehavior;

use super::*;
use super::calendar_grant_policy::CalendarGrantPolicy;

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
        if device_id.trim() != device_id || device_id.is_empty() || !source_authority.is_valid() {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let expected_source =
            calendar_source(self.person_id, connection_id, provider, device_id, source_authority)?;
        // Exactly one grant for this exact source, or no read.
        let grants = self
            .data_access_grants_for_source(&expected_source, 2)
            .await?;
        let [grant] = grants.as_slice() else {
            return Err(AgentFailure::AccessReviewRequired);
        };
        if grant.state() != GrantState::Active {
            return Err(if grant.state() == GrantState::Revoked {
                AgentFailure::PolicyDenied
            } else {
                AgentFailure::AccessReviewRequired
            });
        }
        if grant.authority_owner() != self.vault_id {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let policy = self.calendar_grant_policy(grant.id()).await?;
        let native_subject_fingerprint = native_subject_fingerprint
            .ok_or(AgentFailure::AccessReviewRequired)
            .and_then(validate_native_subject_fingerprint)?;
        if policy.reviewed_native_subject_fingerprint.as_deref() != Some(native_subject_fingerprint.as_str()) {
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
            consumer_policy: policy.consumer_policy,
            reviewed_native_subject_fingerprint: native_subject_fingerprint,
        })
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
        let _ = setup;
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
            // A fresh install must not collide with an existing grant for this
            // exact source; re-review goes through SetScope.
            if !self
                .data_access_grants_for_source_in_transaction(&transaction, &source, 1)
                .await?
                .is_empty()
            {
                return Err(AgentFailure::Conflict);
            }
            let grant = self
                .create_data_access_grant_in_transaction(
                    &transaction,
                    GrantId::new(),
                    source,
                    scope,
                )
                .await?;
            let reviewed = self
                .mutate_data_access_grant_in_transaction(
                    &transaction,
                    grant.id(),
                    grant.authority(),
                    super::access_grants::AccessGrantMutation::Review {
                        source: grant.source().clone(),
                        scope: grant.scope().clone(),
                    },
                )
                .await?;
            self.upsert_calendar_grant_policy_in_transaction(
                &transaction,
                &CalendarGrantPolicy {
                    grant_id: reviewed.id(),
                    person_id: self.person_id,
                    consumer_policy: ConsumerPolicyAuthority::new(),
                    reviewed_native_subject_fingerprint: Some(reviewed_native_subject_fingerprint),
                },
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
            // The grant is found by source identity: the previous binding
            // names the device and provider the grant was reviewed under.
            let previous_source = calendar_source(
                self.person_id,
                connection_id,
                binding.provider,
                &binding.device_id,
                binding
                    .source_authority
                    .ok_or(AgentFailure::AccessReviewRequired)?,
            )?;
            let found = self
                .data_access_grants_for_source_in_transaction(&transaction, &previous_source, 2)
                .await?;
            let current = match found.as_slice() {
                [grant] => grant.clone(),
                [] => return Err(AgentFailure::AccessReviewRequired),
                _ => return Err(AgentFailure::VaultUnavailable),
            };
            let previous_policy = self
                .calendar_grant_policy(current.id())
                .await
                .map_err(|_| AgentFailure::AccessReviewRequired)?;
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
                            previous_policy
                                .reviewed_native_subject_fingerprint
                                .clone()
                                .ok_or(AgentFailure::AccessReviewRequired)?,
                            mutation,
                        )
                    }
                    CalendarAccessChange::SetEnabled { enabled: false } => (
                        current.source().clone(),
                        current.scope().clone(),
                        previous_policy
                            .reviewed_native_subject_fingerprint
                            .clone()
                            .ok_or(AgentFailure::AccessReviewRequired)?,
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
                            && previous_policy.reviewed_native_subject_fingerprint.as_deref()
                                != Some(reviewed_native_subject_fingerprint.as_str())
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
                        previous_policy
                            .reviewed_native_subject_fingerprint
                            .clone()
                            .ok_or(AgentFailure::AccessReviewRequired)?,
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
                previous_policy
                    .consumer_policy
                    .advance()
                    .ok_or(AgentFailure::BudgetExceeded)?
            } else {
                previous_policy.consumer_policy
            };
            self.upsert_calendar_grant_policy_in_transaction(
                &transaction,
                &CalendarGrantPolicy {
                    grant_id: updated.id(),
                    person_id: self.person_id,
                    consumer_policy,
                    reviewed_native_subject_fingerprint: Some(reviewed_native_subject_fingerprint),
                },
            )
            .await?;
            let _ = (source, scope);
            self.check_access()?;
            check()?;
            Ok(())
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    async fn data_access_grants_for_source_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        source: &GrantSourceBinding,
        limit: usize,
    ) -> Result<Vec<DataAccessGrant>, AgentFailure> {
        if limit == 0 || limit > 128 {
            return Err(AgentFailure::BudgetExceeded);
        }
        self.ensure_access_grant_schema_transaction(transaction)
            .await?;
        let mut rows = transaction
            .query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, source_incarnation, source_epoch, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE person_id = ? AND authority_owner = ? AND connection_id = ? AND connector = ? AND execution_owner = ? AND source_incarnation = ? AND source_epoch = ? ORDER BY access_epoch DESC, grant_id LIMIT ?", (self.person_id.to_string(), self.vault_id.to_string(), source.connection_id().as_str().to_owned(), source.connector().as_str().to_owned(), source.execution_owner().as_str().to_owned(), source.source_authority().incarnation().to_string(), source.source_authority().epoch().get() as i64, i64::try_from(limit).map_err(|_| AgentFailure::BudgetExceeded)?))
            .await
            .map_err(storage)?;
        let mut grants = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let grant = super::access_grants::decode_grant(&row)?;
            if !grant.source().same_identity(source) {
                return Err(AgentFailure::VaultUnavailable);
            }
            grants.push(grant);
        }
        Ok(grants)
    }
}

pub(super) fn is_native_provider(provider: CalendarProvider) -> bool {
    matches!(
        provider,
        CalendarProvider::EventKit | CalendarProvider::Android
    )
}

pub(super) fn validate_native_subject_fingerprint(value: &str) -> Result<String, AgentFailure> {
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
            .authorize_current_native_calendar_grant(
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
        // Registry-only toggles no longer move Access policy: the grant stands
        // on its own source review, not on setup/assignment state.
        assert_eq!(policy_aba.consumer_policy, stable_policy);
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
                .authorize_current_native_calendar_grant(
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
            .authorize_current_native_calendar_grant(
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
                .authorize_current_native_calendar_grant(
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
            .authorize_current_native_calendar_grant(
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
            .authorize_current_native_calendar_grant(
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
                .authorize_current_native_calendar_grant(
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
        // The failed pause rolled back: the grant is still active.
        // check_access is latched, so verify through raw rows instead.
        let mut state = raw
            .query(
                "SELECT state FROM data_access_grants WHERE person_id = ? AND connection_id = ? AND connector = ? AND execution_owner = ?",
                (
                    person_id.to_string(),
                    "opaque-eventkit-connection",
                    "calendar.event_kit",
                    "test-device",
                ),
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
    async fn policy_identity_corruption_fails_closed_on_reopen() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let request = setup_request(&vault);
        vault
            .install_calendar_expert_with_connection(
                request,
                &test_packaging(),
                "opaque-eventkit-connection".into(),
                &calendar_consumers(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        // Corrupt the policy's indexed person_id so payload and index disagree.
        let raw = vault.database.connect().unwrap();
        raw.execute(
            "UPDATE calendar_grant_policies SET person_id = ?",
            [floe_kernel::PersonId::new().to_string()],
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
