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
        // Exactly one grant for this connection identity, or no read. Remote
        // execution owners vary by deployment; the connection triple plus the
        // grant's own source record identify the binding.
        let grants = self.list_data_access_grants(128).await?;
        let mut found = None;
        for grant in &grants {
            let source = grant.source();
            if source.connector().as_str() == connector
                && source.connection_id().as_str() == connection_id
                && source.source_authority() == source_authority
            {
                if found.is_some() {
                    return Err(AgentFailure::Conflict);
                }
                found = Some(grant.clone());
            }
        }
        let grant = found.ok_or(AgentFailure::AccessReviewRequired)?;
        if !grant
            .scope()
            .resources()
            .iter()
            .any(|candidate| candidate.as_str() == resource)
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
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
        let creating_policy = expected_policy.is_none();
        let policy = expected_policy.unwrap_or_default();
        if !policy.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(super::storage)?;
        let result = async {
            self.ensure_calendar_grant_policy_schema(&transaction)
                .await?;
            if creating_policy {
                if self
                    .calendar_grant_policy(grant_id)
                    .await
                    .is_ok()
                {
                    return Err(AgentFailure::Conflict);
                }
            } else {
                let stored = self
                    .calendar_grant_policy(grant_id)
                    .await
                    .map_err(|_| AgentFailure::NotFound)?;
                if stored.consumer_policy != policy {
                    return Err(AgentFailure::Conflict);
                }
            }
            let grant = match expected {
                Some(authority) => {
                    self.mutate_data_access_grant_in_transaction(
                        &transaction,
                        grant_id,
                        authority,
                        AccessGrantMutation::Activate {
                            source: source.clone(),
                            scope: scope.clone(),
                        },
                    )
                    .await?
                }
                None => {
                    let existing = self
                        .create_data_access_grant_in_transaction(
                            &transaction,
                            grant_id,
                            source.clone(),
                            scope.clone(),
                        )
                        .await?;
                    self.mutate_data_access_grant_in_transaction(
                        &transaction,
                        grant_id,
                        existing.authority(),
                        AccessGrantMutation::Activate {
                            source: source.clone(),
                            scope: scope.clone(),
                        },
                    )
                    .await?
                }
            };
            self.upsert_calendar_grant_policy_in_transaction(
                &transaction,
                &CalendarGrantPolicy {
                    grant_id,
                    person_id: self.person_id,
                    consumer_policy: policy,
                    reviewed_native_subject_fingerprint: None,
                },
            )
            .await?;
            Ok(grant)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
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
