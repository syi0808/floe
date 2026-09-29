use floe_access::{
    ConnectorId, ExecutionOwnerId, GrantAuthority, GrantConsumer,
    GrantDataCategory, GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding,
    ProcessingRestriction, SourceAuthority,
};
use floe_access::{DataAccessGrant, GrantState};
use floe_agent_contract::CalendarProvider;
use turso::transaction::TransactionBehavior;

use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarGrantAdmission {
    pub grant_id: GrantId,
    pub authority: GrantAuthority,
    pub source: GrantSourceBinding,
    pub scope: GrantScope,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn authorize_current_native_calendar_grant(
        &self,
        connection_id: &str,
        provider: CalendarProvider,
        device_id: &str,
        operation: GrantOperation,
        purpose: GrantPurpose,
        consumer: GrantConsumer,
        processing: ProcessingRestriction,
    ) -> Result<CalendarGrantAdmission, AgentFailure> {
        if !matches!(
            provider,
            CalendarProvider::EventKit | CalendarProvider::Android
        ) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        if device_id.trim() != device_id || device_id.is_empty() {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let expected_source = calendar_source(self.person_id, connection_id, provider, device_id)?;
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
        let logical_resource = floe_access::native_calendar_resource(connection_id)?;
        let requested_scope = GrantScope::try_new(
            vec![logical_resource],
            vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
            vec![GrantOperation::Read],
            vec![purpose],
            vec![consumer.clone()],
            processing,
        )
        .map_err(|_| AgentFailure::InvalidInput)?;
        if !grant.scope().consumers().contains(&consumer) {
            return Err(AgentFailure::AccessReviewRequired);
        }
        if requested_scope.resources() != grant.scope().resources()
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
        })
    }

    /// Review and activate one native Calendar grant for an exact source.
    ///
    /// The review names the stable source identity, trusted consumers, and
    /// grant expectation: none for a fresh grant, or the exact id plus
    /// GrantAuthority for an existing one. The current grant is reloaded for validation only;
    /// its freshly loaded authority is never substituted as the expectation.
    pub async fn review_native_calendar_grant(
        &self,
        connection_id: &str,
        provider: CalendarProvider,
        device_id: &str,
        consumers: &[GrantConsumer],
        expected_grant: Option<(GrantId, GrantAuthority)>,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if !is_native_provider(provider) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        if let Some((id, authority)) = expected_grant {
            if !id.is_valid() || !authority.is_valid() {
                return Err(AgentFailure::InvalidInput);
            }
        }
        let (source, scope) = native_calendar_binding(
            self.person_id,
            provider,
            device_id,
            connection_id,
            consumers,
        )?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let found = self
                .data_access_grants_for_source_in_transaction(&transaction, &source, 2)
                .await?;
            let grant = match found.as_slice() {
                [] => {
                    if expected_grant.is_some() {
                        return Err(AgentFailure::Conflict);
                    }
                    let created = self
                        .create_data_access_grant_in_transaction(
                            &transaction,
                            GrantId::new(),
                            source.clone(),
                            scope.clone(),
                        )
                        .await?;
                    let grant = self
                        .mutate_data_access_grant_in_transaction(
                            &transaction,
                            created.id(),
                            created.authority(),
                            super::access_grants::AccessGrantMutation::Activate {
                                scope: scope.clone(),
                            },
                        )
                        .await?;
                    grant
                }
                [current] => {
                    let (expected_id, expected_authority) =
                        expected_grant.ok_or(AgentFailure::InvalidInput)?;
                    if current.id() != expected_id || current.authority() != expected_authority {
                        return Err(AgentFailure::Conflict);
                    }
                    if current.scope().resources() != scope.resources() {
                        return Err(AgentFailure::AccessReviewRequired);
                    }
                    let grant = self
                        .mutate_data_access_grant_in_transaction(
                            &transaction,
                            expected_id,
                            expected_authority,
                            super::access_grants::AccessGrantMutation::Activate {
                                scope: scope.clone(),
                            },
                        )
                        .await?;
                    grant
                }
                _ => return Err(AgentFailure::VaultUnavailable),
            };
            self.check_access()?;
            Ok(grant)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    /// Pause one reviewed native Calendar grant.
    ///
    /// The caller supplies the grant id plus the GrantAuthority the Person
    /// reviewed. The grant must still belong to the exact current native
    /// source; pause changes GrantAuthority only.
    #[allow(clippy::too_many_arguments)]
    pub async fn pause_native_calendar_grant(
        &self,
        grant_id: GrantId,
        expected: GrantAuthority,
        connection_id: &str,
        provider: CalendarProvider,
        device_id: &str,
        source_authority: SourceAuthority,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if !is_native_provider(provider) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        if !grant_id.is_valid() || !expected.is_valid() || !source_authority.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let source = calendar_source(self.person_id, connection_id, provider, device_id)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let current = self
                .read_data_access_grant_in_transaction(&transaction, grant_id)
                .await?;
            if current.source() != &source {
                return Err(AgentFailure::StaleContext);
            }
            let grant = self
                .mutate_data_access_grant_in_transaction(
                    &transaction,
                    grant_id,
                    expected,
                    super::access_grants::AccessGrantMutation::Pause,
                )
                .await?;
            self.check_access()?;
            Ok(grant)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    /// Revoke one reviewed native Calendar grant.
    ///
    /// Revocation is terminal for the grant. Like pause, it is bound to the
    /// reviewed GrantAuthority and the exact current native source.
    #[allow(clippy::too_many_arguments)]
    pub async fn revoke_native_calendar_grant(
        &self,
        grant_id: GrantId,
        expected: GrantAuthority,
        connection_id: &str,
        provider: CalendarProvider,
        device_id: &str,
        source_authority: SourceAuthority,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if !is_native_provider(provider) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        if !grant_id.is_valid() || !expected.is_valid() || !source_authority.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let source = calendar_source(self.person_id, connection_id, provider, device_id)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let current = self
                .read_data_access_grant_in_transaction(&transaction, grant_id)
                .await?;
            if current.source() != &source {
                return Err(AgentFailure::StaleContext);
            }
            let grant = self
                .mutate_data_access_grant_in_transaction(
                    &transaction,
                    grant_id,
                    expected,
                    super::access_grants::AccessGrantMutation::Revoke,
                )
                .await?;
            self.check_access()?;
            Ok(grant)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

}

pub(super) fn is_native_provider(provider: CalendarProvider) -> bool {
    matches!(
        provider,
        CalendarProvider::EventKit | CalendarProvider::Android
    )
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
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

fn native_calendar_binding(
    person_id: floe_kernel::PersonId,
    provider: CalendarProvider,
    device_id: &str,
    connection_id: &str,
    consumers: &[GrantConsumer],
) -> Result<(GrantSourceBinding, GrantScope), AgentFailure> {
    let source = calendar_source(person_id, connection_id, provider, device_id)?;
    let scope = GrantScope::try_new(
        vec![floe_access::native_calendar_resource(connection_id)?],
        vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
        vec![GrantOperation::Read],
        vec![GrantPurpose::Assistant],
        consumers.to_vec(),
        ProcessingRestriction::LocalOnly,
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    Ok((source, scope))
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, os::unix::fs::PermissionsExt, sync::Mutex};

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

    async fn authorize(
        vault: &EncryptedAgentVault<TestKeys>,
        connection: &str,
        _calendars: &[String],
        _authority: SourceAuthority,
        consumer: &str,
        _fingerprint: &str,
    ) -> Result<CalendarGrantAdmission, AgentFailure> {
        vault
            .authorize_current_native_calendar_grant(
                connection,
                CalendarProvider::EventKit,
                "test-device",
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin(consumer).unwrap(),
                ProcessingRestriction::LocalOnly,
            )
            .await
    }

    #[tokio::test]
    async fn native_calendar_grant_requires_reviewed_activation_and_terminal_removal() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, TestKeys::default())
            .await
            .unwrap();
        let source_authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned(), "work".to_owned()];
        let fingerprint = "a".repeat(64);
        // No grant yet: reads require review.
        assert_eq!(
            authorize(
                &vault,
                "opaque-eventkit-connection",
                &calendars,
                source_authority,
                "floe.builtin.schedule",
                &fingerprint,
            )
            .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        let grant = vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendar_consumers(),
                None,
            )
            .await
            .unwrap();
        let admission = authorize(
            &vault,
            "opaque-eventkit-connection",
            &["home".into()],
            source_authority,
            "floe.builtin.schedule",
            &fingerprint,
        )
        .await
        .unwrap();
        assert_eq!(admission.scope.resources().len(), 1);
        assert_eq!(
            admission.scope.resources()[0].as_str(),
            "calendar.timeline:opaque-eventkit-connection"
        );
        for consumer in [
            "floe.builtin.commitments",
            "floe.builtin.focus-attention",
            "floe.builtin.wellbeing",
        ] {
            authorize(
                &vault,
                "opaque-eventkit-connection",
                &["home".into()],
                source_authority,
                consumer,
                &fingerprint,
            )
            .await
            .unwrap();
        }
        assert_eq!(
            authorize(
                &vault,
                "opaque-eventkit-connection",
                &["home".into()],
                source_authority,
                "calendar.expert",
                &fingerprint,
            )
            .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        assert_eq!(
            authorize(
                &vault,
                "another-eventkit-connection",
                &["home".into()],
                source_authority,
                "floe.builtin.schedule",
                &fingerprint,
            )
            .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        // No-op review keeps the standing grant authority.
        let reviewed = vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendar_consumers(),
                Some((grant.id(), grant.authority())),
            )
            .await
            .unwrap();
        let retry_admission = authorize(
            &vault,
            "opaque-eventkit-connection",
            &["home".into()],
            source_authority,
            "floe.builtin.schedule",
            &fingerprint,
        )
        .await
        .unwrap();
        assert_eq!(retry_admission.authority, admission.authority);
        // Pause blocks reads; re-review reactivates with a
        // new GrantAuthority.
        let paused = vault
            .pause_native_calendar_grant(
                reviewed.id(),
                reviewed.authority(),
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                source_authority,
            )
            .await
            .unwrap();
        assert_eq!(
            authorize(
                &vault,
                "opaque-eventkit-connection",
                &calendars,
                source_authority,
                "floe.builtin.schedule",
                &fingerprint,
            )
            .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        let reactivated_grant = vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendar_consumers(),
                Some((paused.id(), paused.authority())),
            )
            .await
            .unwrap();
        let reactivated = authorize(
            &vault,
            "opaque-eventkit-connection",
            &calendars,
            source_authority,
            "floe.builtin.schedule",
            &fingerprint,
        )
        .await
        .unwrap();
        assert_ne!(reactivated.authority, admission.authority);
        vault
            .revoke_native_calendar_grant(
                reactivated_grant.id(),
                reactivated_grant.authority(),
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                source_authority,
            )
            .await
            .unwrap();
        assert_eq!(
            authorize(
                &vault,
                "opaque-eventkit-connection",
                &calendars,
                source_authority,
                "floe.builtin.schedule",
                &fingerprint,
            )
            .await,
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn leaf_scoped_grant_cannot_be_adopted_by_logical_review() {
        let (_root, vault, _) = fresh_vault().await;
        let source = calendar_source(
            vault.person_id,
            "opaque-eventkit-connection",
            CalendarProvider::EventKit,
            "test-device",
        )
        .unwrap();
        let scope = GrantScope::try_new(
            vec![floe_access::ResourceHandle::try_new("home").unwrap()],
            vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            calendar_consumers(),
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        let leaf = vault.create_data_access_grant(source, scope).await.unwrap();
        assert_eq!(
            vault
                .review_native_calendar_grant(
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &calendar_consumers(),
                    Some((leaf.id(), leaf.authority())),
                )
                .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        assert_eq!(
            vault.list_data_access_grants(128).await.unwrap(),
            vec![leaf]
        );
    }

    #[tokio::test]
    async fn native_source_rotation_preserves_standing_grant() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, TestKeys::default())
            .await
            .unwrap();
        let source_authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned(), "work".to_owned()];
        let grant = vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendar_consumers(),
                None,
            )
            .await
            .unwrap();
        let old = authorize(
            &vault,
            "opaque-eventkit-connection",
            &calendars,
            source_authority,
            "floe.builtin.schedule",
            &"a".repeat(64),
        )
        .await
        .unwrap();
        let rotated_authority = source_authority.advance().unwrap();
        let new = authorize(
            &vault,
            "opaque-eventkit-connection",
            &calendars,
            rotated_authority,
            "floe.builtin.schedule",
            &"b".repeat(64),
        )
        .await
        .unwrap();
        assert_eq!(new.grant_id, grant.id());
        assert_eq!(new.authority, old.authority);
    }

    #[tokio::test]
    async fn failed_review_leaves_grant_unchanged() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, TestKeys::default())
            .await
            .unwrap();
        let source_authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned(), "work".to_owned()];
        let grant = vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendar_consumers(),
                None,
            )
            .await
            .unwrap();
        let before = authorize(
            &vault,
            "opaque-eventkit-connection",
            &calendars,
            source_authority,
            "floe.builtin.schedule",
            &"a".repeat(64),
        )
        .await
        .unwrap();
        // A different source cannot borrow the reviewed grant expectation.
        assert_eq!(
            vault
                .review_native_calendar_grant(
                    "other-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &calendar_consumers(),
                    Some((grant.id(), grant.authority())),
                )
                .await,
            Err(AgentFailure::Conflict)
        );
        let after = authorize(
            &vault,
            "opaque-eventkit-connection",
            &calendars,
            source_authority,
            "floe.builtin.schedule",
            &"a".repeat(64),
        )
        .await
        .unwrap();
        assert_eq!(after, before);
    }

    #[tokio::test]
    async fn grant_identity_corruption_fails_closed_on_reopen() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendar_consumers(),
                None,
            )
            .await
            .unwrap();
        let raw = vault.database.connect().unwrap();
        raw.execute(
            "UPDATE data_access_grants SET person_id = ?",
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

    #[tokio::test]
    async fn obsolete_policy_tables_are_rejected_on_reopen() {
        for table in [
            "calendar_grant_policy_schema",
            "calendar_grant_policies",
            "remote_view_grant_schema",
            "remote_view_grant_mappings",
        ] {
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let person_id = floe_kernel::PersonId::new();
            let keys = TestKeys::default();
            let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
                .await
                .unwrap();
            let raw = vault.database.connect().unwrap();
            raw.execute(&format!("CREATE TABLE {table} (id TEXT PRIMARY KEY)"), ())
                .await
                .unwrap();
            drop(raw);
            drop(vault);
            assert!(matches!(
                EncryptedAgentVault::open(root.path(), person_id, keys).await,
                Err(AgentFailure::UnsupportedVersion)
            ));
        }
    }

    #[tokio::test]
    async fn legacy_grant_mapping_tables_are_rejected_not_migrated_on_reopen() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendar_consumers(),
                None,
            )
            .await
            .unwrap();
        // A setup-bound row from the deleted mapping schema.
        let raw = vault.database.connect().unwrap();
        raw.execute(
            "CREATE TABLE calendar_grant_mappings (setup_id TEXT PRIMARY KEY, payload TEXT NOT NULL)",
            (),
        )
        .await
        .unwrap();
        raw.execute(
            "INSERT INTO calendar_grant_mappings (setup_id, payload) VALUES ('old-setup', '{}')",
            (),
        )
        .await
        .unwrap();
        drop(raw);
        drop(vault);
        // Reopen refuses the profile instead of migrating the old rows, and
        // retrying does not silently convert them either.
        assert!(matches!(
            EncryptedAgentVault::open(root.path(), person_id, keys.clone()).await,
            Err(AgentFailure::UnsupportedVersion)
        ));
        assert!(matches!(
            EncryptedAgentVault::open(root.path(), person_id, keys).await,
            Err(AgentFailure::UnsupportedVersion)
        ));
    }

    async fn fresh_vault() -> (
        tempfile::TempDir,
        EncryptedAgentVault<TestKeys>,
        floe_kernel::PersonId,
    ) {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, TestKeys::default())
            .await
            .unwrap();
        (root, vault, person_id)
    }

    async fn fresh_grant(
        vault: &EncryptedAgentVault<TestKeys>,
        _authority: SourceAuthority,
        _calendars: &[String],
        consumers: &[GrantConsumer],
        _fingerprint: &str,
    ) -> DataAccessGrant {
        vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                consumers,
                None,
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn fresh_review_creates_active_grant() {
        let (_root, vault, _) = fresh_vault().await;
        let authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned()];
        let grant = fresh_grant(
            &vault,
            authority,
            &calendars,
            &calendar_consumers(),
            &"a".repeat(64),
        )
        .await;
        assert_eq!(grant.state(), GrantState::Active);
        let admission = authorize(
            &vault,
            "opaque-eventkit-connection",
            &calendars,
            authority,
            "floe.builtin.schedule",
            &"a".repeat(64),
        )
        .await
        .unwrap();
        assert_eq!(admission.grant_id, grant.id());
        assert_eq!(admission.authority, grant.authority());
        assert_eq!(admission.scope.resources(), grant.scope().resources());
        assert_eq!(admission.scope.processing(), grant.scope().processing());
        assert!(grant.scope().consumers().contains(&GrantConsumer::builtin("floe.builtin.schedule").unwrap()));
    }

    #[tokio::test]
    async fn existing_review_without_expected_grant_is_rejected() {
        let (_root, vault, _) = fresh_vault().await;
        let authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned()];
        let grant = fresh_grant(
            &vault,
            authority,
            &calendars,
            &calendar_consumers(),
            &"a".repeat(64),
        )
        .await;
        assert_eq!(
            vault
                .review_native_calendar_grant(
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &calendar_consumers(),
                    None,
                )
                .await,
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(
            vault
                .review_native_calendar_grant(
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &calendar_consumers(),
                    Some((GrantId::new(), grant.authority())),
                )
                .await,
            Err(AgentFailure::Conflict)
        );
        let nil_id: GrantId = serde_json::from_value(serde_json::json!(uuid::Uuid::nil())).unwrap();
        assert_eq!(
            vault
                .review_native_calendar_grant(
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &calendar_consumers(),
                    Some((nil_id, grant.authority())),
                )
                .await,
            Err(AgentFailure::InvalidInput)
        );
    }

    #[tokio::test]
    async fn stale_review_authority_after_pause_is_conflict() {
        let (_root, vault, _) = fresh_vault().await;
        let authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned()];
        let fingerprint = "a".repeat(64);
        let grant = fresh_grant(
            &vault,
            authority,
            &calendars,
            &calendar_consumers(),
            &fingerprint,
        )
        .await;
        let paused = vault
            .pause_native_calendar_grant(
                grant.id(),
                grant.authority(),
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                authority,
            )
            .await
            .unwrap();
        assert_ne!(paused.authority(), grant.authority());
        assert_eq!(
            vault
                .review_native_calendar_grant(
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &calendar_consumers(),
                    Some((grant.id(), grant.authority())),
                )
                .await,
            Err(AgentFailure::Conflict)
        );
        vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendar_consumers(),
                Some((paused.id(), paused.authority())),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn stale_pause_and_revoke_authority_is_conflict() {
        let (_root, vault, _) = fresh_vault().await;
        let authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned()];
        let grant = fresh_grant(
            &vault,
            authority,
            &calendars,
            &calendar_consumers(),
            &"a".repeat(64),
        )
        .await;
        assert_eq!(
            vault
                .pause_native_calendar_grant(
                    grant.id(),
                    GrantAuthority::new(),
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    authority,
                )
                .await,
            Err(AgentFailure::Conflict)
        );
        let paused = vault
            .pause_native_calendar_grant(
                grant.id(),
                grant.authority(),
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                authority,
            )
            .await
            .unwrap();
        assert_eq!(
            vault
                .pause_native_calendar_grant(
                    grant.id(),
                    grant.authority(),
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    authority,
                )
                .await,
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            vault
                .revoke_native_calendar_grant(
                    grant.id(),
                    grant.authority(),
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    authority,
                )
                .await,
            Err(AgentFailure::Conflict)
        );
        vault
            .revoke_native_calendar_grant(
                paused.id(),
                paused.authority(),
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                authority,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn pause_for_a_different_source_is_rejected() {
        let (_root, vault, _) = fresh_vault().await;
        let authority = SourceAuthority::new();
        let grant = fresh_grant(
            &vault,
            authority,
            &["home".to_owned()],
            &calendar_consumers(),
            &"a".repeat(64),
        )
        .await;
        assert_eq!(
            vault
                .pause_native_calendar_grant(
                    grant.id(),
                    grant.authority(),
                    "other-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    authority,
                )
                .await,
            Err(AgentFailure::StaleContext)
        );
    }

    #[tokio::test]
    async fn resource_change_preserves_logical_grant() {
        let (_root, vault, _) = fresh_vault().await;
        let authority = SourceAuthority::new();
        let fingerprint = "a".repeat(64);
        let grant = fresh_grant(
            &vault,
            authority,
            &["home".to_owned()],
            &calendar_consumers(),
            &fingerprint,
        )
        .await;
        let before = authorize(
            &vault,
            "opaque-eventkit-connection",
            &["home".to_owned()],
            authority,
            "floe.builtin.schedule",
            &fingerprint,
        )
        .await
        .unwrap();
        let changed_source_authority = authority.advance().unwrap();
        let after = authorize(
            &vault,
            "opaque-eventkit-connection",
            &["home".to_owned(), "work".to_owned()],
            changed_source_authority,
            "floe.builtin.schedule",
            &fingerprint,
        )
        .await
        .unwrap();
        assert_eq!(after.grant_id, grant.id());
        assert_eq!(after.authority, grant.authority());
        assert_eq!(after.scope.resources(), before.scope.resources());
    }

    #[tokio::test]
    async fn consumer_change_advances_grant_authority() {
        let (_root, vault, _) = fresh_vault().await;
        let authority = SourceAuthority::new();
        let fingerprint = "a".repeat(64);
        let calendars = vec!["home".to_owned()];
        let grant = fresh_grant(
            &vault,
            authority,
            &calendars,
            &calendar_consumers(),
            &fingerprint,
        )
        .await;
        let before = authorize(
            &vault,
            "opaque-eventkit-connection",
            &calendars,
            authority,
            "floe.builtin.schedule",
            &fingerprint,
        )
        .await
        .unwrap();
        let subset = vec![GrantConsumer::builtin("floe.builtin.schedule").unwrap()];
        let updated = vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &subset,
                Some((grant.id(), grant.authority())),
            )
            .await
            .unwrap();
        assert_ne!(updated.authority(), grant.authority());
        let after = authorize(
            &vault,
            "opaque-eventkit-connection",
            &calendars,
            authority,
            "floe.builtin.schedule",
            &fingerprint,
        )
        .await
        .unwrap();
        assert_eq!(after.authority, updated.authority());
        assert_ne!(after.authority, before.authority);
    }

    #[tokio::test]
    async fn missing_grant_row_for_reviewed_expectation_fails_closed() {
        let (_root, vault, _) = fresh_vault().await;
        let authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned()];
        let fingerprint = "a".repeat(64);
        let grant = fresh_grant(
            &vault,
            authority,
            &calendars,
            &calendar_consumers(),
            &fingerprint,
        )
        .await;
        let raw = vault.database.connect().unwrap();
        raw.execute(
            "DELETE FROM data_access_grants WHERE grant_id = ?",
            [grant.id().as_uuid().to_string()],
        )
        .await
        .unwrap();
        drop(raw);
        assert_eq!(
            authorize(
                &vault,
                "opaque-eventkit-connection",
                &calendars,
                authority,
                "floe.builtin.schedule",
                &fingerprint,
            )
            .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        assert_eq!(
            vault
                .review_native_calendar_grant(
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &calendar_consumers(),
                    Some((grant.id(), grant.authority())),
                )
                .await,
            Err(AgentFailure::Conflict)
        );
    }

    #[tokio::test]
    async fn corrupt_grant_payload_fails_closed_on_review() {
        let (_root, vault, _) = fresh_vault().await;
        let authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned()];
        let fingerprint = "a".repeat(64);
        let grant = fresh_grant(
            &vault,
            authority,
            &calendars,
            &calendar_consumers(),
            &fingerprint,
        )
        .await;
        let raw = vault.database.connect().unwrap();
        raw.execute(
            "UPDATE data_access_grants SET payload = 'not-json' WHERE grant_id = ?",
            [grant.id().as_uuid().to_string()],
        )
        .await
        .unwrap();
        drop(raw);
        assert_eq!(
            vault
                .review_native_calendar_grant(
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &calendar_consumers(),
                    Some((grant.id(), grant.authority())),
                )
                .await,
            Err(AgentFailure::VaultUnavailable)
        );
    }

    #[tokio::test]
    async fn pause_alone_preserves_grant_scope() {
        let (_root, vault, _) = fresh_vault().await;
        let authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned()];
        let fingerprint = "a".repeat(64);
        let grant = fresh_grant(
            &vault,
            authority,
            &calendars,
            &calendar_consumers(),
            &fingerprint,
        )
        .await;
        let before = grant.scope().clone();
        let paused = vault
            .pause_native_calendar_grant(
                grant.id(),
                grant.authority(),
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                authority,
            )
            .await
            .unwrap();
        assert_eq!(paused.scope(), &before);
        let reactivated = vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendar_consumers(),
                Some((paused.id(), paused.authority())),
            )
            .await
            .unwrap();
        assert_eq!(reactivated.scope(), &before);
        assert_ne!(reactivated.authority(), grant.authority());
    }

    #[tokio::test]
    async fn old_dependency_fails_after_grant_change() {
        let (_root, vault, person_id) = fresh_vault().await;
        let authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned()];
        let fingerprint = "a".repeat(64);
        let grant = fresh_grant(
            &vault,
            authority,
            &calendars,
            &calendar_consumers(),
            &fingerprint,
        )
        .await;
        let admission = authorize(
            &vault,
            "opaque-eventkit-connection",
            &calendars,
            authority,
            "floe.builtin.schedule",
            &fingerprint,
        )
        .await
        .unwrap();
        let observed_at = chrono::Utc::now();
        let old_dependency = floe_access::ContextDependency::try_new(
            person_id,
            admission.grant_id,
            admission.authority,
            admission.source.clone(),
            admission.scope.resources().to_vec(),
            authority,
            admission.scope.resources().to_vec(),
            admission.scope.categories().to_vec(),
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
            ProcessingRestriction::LocalOnly,
            Uuid::new_v4(),
            vec![1],
            Uuid::new_v4(),
            Uuid::new_v4(),
            observed_at,
            observed_at + chrono::Duration::minutes(59),
        )
        .unwrap();
        let paused = vault
            .pause_native_calendar_grant(
                grant.id(),
                grant.authority(),
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                authority,
            )
            .await
            .unwrap();
        let reactivated = vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendar_consumers(),
                Some((paused.id(), paused.authority())),
            )
            .await
            .unwrap();
        assert_eq!(
            floe_access::validate_grant_dependency(&reactivated, &old_dependency),
            Err(AgentFailure::PolicyDenied)
        );
        let rotated_authority = authority.advance().unwrap();
        let rotated = vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendar_consumers(),
                Some((reactivated.id(), reactivated.authority())),
            )
            .await
            .unwrap();
        let current = authorize(
            &vault,
            "opaque-eventkit-connection",
            &calendars,
            rotated_authority,
            "floe.builtin.schedule",
            &"b".repeat(64),
        )
        .await
        .unwrap();
        assert_eq!(current.grant_id, rotated.id());
        assert_eq!(current.authority, rotated.authority());
        assert_eq!(
            floe_access::validate_grant_dependency(&rotated, &old_dependency),
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn fresh_native_grant_survives_reopen() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned()];
        let fingerprint = "a".repeat(64);
        let (grant_id, grant_authority) = {
            let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
                .await
                .unwrap();
            let grant = fresh_grant(
                &vault,
                authority,
                &calendars,
                &calendar_consumers(),
                &fingerprint,
            )
            .await;
            let admission = authorize(
                &vault,
                "opaque-eventkit-connection",
                &calendars,
                authority,
                "floe.builtin.schedule",
                &fingerprint,
            )
            .await
            .unwrap();
            assert_eq!(admission.grant_id, grant.id());
            (grant.id(), grant.authority())
        };
        let vault = EncryptedAgentVault::open(root.path(), person_id, keys)
            .await
            .unwrap();
        let admission = authorize(
            &vault,
            "opaque-eventkit-connection",
            &calendars,
            authority,
            "floe.builtin.schedule",
            &fingerprint,
        )
        .await
        .unwrap();
        assert_eq!(admission.grant_id, grant_id);
        assert_eq!(admission.authority, grant_authority);
    }
}
