use super::*;
use floe_access::{
    ConnectionReview, ExpectedGrant, GrantAbort, GrantAbortOutcome, GrantAbortReceipt, GrantCommit,
    GrantCommitKind, GrantCommitReceipt, GrantMutation, GrantOperationIdentity,
    GrantOperationReceipt, GrantReceiptQuery, GrantRepository, GrantResult, GrantSnapshot,
    ReviewRef, SourceReservationEvidence, validate_commit,
};
use floe_context_contract::GrantSourceBinding;
use floe_execution::BoxFuture;
use turso::transaction::{Transaction, TransactionBehavior};

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn initialize_connection_reviews(
        &self,
        create: bool,
    ) -> Result<(), AgentFailure> {
        let connection = self.connection()?;
        if create {
            connection.execute("CREATE TABLE access_connection_reviews (review_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, command_id TEXT NOT NULL, intent_digest TEXT NOT NULL, payload TEXT NOT NULL, UNIQUE(person_id, command_id))", ()).await.map_err(storage)?;
            connection.execute("CREATE TABLE access_grant_operations (operation_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)", ()).await.map_err(storage)?;
        }
        // Missing tables on open are unsupported stored meaning, never an implicit migration.
        connection
            .query(
                "SELECT review_id FROM access_connection_reviews LIMIT 1",
                (),
            )
            .await
            .map_err(|_| AgentFailure::UnsupportedVersion)?;
        connection
            .query(
                "SELECT operation_id FROM access_grant_operations LIMIT 1",
                (),
            )
            .await
            .map_err(|_| AgentFailure::UnsupportedVersion)?;
        Ok(())
    }
    async fn operation_on(
        &self,
        transaction: &Transaction<'_>,
        expected: &GrantOperationIdentity,
    ) -> Result<Option<GrantOperationReceipt>, AgentFailure> {
        expected.validate()?;
        if expected.source.person_id() != self.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut rows = transaction.query("SELECT payload FROM access_grant_operations WHERE operation_id = ? AND person_id = ?", (expected.operation_id.to_string(), self.person_id.to_string())).await.map_err(storage)?;
        let Some(row) = rows.next().await.map_err(storage)? else {
            return Ok(None);
        };
        let receipt: GrantOperationReceipt = decode(row.get::<String>(0).map_err(storage)?)?;
        receipt
            .validate()
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let identity = match &receipt {
            GrantOperationReceipt::Committed(receipt) => receipt.reservation.identity(),
            GrantOperationReceipt::Aborted(receipt) => receipt.identity.clone(),
        };
        if &identity != expected {
            return Err(AgentFailure::Conflict);
        }
        Ok(Some(receipt))
    }
    async fn insert_operation_on(
        &self,
        transaction: &Transaction<'_>,
        identity: &GrantOperationIdentity,
        receipt: &GrantOperationReceipt,
    ) -> Result<(), AgentFailure> {
        transaction.execute("INSERT INTO access_grant_operations (operation_id, person_id, payload) VALUES (?, ?, ?)", (identity.operation_id.to_string(), self.person_id.to_string(), encode(receipt)?)).await.map_err(storage)?;
        Ok(())
    }
}
impl<Keys: VaultKeyProvider> GrantRepository for EncryptedAgentVault<Keys> {
    fn find_review<'a>(
        &'a self,
        person_id: PersonId,
        command_id: Uuid,
        intent_digest: [u8; 32],
    ) -> BoxFuture<'a, Result<Option<ConnectionReview>, AgentFailure>> {
        Box::pin(async move {
            if person_id != self.person_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let connection = self.connection()?;
            let mut rows = connection.query("SELECT payload FROM access_connection_reviews WHERE person_id = ? AND command_id = ?", (person_id.to_string(), command_id.to_string())).await.map_err(storage)?;
            let Some(row) = rows.next().await.map_err(storage)? else {
                return Ok(None);
            };
            let review: ConnectionReview = decode(row.get::<String>(0).map_err(storage)?)?;
            review.validate()?;
            if review.person_id != person_id
                || review.command_id != command_id
                || review.intent_digest != intent_digest
            {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            Ok(Some(review))
        })
    }
    fn read_review<'a>(
        &'a self,
        reference: ReviewRef,
    ) -> BoxFuture<'a, Result<ConnectionReview, AgentFailure>> {
        Box::pin(async move {
            reference.validate()?;
            let connection = self.connection()?;
            let mut rows = connection.query("SELECT payload FROM access_connection_reviews WHERE review_id = ? AND person_id = ?", (reference.id.to_string(), self.person_id.to_string())).await.map_err(storage)?;
            let row = rows
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::NotFound)?;
            let review: ConnectionReview = decode(row.get::<String>(0).map_err(storage)?)?;
            review.validate()?;
            if review.reference != reference || review.person_id != self.person_id {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            Ok(review)
        })
    }
    fn store_review<'a>(
        &'a self,
        review: ConnectionReview,
    ) -> BoxFuture<'a, Result<ReviewRef, AgentFailure>> {
        Box::pin(async move {
            review.validate()?;
            if review.person_id != self.person_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let mut connection = self.connection()?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(storage)?;
            let result = async {
                let mut rows = transaction.query("SELECT payload FROM access_connection_reviews WHERE person_id = ? AND command_id = ?", (self.person_id.to_string(), review.command_id.to_string())).await.map_err(storage)?;
                if let Some(row) = rows.next().await.map_err(storage)? {
                    let existing: ConnectionReview = decode(row.get::<String>(0).map_err(storage)?)?;
                    existing.validate()?;
                    if existing.intent_digest != review.intent_digest { return Err(AgentFailure::Conflict); }
                    return Ok(existing.reference);
                }
                transaction.execute("INSERT INTO access_connection_reviews (review_id, person_id, command_id, intent_digest, payload) VALUES (?, ?, ?, ?, ?)", (review.reference.id.to_string(), self.person_id.to_string(), review.command_id.to_string(), hex(&review.intent_digest), encode(&review)?)).await.map_err(storage)?;
                self.check_access()?;
                Ok(review.reference)
            }.await;
            self.finish_access_grant_transaction(transaction, result)
                .await
        })
    }
    fn snapshot<'a>(
        &'a self,
        source: GrantSourceBinding,
    ) -> BoxFuture<'a, Result<GrantSnapshot, AgentFailure>> {
        Box::pin(async move {
            Ok(GrantSnapshot {
                authority_owner: self.vault_id,
                grants: self.data_access_grants_for_source(&source, 128).await?,
                source,
            })
        })
    }
    fn receipt<'a>(
        &'a self,
        query: GrantReceiptQuery,
    ) -> BoxFuture<'a, Result<Option<GrantOperationReceipt>, AgentFailure>> {
        Box::pin(async move {
            let mut connection = self.connection()?;
            let transaction = connection.transaction().await.map_err(storage)?;
            let result = self.operation_on(&transaction, &query.identity).await;
            self.finish_access_grant_transaction(transaction, result)
                .await
        })
    }
    fn abort<'a>(
        &'a self,
        command: GrantAbort,
    ) -> BoxFuture<'a, Result<GrantAbortOutcome, AgentFailure>> {
        Box::pin(async move {
            let mut connection = self.connection()?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(storage)?;
            let result = async {
                match self.operation_on(&transaction, &command.identity).await? {
                    Some(GrantOperationReceipt::Committed(receipt)) => {
                        return Ok(GrantAbortOutcome::AlreadyCommitted(receipt));
                    }
                    Some(GrantOperationReceipt::Aborted(receipt)) => {
                        return Ok(GrantAbortOutcome::Aborted(receipt));
                    }
                    None => {}
                }
                let receipt = GrantAbortReceipt {
                    identity: command.identity.clone(),
                    abort_id: Uuid::new_v4(),
                };
                self.insert_operation_on(
                    &transaction,
                    &command.identity,
                    &GrantOperationReceipt::Aborted(receipt.clone()),
                )
                .await?;
                self.check_access()?;
                Ok(GrantAbortOutcome::Aborted(receipt))
            }
            .await;
            self.finish_access_grant_transaction(transaction, result)
                .await
        })
    }
    fn commit<'a>(
        &'a self,
        command: GrantCommit,
    ) -> BoxFuture<'a, Result<GrantCommitReceipt, AgentFailure>> {
        Box::pin(async move {
            let mut connection = self.connection()?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(storage)?;
            let result = async {
                match self.operation_on(&transaction, &command.reservation.identity()).await? {
                    Some(GrantOperationReceipt::Committed(receipt)) if receipt.kind == command.kind && receipt.commit_digest == floe_access::digest(&command)? => return Ok(receipt),
                    Some(_) => return Err(AgentFailure::Conflict), None => {},
                }
                let review = if let GrantCommitKind::Reviewed { review } = &command.kind {
                    let mut rows = transaction.query("SELECT payload FROM access_connection_reviews WHERE review_id = ? AND person_id = ?", (review.id.to_string(), self.person_id.to_string())).await.map_err(storage)?;
                    let row = rows.next().await.map_err(storage)?.ok_or(AgentFailure::NotFound)?;
                    Some(decode::<ConnectionReview>(row.get::<String>(0).map_err(storage)?)?)
                } else { None };
                let snapshot = GrantSnapshot { authority_owner: self.vault_id, source: command.reservation.source.source.clone(), grants: self.data_access_grants_for_source_in_transaction(&transaction, &command.reservation.source.source, 128).await? };
                validate_commit(&command, review.as_ref(), &snapshot, chrono::Utc::now())?;
                let mut cleanup_ids = Vec::new();
                for mutation in &command.mutations {
                    if let Some(cleanup_id) = self.write_reviewed_grant_on(&transaction, mutation, &snapshot.grants).await? { cleanup_ids.push(cleanup_id); }
                }
                let commit_digest = floe_access::digest(&command)?;
                let receipt = GrantCommitReceipt { commit_digest, reservation: command.reservation.clone(), kind: command.kind, commit_id: Uuid::new_v4(),
                    grants: command.mutations.iter().map(|mutation| GrantResult { grant_id: mutation.successor.id(), authority: mutation.successor.authority(), state: mutation.successor.state() }).collect(), cleanup_ids };
                self.insert_operation_on(&transaction, &command.reservation.identity(), &GrantOperationReceipt::Committed(receipt.clone())).await?;
                self.check_access()?;
                Ok(receipt)
            }.await;
            self.finish_access_grant_transaction(transaction, result)
                .await
        })
    }
}
fn encode(value: &impl serde::Serialize) -> Result<String, AgentFailure> {
    let payload = serde_json::to_string(value).map_err(|_| AgentFailure::StorageUnavailable)?;
    if payload.len() > 256 * 1024 {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload)
}
fn decode<T: serde::de::DeserializeOwned>(payload: String) -> Result<T, AgentFailure> {
    if payload.len() > 256 * 1024 {
        return Err(AgentFailure::VaultUnavailable);
    }
    serde_json::from_str(&payload).map_err(|_| AgentFailure::VaultUnavailable)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
