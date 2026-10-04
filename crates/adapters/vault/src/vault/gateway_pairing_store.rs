//! Atomic storage of Connections-owned pairing state and its private material.
//! Only the root encryption key lives in the platform Keychain.
use super::gateway_authority::{
    bounded_decode, bounded_encode, pairing_storage, validate_producer,
};
use super::{EncryptedAgentVault, VaultKeyProvider, storage};
use floe_access::GatewayCredentialExpectation;
use floe_connections::*;
use floe_execution::BoxFuture;
use floe_kernel::{AgentFailure, PersonId};
use sha2::Digest;
use turso::{Connection, transaction::TransactionBehavior};
use uuid::Uuid;
use zeroize::Zeroizing;

struct PrivateRow {
    proof: PairingProof,
    enrollment: Option<EnrollmentSigningCommand>,
    credential: Option<GatewayCredentialMaterial>,
}

pub(super) fn pairing_state(state: PairingState) -> &'static str {
    match state {
        PairingState::Pending => "pending",
        PairingState::AwaitingLocalConfirmation => "awaiting_local_confirmation",
        PairingState::AwaitingApproval => "awaiting_approval",
        PairingState::Cancelling => "cancelling",
        PairingState::Paired => "paired",
        PairingState::Rejected => "rejected",
        PairingState::Expired => "expired",
        PairingState::Cancelled => "cancelled",
        PairingState::RepairRequired => "repair_required",
        PairingState::RevocationPending => "revocation_pending",
        PairingState::Forgotten => "forgotten",
    }
}

