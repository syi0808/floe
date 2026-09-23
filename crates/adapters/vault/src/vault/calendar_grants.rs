use floe_access::{
    ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority, GrantConsumer,
    GrantDataCategory, GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding,
    ProcessingRestriction, ResourceHandle, SourceAuthority,
};
use floe_access::{DataAccessGrant, GrantState};
use floe_agent_contract::CalendarProvider;
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

    /// Review and activate one native Calendar grant for an exact source.
    ///
    /// The review names the source identity (connection, provider, device,
    /// authority), the calendars in scope, the consumers admitted, and the
    /// subject fingerprint the Person approved. Re-review with a new
    /// fingerprint or scope advances the active authority.
    #[allow(clippy::too_many_arguments)]
    pub async fn review_native_calendar_grant(
        &self,
        connection_id: &str,
        provider: CalendarProvider,
        device_id: &str,
        calendar_ids: &[String],
        source_authority: SourceAuthority,
        consumers: &[GrantConsumer],
        native_subject_fingerprint: &str,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if !is_native_provider(provider) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let fingerprint = validate_native_subject_fingerprint(native_subject_fingerprint)?;
        let (source, scope) = calendar_binding(
            self.person_id,
            provider,
            device_id,
            calendar_ids,
            connection_id,
            source_authority,
            consumers,
        )?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            self.ensure_calendar_grant_policy_schema(&transaction)
                .await?;
            let found = self
                .data_access_grants_for_source_in_transaction(&transaction, &source, 2)
                .await?;
            let grant = match found.as_slice() {
                [] => {
                    let created = self
                        .create_data_access_grant_in_transaction(
                            &transaction,
                            GrantId::new(),
                            source,
                            scope.clone(),
                        )
                        .await?;
                    self.mutate_data_access_grant_in_transaction(
                        &transaction,
                        created.id(),
                        created.authority(),
                        super::access_grants::AccessGrantMutation::Activate {
                            source: created.source().clone(),
                            scope,
                        },
                    )
                    .await?
                }
                [current] => {
                    let mutation = if current.state() == GrantState::Active {
                        super::access_grants::AccessGrantMutation::Activate {
                            source: current.source().clone(),
                            scope: scope.clone(),
                        }
                    } else {
                        super::access_grants::AccessGrantMutation::Review {
                            source: current.source().clone(),
                            scope: scope.clone(),
                        }
                    };
                    let updated = self
                        .mutate_data_access_grant_in_transaction(
                            &transaction,
                            current.id(),
                            current.authority(),
                            mutation,
                        )
                        .await?;
                    if updated.state() != GrantState::Active {
                        self.mutate_data_access_grant_in_transaction(
                            &transaction,
                            updated.id(),
                            updated.authority(),
                            super::access_grants::AccessGrantMutation::Activate {
                                source: updated.source().clone(),
                                scope: updated.scope().clone(),
                            },
                        )
                        .await?
                    } else {
                        updated
                    }
                }
                _ => return Err(AgentFailure::VaultUnavailable),
            };
            let previous_policy = self.calendar_grant_policy(grant.id()).await.ok();
            // The policy advances when the review changes what the Person
            // approved (fingerprint) or reactivates a non-active grant.
            let reactivated = match found.as_slice() {
                [current] => current.state() != GrantState::Active,
                _ => false,
            };
            let consumer_policy = match previous_policy {
                Some(previous)
                    if !reactivated
                        && previous.reviewed_native_subject_fingerprint.as_deref()
                            == Some(fingerprint.as_str()) =>
                {
                    previous.consumer_policy
                }
                Some(previous) => previous
                    .consumer_policy
                    .advance()
                    .ok_or(AgentFailure::BudgetExceeded)?,
                None => ConsumerPolicyAuthority::new(),
            };
            self.upsert_calendar_grant_policy_in_transaction(
                &transaction,
                &CalendarGrantPolicy {
                    grant_id: grant.id(),
                    person_id: self.person_id,
                    consumer_policy,
                    reviewed_native_subject_fingerprint: Some(fingerprint),
                },
            )
            .await?;
            self.check_access()?;
            Ok(grant)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    /// Pause the native Calendar grant for one exact source.
    pub async fn pause_native_calendar_grant(
        &self,
        connection_id: &str,
        provider: CalendarProvider,
        device_id: &str,
        source_authority: SourceAuthority,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if !is_native_provider(provider) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let source =
            calendar_source(self.person_id, connection_id, provider, device_id, source_authority)?;
        let grants = self.data_access_grants_for_source(&source, 2).await?;
        let [grant] = grants.as_slice() else {
            return Err(AgentFailure::AccessReviewRequired);
        };
        self.pause_data_access_grant(grant.id(), grant.authority())
            .await
    }

    /// Revoke the native Calendar grant for one exact source.
    pub async fn revoke_native_calendar_grant(
        &self,
        connection_id: &str,
        provider: CalendarProvider,
        device_id: &str,
        source_authority: SourceAuthority,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if !is_native_provider(provider) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let source =
            calendar_source(self.person_id, connection_id, provider, device_id, source_authority)?;
        let grants = self.data_access_grants_for_source(&source, 2).await?;
        let [grant] = grants.as_slice() else {
            return Err(AgentFailure::AccessReviewRequired);
        };
        self.revoke_data_access_grant(grant.id(), grant.authority())
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
    provider: CalendarProvider,
    device_id: &str,
    calendar_ids: &[String],
    connection_id: &str,
    source_authority: SourceAuthority,
    consumers: &[GrantConsumer],
) -> Result<(GrantSourceBinding, GrantScope), AgentFailure> {
    let source = calendar_source(person_id, connection_id, provider, device_id, source_authority)?;
    let resources = calendar_ids
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
        calendars: &[String],
        authority: SourceAuthority,
        consumer: &str,
        fingerprint: &str,
    ) -> Result<CalendarGrantAdmission, AgentFailure> {
        vault
            .authorize_current_native_calendar_grant(
                connection,
                CalendarProvider::EventKit,
                "test-device",
                calendars,
                authority,
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin(consumer).unwrap(),
                ProcessingRestriction::LocalOnly,
                Some(fingerprint),
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
        vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendars,
                source_authority,
                &calendar_consumers(),
                &fingerprint,
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
        assert_eq!(admission.grant_scope.resources().len(), 2);
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
        assert_eq!(
            authorize(
                &vault,
                "opaque-eventkit-connection",
                &["home".into()],
                source_authority,
                "floe.builtin.schedule",
                &"b".repeat(64),
            )
            .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        let stable_policy = admission.consumer_policy;
        // Re-review with the same fingerprint keeps the policy.
        vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendars,
                source_authority,
                &calendar_consumers(),
                &fingerprint,
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
        assert_eq!(retry_admission.consumer_policy, stable_policy);
        // Pause blocks reads; re-review reactivates with a new policy.
        vault
            .pause_native_calendar_grant(
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
        vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendars,
                source_authority,
                &calendar_consumers(),
                &fingerprint,
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
        assert_ne!(reactivated.consumer_policy, stable_policy);
        vault
            .revoke_native_calendar_grant(
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
    async fn native_subject_rereview_advances_active_authority_and_rejects_old_fingerprint() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, TestKeys::default())
            .await
            .unwrap();
        let source_authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned(), "work".to_owned()];
        vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendars,
                source_authority,
                &calendar_consumers(),
                &"a".repeat(64),
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
        vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendars,
                source_authority,
                &calendar_consumers(),
                &"b".repeat(64),
            )
            .await
            .unwrap();
        let new = authorize(
            &vault,
            "opaque-eventkit-connection",
            &calendars,
            source_authority,
            "floe.builtin.schedule",
            &"b".repeat(64),
        )
        .await
        .unwrap();
        assert_ne!(new.consumer_policy, old.consumer_policy);
        assert_eq!(
            authorize(
                &vault,
                "opaque-eventkit-connection",
                &calendars,
                source_authority,
                "floe.builtin.schedule",
                &"a".repeat(64),
            )
            .await,
            Err(AgentFailure::AccessReviewRequired)
        );
    }

    #[tokio::test]
    async fn failed_review_leaves_grant_and_policy_unchanged() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_kernel::PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, TestKeys::default())
            .await
            .unwrap();
        let source_authority = SourceAuthority::new();
        let calendars = vec!["home".to_owned(), "work".to_owned()];
        vault
            .review_native_calendar_grant(
                "opaque-eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &calendars,
                source_authority,
                &calendar_consumers(),
                &"a".repeat(64),
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
        // An invalid fingerprint fails the whole review atomically.
        assert_eq!(
            vault
                .review_native_calendar_grant(
                    "opaque-eventkit-connection",
                    CalendarProvider::EventKit,
                    "test-device",
                    &calendars,
                    source_authority,
                    &calendar_consumers(),
                    "not-a-fingerprint",
                )
                .await,
            Err(AgentFailure::AccessReviewRequired)
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
    async fn policy_identity_corruption_fails_closed_on_reopen() {
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
                &["home".to_owned()],
                SourceAuthority::new(),
                &calendar_consumers(),
                &"a".repeat(64),
            )
            .await
            .unwrap();
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
