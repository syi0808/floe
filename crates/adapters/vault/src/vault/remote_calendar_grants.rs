use floe_access::DataAccessGrant;
use floe_access::{
    ConsumerPolicyAuthority, GrantAuthority, GrantId, GrantScope, GrantSourceBinding,
};
use floe_agent_contract::AgentFailure;

use super::{EncryptedAgentVault, VaultKeyProvider, access_grants::AccessGrantMutation};
use super::calendar_grant_policy::CalendarGrantPolicy;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteCalendarGrantBinding {
    pub grant: DataAccessGrant,
    pub consumer_policy: ConsumerPolicyAuthority,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn remote_calendar_grant_binding(
        &self,
        connector: &str,
        connection_id: &str,
        source_authority: floe_access::SourceAuthority,
        resource: &str,
    ) -> Result<RemoteCalendarGrantBinding, AgentFailure> {
        if connector.is_empty() || connection_id.is_empty() || resource.is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        // Exactly one grant for this exact connection, authority and resource,
        // or no read. Remote execution owners vary by deployment, so the owner
        // is not part of the match; a sibling resource grant is never a match.
        let grants = self.list_data_access_grants(128).await?;
        let mut found = None;
        for grant in &grants {
            let source = grant.source();
            if source.connector().as_str() == connector
                && source.connection_id().as_str() == connection_id
                && source.source_authority() == source_authority
                && grant.scope().resources().len() == 1
                && grant.scope().resources()[0].as_str() == resource
            {
                if found.is_some() {
                    return Err(AgentFailure::Conflict);
                }
                found = Some(grant.clone());
            }
        }
        let grant = found.ok_or(AgentFailure::AccessReviewRequired)?;
        if grant.authority_owner() != self.vault_id
            || grant.state() != floe_access::GrantState::Active
            || grant.review_required()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let policy = self.calendar_grant_policy(grant.id()).await?;
        Ok(RemoteCalendarGrantBinding {
            grant,
            consumer_policy: policy.consumer_policy,
        })
    }

    /// The single grant for one exact Calendar source identity and resource.
    ///
    /// The source authority is not part of the match: a review under a rotated
    /// authority updates the same grant rather than forking a duplicate. Any
    /// state matches; two grants for the same exact resource conflict.
    pub async fn find_remote_calendar_grant(
        &self,
        source: &GrantSourceBinding,
        resource: &str,
    ) -> Result<Option<DataAccessGrant>, AgentFailure> {
        if source.person_id() != self.person_id || resource.is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        let grants = self.list_data_access_grants(128).await?;
        let mut found = None;
        for grant in &grants {
            if grant.source().same_identity(source)
                && grant.scope().resources().len() == 1
                && grant.scope().resources()[0].as_str() == resource
            {
                if found.is_some() {
                    return Err(AgentFailure::Conflict);
                }
                found = Some(grant.clone());
            }
        }
        Ok(found)
    }

    /// The consumer-policy authority one Calendar review established.
    ///
    /// Callers hold the grant already, so a missing row is corrupt state rather
    /// than a review the Person owes.
    pub async fn calendar_grant_policy_authority(
        &self,
        grant_id: GrantId,
    ) -> Result<ConsumerPolicyAuthority, AgentFailure> {
        if !grant_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        self.calendar_grant_policy(grant_id)
            .await
            .map(|policy| policy.consumer_policy)
            .map_err(|failure| match failure {
                AgentFailure::AccessReviewRequired => AgentFailure::VaultUnavailable,
                other => other,
            })
    }

    /// Review and activate one remote Calendar grant for an exact resource.
    ///
    /// A fresh review carries no expectation; an existing-grant review carries
    /// the exact GrantAuthority and consumer-policy authority the Person
    /// reviewed. The policy is evolved, never written through: an exact
    /// semantic no-op preserves it and any reviewed change advances it.
    pub async fn review_and_activate_remote_calendar_grant(
        &self,
        grant_id: GrantId,
        expected: Option<GrantAuthority>,
        source: GrantSourceBinding,
        scope: GrantScope,
        expected_policy: Option<ConsumerPolicyAuthority>,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if !grant_id.is_valid() || source.person_id() != self.person_id {
            return Err(AgentFailure::InvalidInput);
        }
        let expected_pair = match (expected, expected_policy) {
            (None, None) => None,
            (Some(authority), Some(policy))
                if authority.is_valid() && policy.is_valid() =>
            {
                Some((authority, policy))
            }
            _ => return Err(AgentFailure::InvalidInput),
        };
        if scope.resources().len() != 1 {
            return Err(AgentFailure::InvalidInput);
        }
        let resource = scope.resources()[0].as_str().to_owned();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(super::storage)?;
        let result = async {
            self.ensure_calendar_grant_policy_schema(&transaction)
                .await?;
            let grant = match expected_pair {
                None => {
                    if self
                        .find_remote_calendar_grant_in_transaction(&transaction, &source, &resource)
                        .await?
                        .is_some()
                    {
                        return Err(AgentFailure::Conflict);
                    }
                    if self
                        .maybe_calendar_grant_policy_in_transaction(&transaction, grant_id)
                        .await?
                        .is_some()
                    {
                        return Err(AgentFailure::Conflict);
                    }
                    let created = self
                        .create_data_access_grant_in_transaction(
                            &transaction,
                            grant_id,
                            source.clone(),
                            scope.clone(),
                        )
                        .await?;
                    let grant = self
                        .mutate_data_access_grant_in_transaction(
                            &transaction,
                            grant_id,
                            created.authority(),
                            AccessGrantMutation::Activate {
                                source: source.clone(),
                                scope: scope.clone(),
                            },
                        )
                        .await?;
                    let consumer_policy =
                        super::calendar_grant_policy::evolve_calendar_consumer_policy(
                            None, None, &source, &scope, None,
                        )?;
                    self.upsert_calendar_grant_policy_in_transaction(
                        &transaction,
                        &CalendarGrantPolicy {
                            grant_id,
                            person_id: self.person_id,
                            consumer_policy,
                            reviewed_native_subject_fingerprint: None,
                        },
                    )
                    .await?;
                    grant
                }
                Some((authority, policy)) => {
                    let previous = self
                        .read_data_access_grant_in_transaction(&transaction, grant_id)
                        .await?;
                    if previous.scope().resources() != scope.resources() {
                        return Err(AgentFailure::PolicyDenied);
                    }
                    let stored = self
                        .maybe_calendar_grant_policy_in_transaction(&transaction, grant_id)
                        .await?;
                    if stored.as_ref().is_some_and(|stored| {
                        stored.consumer_policy != policy
                    }) {
                        return Err(AgentFailure::Conflict);
                    }
                    let consumer_policy =
                        super::calendar_grant_policy::evolve_calendar_consumer_policy(
                            Some(&previous),
                            stored.as_ref(),
                            &source,
                            &scope,
                            None,
                        )?;
                    let grant = self
                        .mutate_data_access_grant_in_transaction(
                            &transaction,
                            grant_id,
                            authority,
                            AccessGrantMutation::Activate {
                                source: source.clone(),
                                scope: scope.clone(),
                            },
                        )
                        .await?;
                    self.upsert_calendar_grant_policy_in_transaction(
                        &transaction,
                        &CalendarGrantPolicy {
                            grant_id,
                            person_id: self.person_id,
                            consumer_policy,
                            reviewed_native_subject_fingerprint: None,
                        },
                    )
                    .await?;
                    grant
                }
            };
            self.check_access()?;
            Ok(grant)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    async fn find_remote_calendar_grant_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        source: &GrantSourceBinding,
        resource: &str,
    ) -> Result<Option<DataAccessGrant>, AgentFailure> {
        self.ensure_access_grant_schema_transaction(transaction)
            .await?;
        let mut rows = transaction
            .query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, source_incarnation, source_epoch, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE person_id = ? AND authority_owner = ? AND connection_id = ? AND connector = ? AND execution_owner = ?", (self.person_id.to_string(), self.vault_id.to_string(), source.connection_id().as_str().to_owned(), source.connector().as_str().to_owned(), source.execution_owner().as_str().to_owned()))
            .await
            .map_err(super::storage)?;
        let mut found = None;
        while let Some(row) = rows.next().await.map_err(super::storage)? {
            let grant = super::access_grants::decode_grant(&row)?;
            if !grant.source().same_identity(source)
                || grant.scope().resources().len() != 1
                || grant.scope().resources()[0].as_str() != resource
            {
                continue;
            }
            if found.is_some() {
                return Err(AgentFailure::Conflict);
            }
            found = Some(grant);
        }
        Ok(found)
    }

    pub async fn pause_remote_calendar_grant(
        &self,
        grant_id: GrantId,
        expected: GrantAuthority,
    ) -> Result<DataAccessGrant, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(super::storage)?;
        let result = self
            .mutate_data_access_grant_in_transaction(
                &transaction,
                grant_id,
                expected,
                AccessGrantMutation::Pause,
            )
            .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, os::unix::fs::PermissionsExt, sync::Mutex};

    use floe_access::{
        ConnectionId, ConnectorId, ExecutionOwnerId, GrantConsumer, GrantDataCategory,
        GrantOperation, GrantPurpose, GrantState, ProcessingRestriction, ResourceHandle,
        SourceAuthority,
    };

    use super::*;

    #[derive(Clone, Default)]
    struct TestKeys(
        std::sync::Arc<Mutex<HashMap<(floe_kernel::PersonId, uuid::Uuid), [u8; 32]>>>,
    );

    impl VaultKeyProvider for TestKeys {
        fn load(
            &self,
            person_id: floe_kernel::PersonId,
            vault_id: uuid::Uuid,
        ) -> Result<crate::VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .get(&(person_id, vault_id))
                .copied()
                .map(crate::VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: floe_kernel::PersonId,
            vault_id: uuid::Uuid,
            key: &crate::VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    const CONNECTOR: &str = "calendar.google";
    const CONNECTION: &str = "remote-connection";
    const OWNER: &str = "server-owner";

    fn consumers() -> Vec<GrantConsumer> {
        vec![GrantConsumer::builtin("floe.builtin.schedule").unwrap()]
    }

    fn source(
        person: floe_kernel::PersonId,
        authority: SourceAuthority,
    ) -> GrantSourceBinding {
        GrantSourceBinding::try_new(
            person,
            ConnectionId::try_new(CONNECTION).unwrap(),
            ConnectorId::try_new(CONNECTOR).unwrap(),
            ExecutionOwnerId::try_new(OWNER).unwrap(),
            authority,
        )
        .unwrap()
    }

    fn scope(resource: &str, consumers: &[GrantConsumer]) -> GrantScope {
        GrantScope::try_new(
            vec![ResourceHandle::try_new(resource).unwrap()],
            vec![GrantDataCategory::Content],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            consumers.to_vec(),
            ProcessingRestriction::LocalOnly,
        )
        .unwrap()
    }

    async fn vault() -> (
        tempfile::TempDir,
        EncryptedAgentVault<TestKeys>,
        floe_kernel::PersonId,
    ) {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person = floe_kernel::PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person, TestKeys::default())
            .await
            .unwrap();
        (root, vault, person)
    }

    async fn fresh_review(
        vault: &EncryptedAgentVault<TestKeys>,
        person: floe_kernel::PersonId,
        authority: SourceAuthority,
        resource: &str,
    ) -> DataAccessGrant {
        vault
            .review_and_activate_remote_calendar_grant(
                GrantId::new(),
                None,
                source(person, authority),
                scope(resource, &consumers()),
                None,
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn sibling_resources_bind_independently() {
        let (_root, vault, person) = vault().await;
        let authority = SourceAuthority::new();
        let grant_a = fresh_review(&vault, person, authority, "calendar-a").await;
        let grant_b = fresh_review(&vault, person, authority, "calendar-b").await;
        assert_ne!(grant_a.id(), grant_b.id());
        let binding_a = vault
            .remote_calendar_grant_binding(CONNECTOR, CONNECTION, authority, "calendar-a")
            .await
            .unwrap();
        assert_eq!(binding_a.grant.id(), grant_a.id());
        let binding_b = vault
            .remote_calendar_grant_binding(CONNECTOR, CONNECTION, authority, "calendar-b")
            .await
            .unwrap();
        assert_eq!(binding_b.grant.id(), grant_b.id());
        assert_ne!(binding_a.consumer_policy, binding_b.consumer_policy);
        let found = vault
            .find_remote_calendar_grant(&source(person, authority), "calendar-a")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id(), grant_a.id());
    }

    #[tokio::test]
    async fn duplicate_exact_resource_grants_conflict() {
        let (_root, vault, person) = vault().await;
        let authority = SourceAuthority::new();
        let first = fresh_review(&vault, person, authority, "primary").await;
        assert_eq!(first.state(), GrantState::Active);
        let duplicate = vault
            .create_data_access_grant_with_id(
                GrantId::new(),
                source(person, authority),
                scope("primary", &consumers()),
            )
            .await
            .unwrap();
        vault
            .activate_data_access_grant(
                duplicate.id(),
                duplicate.authority(),
                source(person, authority),
                scope("primary", &consumers()),
            )
            .await
            .unwrap();
        assert_eq!(
            vault
                .remote_calendar_grant_binding(CONNECTOR, CONNECTION, authority, "primary")
                .await,
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            vault
                .find_remote_calendar_grant(&source(person, authority), "primary")
                .await,
            Err(AgentFailure::Conflict)
        );
    }

    #[tokio::test]
    async fn repeated_review_updates_the_same_grant() {
        let (_root, vault, person) = vault().await;
        let authority = SourceAuthority::new();
        let first = fresh_review(&vault, person, authority, "primary").await;
        let policy = vault.calendar_grant_policy_authority(first.id()).await.unwrap();
        let second = vault
            .review_and_activate_remote_calendar_grant(
                first.id(),
                Some(first.authority()),
                source(person, authority),
                scope("primary", &consumers()),
                Some(policy),
            )
            .await
            .unwrap();
        assert_eq!(second.id(), first.id());
        assert_eq!(second.authority(), first.authority());
        assert_eq!(
            vault.calendar_grant_policy_authority(first.id()).await.unwrap(),
            policy
        );
        let grants = vault.list_data_access_grants(128).await.unwrap();
        assert_eq!(grants.len(), 1);
    }

    #[tokio::test]
    async fn fresh_review_conflicts_when_the_grant_already_exists() {
        let (_root, vault, person) = vault().await;
        let authority = SourceAuthority::new();
        fresh_review(&vault, person, authority, "primary").await;
        assert_eq!(
            vault
                .review_and_activate_remote_calendar_grant(
                    GrantId::new(),
                    None,
                    source(person, authority),
                    scope("primary", &consumers()),
                    None,
                )
                .await,
            Err(AgentFailure::Conflict)
        );
    }

    #[tokio::test]
    async fn stale_grant_and_policy_expectations_conflict() {
        let (_root, vault, person) = vault().await;
        let authority = SourceAuthority::new();
        let grant = fresh_review(&vault, person, authority, "primary").await;
        let policy = vault.calendar_grant_policy_authority(grant.id()).await.unwrap();
        let paused = vault
            .pause_remote_calendar_grant(grant.id(), grant.authority())
            .await
            .unwrap();
        assert_eq!(
            vault
                .review_and_activate_remote_calendar_grant(
                    grant.id(),
                    Some(grant.authority()),
                    source(person, authority),
                    scope("primary", &consumers()),
                    Some(policy),
                )
                .await,
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            vault
                .review_and_activate_remote_calendar_grant(
                    grant.id(),
                    Some(paused.authority()),
                    source(person, authority),
                    scope("primary", &consumers()),
                    Some(ConsumerPolicyAuthority::new()),
                )
                .await,
            Err(AgentFailure::Conflict)
        );
        let reactivated = vault
            .review_and_activate_remote_calendar_grant(
                paused.id(),
                Some(paused.authority()),
                source(person, authority),
                scope("primary", &consumers()),
                Some(policy),
            )
            .await
            .unwrap();
        assert_eq!(reactivated.id(), grant.id());
        assert_eq!(reactivated.state(), GrantState::Active);
    }

    #[tokio::test]
    async fn review_rejects_a_sibling_resource_grant_id() {
        let (_root, vault, person) = vault().await;
        let authority = SourceAuthority::new();
        let grant_a = fresh_review(&vault, person, authority, "calendar-a").await;
        let grant_b = fresh_review(&vault, person, authority, "calendar-b").await;
        let policy_b = vault.calendar_grant_policy_authority(grant_b.id()).await.unwrap();
        assert_eq!(
            vault
                .review_and_activate_remote_calendar_grant(
                    grant_b.id(),
                    Some(grant_b.authority()),
                    source(person, authority),
                    scope("calendar-a", &consumers()),
                    Some(policy_b),
                )
                .await,
            Err(AgentFailure::PolicyDenied)
        );
        assert_eq!(
            vault
                .remote_calendar_grant_binding(CONNECTOR, CONNECTION, authority, "calendar-a")
                .await
                .unwrap()
                .grant
                .id(),
            grant_a.id()
        );
    }

    #[tokio::test]
    async fn authority_rotation_updates_the_same_grant_and_advances_policy() {
        let (_root, vault, person) = vault().await;
        let first_authority = SourceAuthority::new();
        let grant = fresh_review(&vault, person, first_authority, "primary").await;
        let policy = vault.calendar_grant_policy_authority(grant.id()).await.unwrap();
        let rotated_authority = SourceAuthority::new();
        let rotated = vault
            .review_and_activate_remote_calendar_grant(
                grant.id(),
                Some(grant.authority()),
                source(person, rotated_authority),
                scope("primary", &consumers()),
                Some(policy),
            )
            .await
            .unwrap();
        assert_eq!(rotated.id(), grant.id());
        assert_ne!(rotated.authority(), grant.authority());
        assert_eq!(rotated.source().source_authority(), rotated_authority);
        let advanced = vault.calendar_grant_policy_authority(grant.id()).await.unwrap();
        assert_ne!(advanced, policy);
        let binding = vault
            .remote_calendar_grant_binding(CONNECTOR, CONNECTION, rotated_authority, "primary")
            .await
            .unwrap();
        assert_eq!(binding.grant.id(), grant.id());
        assert_eq!(binding.consumer_policy, advanced);
    }

    #[tokio::test]
    async fn consumer_change_advances_remote_policy() {
        let (_root, vault, person) = vault().await;
        let authority = SourceAuthority::new();
        let grant = fresh_review(&vault, person, authority, "primary").await;
        let policy = vault.calendar_grant_policy_authority(grant.id()).await.unwrap();
        let extended = vec![
            GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
            GrantConsumer::builtin("floe.builtin.commitments").unwrap(),
        ];
        let updated = vault
            .review_and_activate_remote_calendar_grant(
                grant.id(),
                Some(grant.authority()),
                source(person, authority),
                scope("primary", &extended),
                Some(policy),
            )
            .await
            .unwrap();
        assert_ne!(updated.authority(), grant.authority());
        assert_ne!(
            vault.calendar_grant_policy_authority(grant.id()).await.unwrap(),
            policy
        );
    }

    #[tokio::test]
    async fn missing_remote_policy_fails_closed() {
        let (_root, vault, person) = vault().await;
        let authority = SourceAuthority::new();
        let grant = fresh_review(&vault, person, authority, "primary").await;
        let policy = vault.calendar_grant_policy_authority(grant.id()).await.unwrap();
        let raw = vault.database.connect().unwrap();
        raw.execute(
            "DELETE FROM calendar_grant_policies WHERE grant_id = ?",
            [grant.id().as_uuid().to_string()],
        )
        .await
        .unwrap();
        drop(raw);
        assert_eq!(
            vault
                .review_and_activate_remote_calendar_grant(
                    grant.id(),
                    Some(grant.authority()),
                    source(person, authority),
                    scope("primary", &consumers()),
                    Some(policy),
                )
                .await,
            Err(AgentFailure::VaultUnavailable)
        );
    }

    #[tokio::test]
    async fn sibling_grants_and_policies_survive_reopen() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let authority = SourceAuthority::new();
        let (policy_a, policy_b) = {
            let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
                .await
                .unwrap();
            let grant_a = fresh_review(&vault, person, authority, "calendar-a").await;
            let grant_b = fresh_review(&vault, person, authority, "calendar-b").await;
            (
                vault.calendar_grant_policy_authority(grant_a.id()).await.unwrap(),
                vault.calendar_grant_policy_authority(grant_b.id()).await.unwrap(),
            )
        };
        let vault = EncryptedAgentVault::open(root.path(), person, keys)
            .await
            .unwrap();
        let binding_a = vault
            .remote_calendar_grant_binding(CONNECTOR, CONNECTION, authority, "calendar-a")
            .await
            .unwrap();
        assert_eq!(binding_a.consumer_policy, policy_a);
        let binding_b = vault
            .remote_calendar_grant_binding(CONNECTOR, CONNECTION, authority, "calendar-b")
            .await
            .unwrap();
        assert_eq!(binding_b.consumer_policy, policy_b);
    }

    #[tokio::test]
    async fn mixed_remote_expectation_halves_are_rejected() {
        let (_root, vault, person) = vault().await;
        let authority = SourceAuthority::new();
        let grant = fresh_review(&vault, person, authority, "primary").await;
        let policy = vault.calendar_grant_policy_authority(grant.id()).await.unwrap();
        for (expected, expected_policy) in [
            (Some(grant.authority()), None),
            (None, Some(policy)),
        ] {
            assert_eq!(
                vault
                    .review_and_activate_remote_calendar_grant(
                        grant.id(),
                        expected,
                        source(person, authority),
                        scope("primary", &consumers()),
                        expected_policy,
                    )
                    .await,
                Err(AgentFailure::InvalidInput)
            );
        }
    }
}