async fn expectation_on(
    connection: &Connection,
) -> Result<GatewayCredentialExpectation, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT payload FROM gateway_credential_expectation WHERE id=1",
            (),
        )
        .await
        .map_err(storage)?;
    bounded_decode(
        &rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?
            .get::<String>(0)
            .map_err(storage)?,
    )
}
async fn set_expectation_on(
    connection: &Connection,
    expectation: &GatewayCredentialExpectation,
) -> Result<(), AgentFailure> {
    if connection
        .execute(
            "UPDATE gateway_credential_expectation SET payload=? WHERE id=1",
            (bounded_encode(expectation)?,),
        )
        .await
        .map_err(storage)?
        != 1
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}
async fn pairing_on(
    connection: &Connection,
    person: PersonId,
    id: Uuid,
) -> Result<Option<PairingRecord>, AgentFailure> {
    let mut rows = connection.query(
        "SELECT revision,state,payload FROM gateway_pairing_operations WHERE operation_id=? AND person_id=?",
        (id.to_string(), person.to_string()),
    ).await.map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    let record: PairingRecord = bounded_decode(&row.get::<String>(2).map_err(storage)?)?;
    record
        .validate()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    if record.person_id != person
        || record.operation_id != id
        || row.get::<i64>(0).map_err(storage)? <= 0
        || row.get::<i64>(0).map_err(storage)? as u64 != record.revision
        || row.get::<String>(1).map_err(storage)? != pairing_state(record.state)
        || rows.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(record))
}
async fn write_pairing_on(
    connection: &Connection,
    expected: u64,
    record: &PairingRecord,
) -> Result<(), AgentFailure> {
    record.validate().map_err(|_| AgentFailure::Conflict)?;
    if record.revision != expected.checked_add(1).ok_or(AgentFailure::Conflict)?
        || record.revision > i64::MAX as u64
    {
        return Err(AgentFailure::Conflict);
    }
    let changed = connection.execute(
        "UPDATE gateway_pairing_operations SET revision=?,state=?,payload=? WHERE operation_id=? AND person_id=? AND revision=?",
        (record.revision as i64, pairing_state(record.state), bounded_encode(record)?,
            record.operation_id.to_string(), record.person_id.to_string(), expected as i64),
    ).await.map_err(storage)?;
    if changed != 1 {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}
async fn private_on(
    connection: &Connection,
    operation: &PairingRecord,
) -> Result<PrivateRow, AgentFailure> {
    let mut rows = connection.query(
        "SELECT proof,enrollment_json,credential FROM gateway_pairing_private WHERE operation_id=?",
        (operation.operation_id.to_string(),),
    ).await.map_err(storage)?;
    let row = rows
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::VaultUnavailable)?;
    let bytes = Zeroizing::new(row.get::<Vec<u8>>(0).map_err(storage)?);
    let proof: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let enrollment: Option<EnrollmentSigningCommand> = row
        .get::<Option<String>>(1)
        .map_err(storage)?
        .map(|json| bounded_decode(&json))
        .transpose()?;
    let credential = row
        .get::<Option<Vec<u8>>>(2)
        .map_err(storage)?
        .map(GatewayCredentialMaterial::new)
        .transpose()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    if rows.next().await.map_err(storage)?.is_some()
        || enrollment.as_ref().is_some_and(|command| {
            command.operation_id != operation.operation_id
                || command.person_id != operation.person_id
                || command.device_id != operation.device_id
        })
        || (operation.handle.is_some() != enrollment.is_some())
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(PrivateRow {
        proof: PairingProof::new(proof),
        enrollment,
        credential,
    })
}
async fn pin_on(connection: &Connection) -> Result<Option<GatewayPin>, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT identity_json,revision FROM remote_authority_producer WHERE id=1",
            (),
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    let producer: floe_access::RemoteProducerIdentity =
        bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
    let revision = row.get::<i64>(1).map_err(storage)?;
    validate_producer(&producer)?;
    if revision <= 0 || rows.next().await.map_err(storage)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(GatewayPin {
        producer,
        revision: revision as u64,
    }))
}
async fn validate_forgotten_on(
    connection: &Connection,
    person: PersonId,
    device: &str,
    operation_id: Uuid,
    generation: u64,
) -> Result<(), AgentFailure> {
    let operation = pairing_on(connection, person, operation_id)
        .await?
        .ok_or(AgentFailure::VaultUnavailable)?;
    let command = operation
        .forgotten_command
        .ok_or(AgentFailure::PolicyDenied)?;
    if operation.device_id != device || operation.generation != generation {
        return Err(AgentFailure::PolicyDenied);
    }
    let mut rows = connection
        .query(
            "SELECT payload FROM connections_product_records WHERE person_id=? AND command_id=?",
            (person.to_string(), command.to_string()),
        )
        .await
        .map_err(storage)?;
    let receipt: ConnectionsRecord = bounded_decode(
        &rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?
            .get::<String>(0)
            .map_err(storage)?,
    )?;
    receipt.validate()?;
    if receipt.person_id != person
        || receipt.device_id != device
        || receipt.command_id != command
        || !matches!(receipt.payload, ConnectionsPayload::GatewayForgotten(ref summary)
            if summary.gateway_ref == operation_id && summary.state == GatewayState::Forgotten)
        || rows.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn pending_for(expectation: &GatewayCredentialExpectation, operation: Uuid) -> bool {
    *expectation
        == GatewayCredentialExpectation::Pending {
            operation_id: operation,
        }
}

async fn execute_pairing_command_on(
    tx: &Connection,
    command: &PairingMutation,
) -> Result<Result<PairingSnapshot, PairingError>, AgentFailure> {
    match &command.action {
        PairingMutationAction::Start {
            operation_id,
            setup,
        } => {
            if operation_id.is_nil() || setup.validate().is_err() {
                return Ok(Err(PairingError::InvalidInput));
            }
            if pairing_on(tx, command.person_id, *operation_id)
                .await?
                .is_some()
            {
                // Successful admission and its command receipt are indivisible.
                return Err(AgentFailure::VaultUnavailable);
            }
            let mut rows = tx
                .query(
                    "SELECT payload FROM gateway_setup_receipts WHERE target_ref=? AND person_id=?",
                    (setup.target_ref.to_string(), command.person_id.to_string()),
                )
                .await
                .map_err(storage)?;
            let Some(row) = rows.next().await.map_err(storage)? else {
                return Ok(Err(PairingError::InvalidInput));
            };
            let prepared: GatewaySetupRecord =
                bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
            drop(rows);
            if prepared.setup != *setup
                || prepared.device_id != command.device_id
                || prepared.person_id != command.person_id
            {
                return Ok(Err(PairingError::ForeignIdentity));
            }
            if setup.expires_at <= chrono::Utc::now() {
                return Ok(Err(PairingError::Expired));
            }
            let prior = expectation_on(tx).await?;
            let mut generations = tx
                .query(
                    "SELECT generation FROM gateway_pairing_generation WHERE id=1",
                    (),
                )
                .await
                .map_err(storage)?;
            let high_water = generations
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?
                .get::<i64>(0)
                .map_err(storage)?;
            drop(generations);
            let Some(generation) = high_water.checked_add(1).filter(|value| *value > 0) else {
                return Ok(Err(PairingError::Conflict));
            };
            let record = match PairingRecord::admit(
                PairingAdmission {
                    operation_id: *operation_id,
                    command_id: command.command_id,
                    person_id: command.person_id,
                    device_id: command.device_id.clone(),
                    setup: setup.clone(),
                },
                prior,
                generation as u64,
            ) {
                Ok(record) => record,
                Err(error) => return Ok(Err(error)),
            };
            let mut proof = Zeroizing::new([0u8; 32]);
            getrandom::fill(proof.as_mut()).map_err(|_| AgentFailure::StorageUnavailable)?;
            tx.execute(
                "UPDATE gateway_pairing_generation SET generation=? WHERE id=1",
                (generation,),
            )
            .await
            .map_err(storage)?;
            tx.execute(
                "INSERT INTO gateway_pairing_operations VALUES(?,?,?,?,?,?)",
                (
                    record.operation_id.to_string(),
                    record.person_id.to_string(),
                    record.command_id.to_string(),
                    record.revision as i64,
                    pairing_state(record.state),
                    bounded_encode(&record)?,
                ),
            )
            .await
            .map_err(storage)?;
            tx.execute(
                "INSERT INTO gateway_pairing_private(operation_id,proof) VALUES(?,?)",
                (record.operation_id.to_string(), proof.as_slice()),
            )
            .await
            .map_err(storage)?;
            set_expectation_on(
                tx,
                &GatewayCredentialExpectation::Pending {
                    operation_id: record.operation_id,
                },
            )
            .await?;
            Ok(Ok(record.snapshot()))
        }
        PairingMutationAction::Confirm { operation_id, .. }
        | PairingMutationAction::Cancel { operation_id, .. } => {
            let Some(current) = pairing_on(tx, command.person_id, *operation_id).await? else {
                return Ok(Err(PairingError::InvalidInput));
            };
            if !pending_for(&expectation_on(tx).await?, *operation_id) {
                return Ok(Err(PairingError::Conflict));
            }
            let next = match current.decide_command(command) {
                Ok(next) => next,
                Err(error) => return Ok(Err(error)),
            };
            if next.state == PairingState::Cancelled
                && next.start_phase == PairingStartPhase::Staged
            {
                set_expectation_on(tx, &current.prior_credential_expectation).await?;
            }
            write_pairing_on(tx, current.revision, &next).await?;
            Ok(Ok(next.snapshot()))
        }
    }
}

impl<K: VaultKeyProvider> PairingRepository for EncryptedAgentVault<K> {
    fn setup<'a>(
        &'a self,
        target: Uuid,
    ) -> BoxFuture<'a, Result<Option<GatewaySetupRecord>, PairingError>> {
        Box::pin(async move {
            let mut rows = self
                .connection()
                .map_err(pairing_storage)?
                .query(
                    "SELECT payload FROM gateway_setup_receipts WHERE target_ref=? AND person_id=?",
                    (target.to_string(), self.person_id.to_string()),
                )
                .await
                .map_err(pairing_database)?;
            let record: Option<GatewaySetupRecord> = rows
                .next()
                .await
                .map_err(|_| PairingError::StorageUnavailable)?
                .map(|row| bounded_decode(&row.get::<String>(0).map_err(storage)?))
                .transpose()
                .map_err(pairing_storage)?;
            if let Some(record) = &record {
                record.setup.validate()?;
                if record.setup.target_ref != target || record.person_id != self.person_id {
                    return Err(PairingError::ForeignIdentity);
                }
            }
            Ok(record)
        })
    }
    fn store_setup<'a>(
        &'a self,
        record: GatewaySetupRecord,
    ) -> BoxFuture<'a, Result<GatewaySetupRecord, PairingError>> {
        Box::pin(async move {
            record.setup.validate()?;
            if record.person_id != self.person_id
                || record.command_id.is_nil()
                || record.device_id.is_empty()
                || record.device_id.len() > 256
            {
                return Err(PairingError::ForeignIdentity);
            }
            let mut connection = self.connection().map_err(pairing_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(pairing_database)?;
            let result = async {
                let mut rows = tx.query("SELECT payload FROM gateway_setup_receipts WHERE target_ref=? OR (person_id=? AND command_id=?)",
                    (record.setup.target_ref.to_string(), record.person_id.to_string(), record.command_id.to_string())).await.map_err(storage)?;
                if let Some(row) = rows.next().await.map_err(storage)? {
                    let current: GatewaySetupRecord = bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
                    current.setup.validate().map_err(|_| AgentFailure::Conflict)?;
                    if current.command_id != record.command_id || current.person_id != record.person_id
                        || current.device_id != record.device_id || current.address_digest != record.address_digest
                        || current.setup.target_ref != record.setup.target_ref
                        || current.setup.display_address != record.setup.display_address {
                        return Err(AgentFailure::Conflict);
                    }
                    // Exact intent rejoins the first descriptor, including its
                    // original expiry, even after observer/commit response loss.
                    return Ok(current);
                }
                drop(rows);
                tx.execute("INSERT INTO gateway_setup_receipts VALUES(?,?,?,?)", (
                    record.setup.target_ref.to_string(), record.person_id.to_string(), record.command_id.to_string(), bounded_encode(&record)?
                )).await.map_err(storage)?;
                Ok(record)
            }.await;
            self.finish_access_grant_transaction(tx, result)
                .await
                .map_err(pairing_storage)
        })
    }
    fn execute_command<'a>(
        &'a self,
        command: PairingMutation,
    ) -> BoxFuture<'a, Result<PairingMutationReceipt, PairingError>> {
        Box::pin(async move {
            if command.person_id != self.person_id
                || command.command_id.is_nil()
                || command.device_id.is_empty()
                || command.device_id.len() > 256
            {
                return Err(PairingError::ForeignIdentity);
            }
            let mut connection = self.connection().map_err(pairing_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(pairing_database)?;
            let result = async {
                let mut rows = tx.query("SELECT payload FROM gateway_pairing_command_receipts WHERE person_id=? AND command_id=?",
                    (command.person_id.to_string(), command.command_id.to_string())).await.map_err(storage)?;
                if let Some(row) = rows.next().await.map_err(storage)? {
                    let receipt: PairingMutationReceipt = bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
                    if receipt.command != command { return Err(AgentFailure::Conflict); }
                    self.check_access()?;
                    return Ok(receipt);
                }
                drop(rows);
                let outcome = execute_pairing_command_on(&tx, &command).await?;
                let receipt = PairingMutationReceipt { command, outcome };
                tx.execute("INSERT INTO gateway_pairing_command_receipts VALUES(?,?,?)", (
                    receipt.command.person_id.to_string(), receipt.command.command_id.to_string(), bounded_encode(&receipt)?,
                )).await.map_err(storage)?;
                self.check_access()?;
                Ok(receipt)
            }.await;
            // Only a successfully committed exact receipt can resolve earlier
            // uncertainty. Rollback/commit/access errors never fabricate one.
            self.finish_access_grant_transaction(tx, result)
                .await
                .map_err(pairing_storage)
        })
    }

    fn load<'a>(&'a self, id: Uuid) -> BoxFuture<'a, Result<Option<PairingRecord>, PairingError>> {
        Box::pin(async move {
            let record = pairing_on(
                &self.connection().map_err(pairing_storage)?,
                self.person_id,
                id,
            )
            .await
            .map_err(pairing_storage)?;
            self.check_access().map_err(pairing_storage)?;
            Ok(record)
        })
    }

    fn compare_and_swap<'a>(
        &'a self,
        expected: u64,
        next: PairingRecord,
    ) -> BoxFuture<'a, Result<PairingRecord, PairingError>> {
        Box::pin(async move {
            let mut connection = self.connection().map_err(pairing_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(pairing_database)?;
            let result = async {
                let current = pairing_on(&tx, self.person_id, next.operation_id)
                    .await?
                    .ok_or(AgentFailure::Conflict)?;
                current
                    .validate_successor(&next, expected)
                    .map_err(|_| AgentFailure::Conflict)?;
                if !pending_for(&expectation_on(&tx).await?, current.operation_id) {
                    return Err(AgentFailure::Conflict);
                }
                // A terminal remote observation refers to an existing exact
                // handle. A cancellation before dispatch is locally proven.
                let proven_terminal = matches!(
                    next.state,
                    PairingState::Rejected | PairingState::Expired | PairingState::Cancelled
                ) && (next.handle.is_some()
                    || next.start_phase == PairingStartPhase::Staged);
                if proven_terminal {
                    set_expectation_on(&tx, &current.prior_credential_expectation).await?;
                }
                write_pairing_on(&tx, expected, &next).await?;
                self.check_access()?;
                Ok(next)
            }
            .await;
            self.finish_access_grant_transaction(tx, result)
                .await
                .map_err(pairing_storage)
        })
    }

    fn accept_started<'a>(
        &'a self,
        expected: u64,
        started: StartedPairing,
    ) -> BoxFuture<'a, Result<PairingRecord, PairingError>> {
        Box::pin(async move {
            let mut connection = self.connection().map_err(pairing_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(pairing_database)?;
            let result = async {
                let current = pairing_on(&tx, self.person_id, started.challenge.handle.operation_id).await?.ok_or(AgentFailure::Conflict)?;
                let private = private_on(&tx, &current).await?;
                if current.handle.as_ref() == Some(&started.challenge.handle)
                    && current.reviewed.as_ref() == Some(&started.challenge.reviewed)
                    && current.display_code.as_ref() == Some(&started.challenge.display_code)
                    && current.expires_at_unix_ms == Some(started.challenge.expires_at_unix_ms)
                    && private.enrollment.as_ref() == Some(&started.enrollment) {
                    return Ok(current);
                }
                if expected > current.revision || (current.revision != expected
                    && current.cancellation_command.is_none() && current.forgotten_command.is_none()) {
                    return Err(AgentFailure::Conflict);
                }
                if current.forgotten_command.is_none() && !pending_for(&expectation_on(&tx).await?, current.operation_id) {
                    return Err(AgentFailure::Conflict);
                }
                // Validate cryptographic material at the storage admission as
                // well as at the external protocol adapter.
                super::gateway_authority::verify_producer_signature(&started.enrollment.producer,
                    &started.enrollment.canonical_bytes, &started.enrollment.producer_signature)?;
                if started.enrollment.request_digest != <[u8; 32]>::from(sha2::Sha256::digest(&started.enrollment.canonical_bytes)) {
                    return Err(AgentFailure::PolicyDenied);
                }
                let next = current.accept_started(&started).map_err(|_| AgentFailure::PolicyDenied)?;
                if tx.execute("UPDATE gateway_pairing_private SET enrollment_json=? WHERE operation_id=? AND enrollment_json IS NULL",
                    (bounded_encode(&started.enrollment)?, current.operation_id.to_string())).await.map_err(storage)? != 1 {
                    return Err(AgentFailure::Conflict);
                }
                write_pairing_on(&tx, current.revision, &next).await?;
                self.check_access()?;
                Ok(next)
            }.await;
            self.finish_access_grant_transaction(tx, result)
                .await
                .map_err(pairing_storage)
        })
    }
    fn activate<'a>(
        &'a self,
        expected: u64,
        approval: PairingApproval,
    ) -> BoxFuture<'a, Result<PairingActivationResult, PairingError>> {
        Box::pin(async move {
            let owner = self
                .remote_owner_public_key()
                .await
                .map_err(pairing_storage)?;
            let mut connection = self.connection().map_err(pairing_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(pairing_database)?;
            let result = async {
                let current = pairing_on(&tx, self.person_id, approval.handle.operation_id).await?.ok_or(AgentFailure::Conflict)?;
                let private = private_on(&tx, &current).await?;
                let enrolled = private.enrollment.as_ref().ok_or(AgentFailure::PolicyDenied)?;
                current.validate_approval(&approval, enrolled).map_err(|_| AgentFailure::PolicyDenied)?;
                if enrolled.issuer != owner { return Err(AgentFailure::PolicyDenied); }
                let mut rows = tx.query("SELECT command_json FROM gateway_enrollment_receipts WHERE operation_id=?",
                    (current.operation_id.to_string(),)).await.map_err(storage)?;
                let signed: EnrollmentSigningCommand = bounded_decode(&rows.next().await.map_err(storage)?
                    .ok_or(AgentFailure::PolicyDenied)?.get::<String>(0).map_err(storage)?)?;
                drop(rows);
                if &signed != enrolled { return Err(AgentFailure::PolicyDenied); }
                let expectation = expectation_on(&tx).await?;
                if let Some(existing) = &private.credential {
                    if existing.as_bytes() != approval.credential.as_bytes() { return Err(AgentFailure::PolicyDenied); }
                    if current.forgotten_command.is_some() || current.cancellation_command.is_some() || current.state != PairingState::Paired {
                        return Ok(PairingActivationResult::HistoricalRevocationEvidence(current));
                    }
                    if current.state == PairingState::Paired
                        && expectation == (GatewayCredentialExpectation::Committed {
                            operation_id: current.operation_id, generation: current.generation,
                        }) {
                        return Ok(PairingActivationResult::Activated(current));
                    }
                    return Err(AgentFailure::Conflict);
                }
                if current.forgotten_command.is_some() || current.cancellation_command.is_some()
                    || matches!(current.state, PairingState::Rejected | PairingState::Expired | PairingState::Cancelled) {
                    // Approval may have won remotely while local cancellation
                    // or Forget won. Retain recovery evidence, never authority.
                    let mut historical = current.clone();
                    historical.revision = historical.revision.checked_add(1).ok_or(AgentFailure::Conflict)?;
                    historical.state = if current.forgotten_command.is_some() { PairingState::Forgotten } else { PairingState::RevocationPending };
                    historical.last_failure = Some(PairingError::Indeterminate);
                    write_pairing_on(&tx, current.revision, &historical).await?;
                    tx.execute("UPDATE gateway_pairing_private SET credential=? WHERE operation_id=? AND credential IS NULL",
                        (approval.credential.as_bytes(), current.operation_id.to_string())).await.map_err(storage)?;
                    self.check_access()?;
                    return Ok(PairingActivationResult::HistoricalRevocationEvidence(historical));
                }
                if current.revision != expected || !pending_for(&expectation, current.operation_id) {
                    return Err(AgentFailure::Conflict);
                }
                let pin = pin_on(&tx).await?;
                let pin_revision = pin.as_ref().map_or(0, |pin| pin.revision);
                if pin_revision != approval.reviewed.expected_pin_revision { return Err(AgentFailure::Conflict); }
                let successor = pin_revision.checked_add(1).filter(|value| *value <= i64::MAX as u64)
                    .ok_or(AgentFailure::Conflict)?;
                let next = current.activate(&approval, enrolled, successor).map_err(|_| AgentFailure::Conflict)?;
                let pin_receipt = GatewayPinReceipt {
                    command: GatewayPinCommit { operation_id: current.operation_id, request_digest: enrolled.request_digest,
                        expected_revision: pin_revision, producer: enrolled.producer.clone() },
                    revision: successor,
                };
                tx.execute("INSERT INTO remote_authority_producer(id,identity_json,revision) VALUES(1,?,?) ON CONFLICT(id) DO UPDATE SET identity_json=excluded.identity_json,revision=excluded.revision",
                    (bounded_encode(&enrolled.producer)?, successor as i64)).await.map_err(storage)?;
                tx.execute("INSERT INTO gateway_pin_receipts VALUES(?,?)",
                    (current.operation_id.to_string(), bounded_encode(&pin_receipt)?)).await.map_err(storage)?;
                if tx.execute("UPDATE gateway_pairing_private SET credential=? WHERE operation_id=? AND credential IS NULL",
                    (approval.credential.as_bytes(), current.operation_id.to_string())).await.map_err(storage)? != 1 {
                    return Err(AgentFailure::Conflict);
                }
                set_expectation_on(&tx, &GatewayCredentialExpectation::Committed {
                    operation_id: current.operation_id, generation: current.generation,
                }).await?;
                write_pairing_on(&tx, expected, &next).await?;
                self.check_access()?;
                Ok(PairingActivationResult::Activated(next))
            }.await;
            self.finish_access_grant_transaction(tx, result)
                .await
                .map_err(pairing_storage)
        })
    }

    fn pending<'a>(
        &'a self,
        person: PersonId,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<PairingRecord>, PairingError>> {
        Box::pin(async move {
            if person != self.person_id || limit == 0 || limit > 64 {
                return Err(PairingError::InvalidInput);
            }
            let mut connection = self.connection().map_err(pairing_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(pairing_database)?;
            let result = async {
                let GatewayCredentialExpectation::Pending { operation_id } =
                    expectation_on(&tx).await?
                else {
                    return Ok(Vec::new());
                };
                let current = pairing_on(&tx, person, operation_id)
                    .await?
                    .ok_or(AgentFailure::VaultUnavailable)?;
                private_on(&tx, &current).await?;
                if current.forgotten_command.is_none()
                    && (!current.state.terminal() || current.can_reconcile_repair())
                {
                    Ok(vec![current])
                } else {
                    Ok(Vec::new())
                }
            }
            .await;
            self.finish_access_grant_transaction(tx, result)
                .await
                .map_err(pairing_storage)
        })
    }
}

impl<K: VaultKeyProvider> GatewayPrivateReader for EncryptedAgentVault<K> {
    fn pairing_private<'a>(
        &'a self,
        operation: Uuid,
        person: PersonId,
        device: &'a str,
        generation: u64,
    ) -> BoxFuture<'a, Result<PairingPrivateSnapshot, PairingError>> {
        Box::pin(async move {
            if person != self.person_id {
                return Err(PairingError::ForeignIdentity);
            }
            let mut connection = self.connection().map_err(pairing_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(pairing_database)?;
            let result = async {
                let record = pairing_on(&tx, person, operation)
                    .await?
                    .ok_or(AgentFailure::NotFound)?;
                if record.device_id != device || record.generation != generation {
                    return Err(AgentFailure::PolicyDenied);
                }
                let private = private_on(&tx, &record).await?;
                Ok(PairingPrivateSnapshot {
                    operation: record,
                    proof: private.proof,
                    enrollment: private.enrollment,
                })
            }
            .await;
            self.finish_access_grant_transaction(tx, result)
                .await
                .map_err(pairing_storage)
        })
    }

    fn credential<'a>(
        &'a self,
        person: PersonId,
        device: &'a str,
    ) -> BoxFuture<'a, Result<GatewayCredentialRead, PairingError>> {
        Box::pin(async move {
            if person != self.person_id {
                return Err(PairingError::ForeignIdentity);
            }
            let mut connection = self.connection().map_err(pairing_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(pairing_database)?;
            let result = async {
                let expectation = expectation_on(&tx).await?;
                let (operation_id, generation) = match expectation {
                    GatewayCredentialExpectation::Unpaired => {
                        return Ok(GatewayCredentialRead::Absent { expectation });
                    }
                    GatewayCredentialExpectation::Forgotten {
                        operation_id,
                        generation,
                    } => {
                        validate_forgotten_on(&tx, person, device, operation_id, generation)
                            .await?;
                        return Ok(GatewayCredentialRead::Absent { expectation });
                    }
                    GatewayCredentialExpectation::Pending { .. } => {
                        return Ok(GatewayCredentialRead::RepairRequired { expectation });
                    }
                    GatewayCredentialExpectation::Committed {
                        operation_id,
                        generation,
                    } => (operation_id, generation),
                };
                let operation = pairing_on(&tx, person, operation_id)
                    .await?
                    .ok_or(AgentFailure::VaultUnavailable)?;
                if operation.device_id != device {
                    return Err(AgentFailure::PolicyDenied);
                }
                let private = private_on(&tx, &operation).await?;
                let enrolled = operation
                    .enrollment
                    .as_ref()
                    .ok_or(AgentFailure::VaultUnavailable)?;
                let pin = pin_on(&tx).await?.ok_or(AgentFailure::PolicyDenied)?;
                if operation.state != PairingState::Paired
                    || operation.forgotten_command.is_some()
                    || operation.cancellation_command.is_some()
                    || operation.generation != generation
                    || enrolled.operation_id != operation_id
                    || enrolled.binding.credential_generation != generation
                    || enrolled.binding.person_id != person.to_string()
                    || enrolled.binding.device_id != device
                    || enrolled.pin_revision != pin.revision
                    || enrolled.binding.producer_instance != pin.producer.instance_id
                    || enrolled.binding.producer_key_fingerprint != pin.producer.fingerprint
                    || enrolled.binding.producer_audience != pin.producer.audience
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                enrolled.binding.validate()?;
                let material = private.credential.ok_or(AgentFailure::VaultUnavailable)?;
                Ok(GatewayCredentialRead::Active(GatewayCredentialSnapshot {
                    operation_id,
                    binding: enrolled.binding.clone(),
                    endpoint: operation.setup.display_address,
                    bearer: material,
                }))
            }
            .await;
            self.finish_access_grant_transaction(tx, result)
                .await
                .map_err(pairing_storage)
        })
    }
}

impl<K: VaultKeyProvider> GatewayRegistry for EncryptedAgentVault<K> {
    fn current<'a>(
        &'a self,
        person: PersonId,
        device: &'a str,
    ) -> BoxFuture<'a, Result<Option<GatewayObservation>, PairingError>> {
        Box::pin(async move {
            if person != self.person_id {
                return Err(PairingError::ForeignIdentity);
            }
            let mut connection = self.connection().map_err(pairing_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(pairing_database)?;
            let result = async {
                let expectation = expectation_on(&tx).await?;
                let operation_id = match expectation {
                    GatewayCredentialExpectation::Unpaired => return Ok(None),
                    GatewayCredentialExpectation::Forgotten {
                        operation_id,
                        generation,
                    } => {
                        validate_forgotten_on(&tx, person, device, operation_id, generation)
                            .await?;
                        return Ok(None);
                    }
                    GatewayCredentialExpectation::Pending { operation_id }
                    | GatewayCredentialExpectation::Committed { operation_id, .. } => operation_id,
                };
                let operation = pairing_on(&tx, person, operation_id)
                    .await?
                    .ok_or(AgentFailure::VaultUnavailable)?;
                if operation.device_id != device || operation.forgotten_command.is_some() {
                    return Err(AgentFailure::PolicyDenied);
                }
                let private = private_on(&tx, &operation).await?;
                if let GatewayCredentialExpectation::Committed { generation, .. } = expectation {
                    let enrolled = operation
                        .enrollment
                        .as_ref()
                        .ok_or(AgentFailure::VaultUnavailable)?;
                    let pin = pin_on(&tx).await?.ok_or(AgentFailure::PolicyDenied)?;
                    if operation.state != PairingState::Paired
                        || operation.generation != generation
                        || operation.cancellation_command.is_some()
                        || private.credential.is_none()
                        || enrolled.pin_revision != pin.revision
                        || enrolled.binding.producer_instance != pin.producer.instance_id
                        || enrolled.binding.producer_key_fingerprint != pin.producer.fingerprint
                        || enrolled.binding.producer_audience != pin.producer.audience
                    {
                        return Err(AgentFailure::PolicyDenied);
                    }
                    return Ok(Some(GatewayObservation::Paired {
                        summary: operation
                            .snapshot()
                            .gateway
                            .ok_or(AgentFailure::VaultUnavailable)?,
                        binding: enrolled.binding.clone(),
                    }));
                }
                Ok(Some(GatewayObservation::RepairRequired {
                    summary: GatewaySummary {
                        display_address: Some(operation.setup.display_address.clone()),
                        gateway_ref: operation_id,
                        revision: operation.revision,
                        display_name: "Gateway".into(),
                        state: GatewayState::RepairRequired,
                        remote_revocation_pending: operation.start_phase
                            == PairingStartPhase::Dispatched,
                        allowed_actions: vec![ConnectionAction::Forget],
                        failure: operation.snapshot().failure.map(|mut notice| {
                            notice.safe_actions = vec![ConnectionAction::Forget];
                            notice.recovery = ConnectionRecovery::None;
                            notice.reload_required = false;
                            notice
                        }),
                    },
                    expectation,
                }))
            }
            .await;
            self.finish_access_grant_transaction(tx, result)
                .await
                .map_err(pairing_storage)
        })
    }

    fn forget<'a>(
        &'a self,
        receipt: ConnectionsRecord,
        expected: GatewayForgetExpectation,
    ) -> BoxFuture<'a, Result<GatewaySummary, PairingError>> {
        Box::pin(async move {
            receipt.validate().map_err(pairing_storage)?;
            if receipt.person_id != self.person_id || receipt.revision != 1 {
                return Err(PairingError::ForeignIdentity);
            }
            let ConnectionsPayload::GatewayForgotten(summary) = &receipt.payload else {
                return Err(PairingError::InvalidInput);
            };
            let mut connection = self.connection().map_err(pairing_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(pairing_database)?;
            let result = async {
                if super::gateway_authority::command_rejection_on(&tx, receipt.person_id, receipt.command_id).await?.is_some() {
                    return Err(AgentFailure::Conflict);
                }
                let mut rows = tx.query("SELECT payload FROM connections_product_records WHERE record_ref=? OR (person_id=? AND command_id=?)",
                    (receipt.record_ref.to_string(), receipt.person_id.to_string(), receipt.command_id.to_string())).await.map_err(storage)?;
                if let Some(row) = rows.next().await.map_err(storage)? {
                    let previous: ConnectionsRecord = bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
                    if previous != receipt { return Err(AgentFailure::Conflict); }
                    return Ok(summary.clone());
                }
                drop(rows);
                let operation = pairing_on(&tx, self.person_id, summary.gateway_ref).await?.ok_or(AgentFailure::NotFound)?;
                if operation.device_id != receipt.device_id || operation.forgotten_command.is_some() {
                    return Err(AgentFailure::Conflict);
                }
                let expectation = expectation_on(&tx).await?;
                let observed_revision = match &expected {
                    GatewayForgetExpectation::Paired(binding) => {
                        let enrolled = operation.enrollment.as_ref().ok_or(AgentFailure::PolicyDenied)?;
                        if operation.state != PairingState::Paired || &enrolled.binding != binding
                            || expectation != (GatewayCredentialExpectation::Committed {
                                operation_id: operation.operation_id, generation: operation.generation,
                            }) {
                            return Err(AgentFailure::Conflict);
                        }
                        operation.generation
                    }
                    GatewayForgetExpectation::RepairRequired { gateway_ref, revision, expectation: reviewed } => {
                        if *gateway_ref != operation.operation_id || *revision != operation.revision
                            || &expectation != reviewed || !pending_for(&expectation, operation.operation_id) {
                            return Err(AgentFailure::Conflict);
                        }
                        *revision
                    }
                };
                if summary.revision != observed_revision.checked_add(1).ok_or(AgentFailure::Conflict)?
                    || summary.remote_revocation_pending != (operation.start_phase == PairingStartPhase::Dispatched)
                    || summary.allowed_actions != vec![ConnectionAction::Pair]
                { return Err(AgentFailure::Conflict); }
                let next = operation.forget(receipt.command_id).map_err(|_| AgentFailure::Conflict)?;
                write_pairing_on(&tx, operation.revision, &next).await?;
                set_expectation_on(&tx, &GatewayCredentialExpectation::Forgotten {
                    operation_id: operation.operation_id, generation: operation.generation,
                }).await?;
                tx.execute("INSERT INTO connections_product_records VALUES(?,?,?,?,?)", (
                    receipt.record_ref.to_string(), receipt.person_id.to_string(), receipt.command_id.to_string(),
                    receipt.revision as i64, bounded_encode(&receipt)?,
                )).await.map_err(storage)?;
                self.check_access()?;
                Ok(summary.clone())
            }.await;
            self.finish_access_grant_transaction(tx, result)
                .await
                .map_err(pairing_storage)
        })
    }
}

fn pairing_database(error: turso::Error) -> PairingError {
    match error {
        turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => PairingError::Conflict,
        _ => PairingError::StorageUnavailable,
    }
}
