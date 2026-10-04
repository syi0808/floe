//! Bounded encrypted identity/operation/receipt storage. Pairing lifecycle and
//! source policy belong to Connections and Access, respectively.
use super::authority_keys::{decode_canonical, decode_exact, enrollment_issuer_on};
use super::gateway_pairing_store::pairing_state;
use super::{EncryptedAgentVault, VaultKeyProvider, storage};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use floe_access::{
    AssistantAuthorizationSigningCommand, AuthorizationSignature, AuthorizationSigner,
    AuthorizationSigningCommand, GatewayTrustReader, ProductCalendarChallenge,
    ProductCalendarSigningCommand, RemoteProducerIdentity, VerifiedGatewayBinding,
};
use floe_agent_contract::{AgentFailure, BoxFuture};
use floe_connections::*;
use ring::signature;
use sha2::{Digest, Sha256};
use turso::transaction::{Transaction, TransactionBehavior};
use uuid::Uuid;
const OWNER_SIGNATURE_DOMAIN: &[u8] = b"floe.remote.authorization.v1\0";
const PRODUCER_SIGNATURE_DOMAIN: &[u8] = b"floe.remote.producer.v1\0";
const MAX_RECORD: usize = 131_072;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct ProductAuthorizationReceipt {
    challenge: ProductCalendarChallenge,
    canonical_bytes_b64url: String,
    producer_signature_b64url: String,
    producer: RemoteProducerIdentity,
    person_id: String,
    device_id: String,
    owner_key_id: String,
    gateway_runtime_generation: u64,
    signature: AuthorizationSignature,
}
impl ProductAuthorizationReceipt {
    fn matches_command(&self, expected: &Self) -> bool {
        self.challenge == expected.challenge
            && self.canonical_bytes_b64url == expected.canonical_bytes_b64url
            && self.producer_signature_b64url == expected.producer_signature_b64url
            && self.producer == expected.producer
            && self.person_id == expected.person_id
            && self.device_id == expected.device_id
            && self.owner_key_id == expected.owner_key_id
            && self.gateway_runtime_generation == expected.gateway_runtime_generation
            && self.signature.key_id == expected.signature.key_id
    }
}

impl<K: VaultKeyProvider> EncryptedAgentVault<K> {
    pub(super) async fn remote_pinned_producer(
        &self,
    ) -> Result<RemoteProducerIdentity, AgentFailure> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(storage)?;
        let result = async {
            let authority = current_credential_in_transaction(&tx, self.person_id).await?;
            self.check_access()?;
            Ok(authority.pin.producer)
        }
        .await;
        self.finish_access_grant_transaction(tx, result).await
    }
    async fn current_pin_record(&self) -> Result<Option<GatewayPin>, AgentFailure> {
        let mut rows = self
            .connection()?
            .query(
                "SELECT identity_json,revision FROM remote_authority_producer WHERE id=1",
                (),
            )
            .await
            .map_err(storage)?;
        let Some(row) = rows.next().await.map_err(storage)? else {
            return Ok(None);
        };
        let producer: RemoteProducerIdentity =
            bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
        validate_producer(&producer)?;
        let revision = row.get::<i64>(1).map_err(storage)?;
        if revision <= 0 || rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::PolicyDenied);
        }
        self.check_access()?;
        Ok(Some(GatewayPin {
            producer,
            revision: revision as u64,
        }))
    }
    async fn enrollment_command(
        &self,
        id: Uuid,
    ) -> Result<Option<EnrollmentSigningCommand>, AgentFailure> {
        let mut rows = self
            .connection()?
            .query(
                "SELECT command_json FROM gateway_enrollment_receipts WHERE operation_id=?",
                (id.to_string(),),
            )
            .await
            .map_err(storage)?;
        rows.next()
            .await
            .map_err(storage)?
            .map(|row| bounded_decode(&row.get::<String>(0).map_err(storage)?))
            .transpose()
    }
    async fn committed_issuer(&self) -> Result<floe_access::RemoteOwnerPublicKey, AgentFailure> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(storage)?;
        let result = async {
            let authority = current_credential_in_transaction(&tx, self.person_id).await?;
            let (_, issuer) = self.enrollment_key_on(&tx, authority.operation_id).await?;
            if issuer != authority.issuer {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(issuer)
        }
        .await;
        self.finish_access_grant_transaction(tx, result).await
    }
    pub(super) async fn advance_clock(
        &self,
        tx: &Transaction<'_>,
        now: i64,
    ) -> Result<(), AgentFailure> {
        let mut rows = tx
            .query(
                "SELECT last_now_unix_ms FROM remote_authority_clock WHERE id=1",
                (),
            )
            .await
            .map_err(storage)?;
        let previous = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?
            .get::<i64>(0)
            .map_err(storage)?;
        if now < previous {
            return Err(AgentFailure::PolicyDenied);
        }
        drop(rows);
        tx.execute(
            "UPDATE remote_authority_clock SET last_now_unix_ms=? WHERE id=1",
            (now,),
        )
        .await
        .map_err(storage)?;
        Ok(())
    }
}
impl<K: VaultKeyProvider> GatewayTrustReader for EncryptedAgentVault<K> {
    fn credential_expectation<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<floe_access::GatewayCredentialExpectation, AgentFailure>> {
        Box::pin(async move {
            let expectation =
                super::gateway_pairing_store::expectation_on(&self.connection()?).await?;
            self.check_access()?;
            Ok(expectation)
        })
    }
    fn pinned_producer<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
        Box::pin(self.remote_pinned_producer())
    }
}
pub struct VaultEnrollmentSigner<K: VaultKeyProvider> {
    vault: std::sync::Arc<EncryptedAgentVault<K>>,
    verifier: std::sync::Arc<dyn EnrollmentProofVerifier>,
}
impl<K: VaultKeyProvider> VaultEnrollmentSigner<K> {
    pub fn new(
        vault: std::sync::Arc<EncryptedAgentVault<K>>,
        verifier: std::sync::Arc<dyn EnrollmentProofVerifier>,
    ) -> Self {
        Self { vault, verifier }
    }
}
impl<K: VaultKeyProvider> EnrollmentSigner for VaultEnrollmentSigner<K> {
    fn sign_enrollment<'a>(
        &'a self,
        command: EnrollmentSigningCommand,
    ) -> BoxFuture<'a, Result<EnrollmentSignature, PairingError>> {
        Box::pin(async move {
            self.verifier.verify(&command)?;
            validate_enrollment(&command, self.vault.person_id)?;
            let mut connection = self.vault.connection().map_err(pairing_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(|_| PairingError::StorageUnavailable)?;
            let result = async {
                self.vault.advance_clock(&tx, chrono::Utc::now().timestamp_millis()).await?;
                let mut rows = tx.query(
                    "SELECT payload FROM gateway_pairing_operations WHERE operation_id=? AND person_id=?",
                    (command.operation_id.to_string(), self.vault.person_id.to_string()),
                ).await.map_err(storage)?;
                let operation: PairingRecord = bounded_decode(&rows.next().await.map_err(storage)?
                    .ok_or(AgentFailure::PolicyDenied)?.get::<String>(0).map_err(storage)?)?;
                drop(rows);
                operation.validate().map_err(|_| AgentFailure::PolicyDenied)?;
                if operation.person_id != command.person_id || operation.device_id != command.device_id
                    || operation.confirmation_command.is_none() || operation.cancellation_command.is_some()
                    || operation.forgotten_command.is_some()
                    || !(operation.state == PairingState::AwaitingApproval || operation.can_reconcile_repair()) {
                    return Err(AgentFailure::PolicyDenied);
                }
                let mut rows = tx.query("SELECT payload FROM gateway_credential_expectation WHERE id=1", ())
                    .await.map_err(storage)?;
                let expectation: floe_access::GatewayCredentialExpectation = bounded_decode(
                    &rows.next().await.map_err(storage)?.ok_or(AgentFailure::PolicyDenied)?
                        .get::<String>(0).map_err(storage)?,
                )?;
                drop(rows);
                if expectation != (floe_access::GatewayCredentialExpectation::Pending { operation_id: command.operation_id }) {
                    return Err(AgentFailure::PolicyDenied);
                }
                let mut rows = tx.query("SELECT enrollment_json FROM gateway_pairing_private WHERE operation_id=?",
                    (command.operation_id.to_string(),)).await.map_err(storage)?;
                let admitted: EnrollmentSigningCommand = bounded_decode(
                    &rows.next().await.map_err(storage)?.ok_or(AgentFailure::PolicyDenied)?
                        .get::<Option<String>>(0).map_err(storage)?.ok_or(AgentFailure::PolicyDenied)?,
                )?;
                drop(rows);
                if admitted != command { return Err(AgentFailure::Conflict); }
                let mut rows = tx.query("SELECT command_json FROM gateway_enrollment_receipts WHERE operation_id=? OR challenge_id=?",
                    (command.operation_id.to_string(), command.challenge_id.to_string())).await.map_err(storage)?;
                if let Some(row) = rows.next().await.map_err(storage)? {
                    let existing: EnrollmentSigningCommand = bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
                    if existing != command { return Err(AgentFailure::Conflict); }
                } else {
                    drop(rows);
                    tx.execute("INSERT INTO gateway_enrollment_receipts VALUES(?,?,?)", (
                        command.operation_id.to_string(), command.challenge_id.to_string(), bounded_encode(&command)?,
                    )).await.map_err(storage)?;
                }
                let (key, issuer) = self.vault.enrollment_key_on(&tx, command.operation_id).await?;
                if issuer != command.issuer { return Err(AgentFailure::PolicyDenied); }
                let mut bytes = OWNER_SIGNATURE_DOMAIN.to_vec();
                bytes.extend_from_slice(&command.canonical_bytes);
                let signature = URL_SAFE_NO_PAD.encode(key.sign(&bytes).as_ref());
                self.vault.check_access()?;
                Ok(EnrollmentSignature {
                    operation_id: command.operation_id, request_digest: command.request_digest,
                    key_id: issuer.key_id, signature,
                })
            }.await;
            self.vault
                .finish_access_grant_transaction(tx, result)
                .await
                .map_err(pairing_storage)
        })
    }
    fn readback<'a>(
        &'a self,
        id: Uuid,
    ) -> BoxFuture<'a, Result<Option<EnrollmentReceipt>, PairingError>> {
        Box::pin(async move {
            match self
                .vault
                .enrollment_command(id)
                .await
                .map_err(pairing_storage)?
            {
                Some(command) => Ok(Some(EnrollmentReceipt {
                    operation_id: command.operation_id,
                    request_digest: command.request_digest,
                    key_id: command.issuer.key_id,
                })),
                None => Ok(None),
            }
        })
    }
}
impl<K: VaultKeyProvider> GatewayAuthorityRepository for EncryptedAgentVault<K> {
    fn current_pin<'a>(&'a self) -> BoxFuture<'a, Result<Option<GatewayPin>, PairingError>> {
        Box::pin(async move { self.current_pin_record().await.map_err(pairing_storage) })
    }
}

fn validate_enrollment(
    command: &EnrollmentSigningCommand,
    person: floe_kernel::PersonId,
) -> Result<(), PairingError> {
    let now = chrono::Utc::now().timestamp_millis();
    if command.operation_id.is_nil()
        || command.challenge_id.is_nil()
        || command.person_id != person
        || command.device_id.is_empty()
        || command.client_id.is_empty()
        || command.canonical_bytes.is_empty()
        || command.canonical_bytes.len() > 65536
        || command.producer_signature.len() != 64
        || command.request_digest != <[u8; 32]>::from(Sha256::digest(&command.canonical_bytes))
        || command.issued_at_unix_ms > now.saturating_add(5000)
        || command.expires_at_unix_ms <= now
    {
        return Err(PairingError::Rejected);
    }
    verify_producer_signature(
        &command.producer,
        &command.canonical_bytes,
        &command.producer_signature,
    )
    .map_err(pairing_storage)
}
pub(super) fn validate_producer(producer: &RemoteProducerIdentity) -> Result<(), AgentFailure> {
    let key = decode_exact(&producer.public_key, 32)?;
    if producer.schema_version != 1
        || producer.audience != format!("floe.server:{}", producer.instance_id)
        || [
            &producer.instance_id,
            &producer.key_id,
            &producer.execution_owner,
        ]
        .iter()
        .any(|id| Uuid::parse_str(id).is_err() || Uuid::parse_str(id).is_ok_and(|id| id.is_nil()))
        || producer.fingerprint != hex(&Sha256::digest(&key))
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}
pub(super) fn verify_producer_signature(
    producer: &RemoteProducerIdentity,
    bytes: &[u8],
    proof: &[u8],
) -> Result<(), AgentFailure> {
    validate_producer(producer)?;
    let key = decode_exact(&producer.public_key, 32)?;
    let mut message = Vec::with_capacity(PRODUCER_SIGNATURE_DOMAIN.len() + bytes.len());
    message.extend_from_slice(PRODUCER_SIGNATURE_DOMAIN);
    message.extend_from_slice(bytes);
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(&message, proof)
        .map_err(|_| AgentFailure::PolicyDenied)
}
pub(super) fn bounded_decode<T: serde::de::DeserializeOwned>(
    payload: &str,
) -> Result<T, AgentFailure> {
    if payload.len() > MAX_RECORD {
        return Err(AgentFailure::VaultUnavailable);
    }
    serde_json::from_str(payload).map_err(|_| AgentFailure::VaultUnavailable)
}
pub(super) fn bounded_encode(value: &impl serde::Serialize) -> Result<String, AgentFailure> {
    let payload = serde_json::to_string(value).map_err(|_| AgentFailure::VaultUnavailable)?;
    if payload.len() > MAX_RECORD {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload)
}
pub(super) fn pairing_storage(error: AgentFailure) -> PairingError {
    match error {
        AgentFailure::Conflict => PairingError::Conflict,
        AgentFailure::PolicyDenied => PairingError::ChangedProducer,
        _ => PairingError::StorageUnavailable,
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub struct VaultAuthorizationSigner<K: VaultKeyProvider> {
    vault: std::sync::Arc<EncryptedAgentVault<K>>,
    verifier: std::sync::Arc<dyn floe_access::AuthorizationProofVerifier>,
}
impl<K: VaultKeyProvider> VaultAuthorizationSigner<K> {
    pub fn new(
        vault: std::sync::Arc<EncryptedAgentVault<K>>,
        verifier: std::sync::Arc<dyn floe_access::AuthorizationProofVerifier>,
    ) -> Self {
        Self { vault, verifier }
    }

    async fn sign_assistant_authorization(
        &self,
        command: AssistantAuthorizationSigningCommand,
    ) -> Result<AuthorizationSignature, AgentFailure> {
        let owner = self.vault.committed_issuer().await?;
        let claims = self.verifier.verify(&command)?;
        command.validate_claims(
            &claims,
            self.vault.person_id,
            &owner.key_id,
            chrono::Utc::now().timestamp_millis(),
        )?;
        verify_producer_signature(
            &command.producer,
            &command.canonical_bytes,
            &command.producer_signature,
        )?;
        let grant_id = floe_access::GrantId::from_uuid(
            Uuid::parse_str(&command.expected.grant_id).map_err(|_| AgentFailure::PolicyDenied)?,
        )
        .ok_or(AgentFailure::PolicyDenied)?;
        let mut connection = self.vault.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let now = chrono::Utc::now().timestamp_millis();
            self.vault.advance_clock(&tx, now).await?;
            if command.expires_at_unix_ms <= now {
                return Err(AgentFailure::PolicyDenied);
            }
            let authority = current_credential_in_transaction(&tx, self.vault.person_id).await?;
            if authority.issuer != owner || authority.pin.producer != command.producer
                || authority.binding.client_id != command.expected.client_id
                || authority.binding.device_id != command.expected.device_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let grant = self
                .vault
                .read_data_access_grant_in_transaction(&tx, grant_id)
                .await?;
            floe_access::validate_authorization_grant(
                &grant,
                self.vault.person_id,
                &command.expected,
                command.purpose,
                &command.consumer,
            )?;
            if command.expected.operation == "release" {
                let mut rows = tx
                    .query(
                        "SELECT expectation_json,expires_at_unix_ms FROM gateway_authorization_receipts WHERE challenge_id=? AND operation='admission'",
                        (command.expected.admission_id.clone(),),
                    )
                    .await
                    .map_err(storage)?;
                let row = rows.next().await.map_err(storage)?.ok_or(AgentFailure::PolicyDenied)?;
                let admitted: floe_access::RemoteViewAuthorizationExpectation = bounded_decode(
                    &row.get::<String>(0).map_err(storage)?,
                )?;
                if row.get::<i64>(1).map_err(storage)? <= now
                    || !same_release(&admitted, &command.expected)
                {
                    return Err(AgentFailure::PolicyDenied);
                }
            }
            let mut prior = tx
                .query(
                    "SELECT operation_id FROM gateway_authorization_receipts WHERE operation_id=? OR challenge_id=?",
                    (command.operation_id.to_string(), command.expected.challenge_id.clone()),
                )
                .await
                .map_err(storage)?;
            if prior.next().await.map_err(storage)?.is_some() {
                return Err(AgentFailure::Conflict);
            }
            drop(prior);
            tx.execute(
                "INSERT INTO gateway_authorization_receipts VALUES(?,?,?,?,?,?,?)",
                (
                    command.operation_id.to_string(),
                    command.expected.challenge_id.clone(),
                    hex(&command.request_digest),
                    command.expected.operation.clone(),
                    command.expected.admission_id.clone(),
                    bounded_encode(&command.expected)?,
                    command.expires_at_unix_ms,
                ),
            )
            .await
            .map_err(storage)?;
            let (key, issuer) = self.vault.enrollment_key_on(&tx, authority.operation_id).await?;
            if issuer != owner { return Err(AgentFailure::PolicyDenied); }
            let mut bytes = OWNER_SIGNATURE_DOMAIN.to_vec();
            bytes.extend_from_slice(&command.canonical_bytes);
            Ok(AuthorizationSignature {
                key_id: issuer.key_id,
                signature: URL_SAFE_NO_PAD.encode(key.sign(&bytes).as_ref()),
            })
        }.await;
        let signature = self
            .vault
            .finish_access_grant_transaction(tx, result)
            .await?;
        if self.vault.committed_issuer().await? != owner {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(signature)
    }

    async fn sign_product_authorization(
        &self,
        command: ProductCalendarSigningCommand<'_>,
        supplied_producer: RemoteProducerIdentity,
    ) -> Result<AuthorizationSignature, AgentFailure> {
        let owner = self.vault.committed_issuer().await?;
        let pin = self
            .vault
            .current_pin_record()
            .await?
            .ok_or(AgentFailure::PolicyDenied)?;
        if supplied_producer != pin.producer {
            return Err(AgentFailure::PolicyDenied);
        }
        let verified = self.verifier.verify_product(&command)?;
        verify_product_pin_binding(&verified, &supplied_producer)?;
        verify_producer_signature(
            &supplied_producer,
            &command.canonical_bytes,
            &command.producer_signature,
        )?;
        command
            .validate_for_signing(&verified, self.vault.person_id, &owner.key_id)
            .await?;
        validate_product_result_digest(&verified)?;

        let claims = verified.claims();
        let binding = VerifiedGatewayBinding {
            person_id: claims.person_id.clone(),
            device_id: claims.device_id.clone(),
            client_id: claims.client_id.clone(),
            producer_instance: claims.producer_instance.clone(),
            producer_key_fingerprint: claims.producer_key_fingerprint.clone(),
            producer_audience: claims.audience.clone(),
            enrollment_id: claims.enrollment_id.clone(),
            credential_generation: claims.credential_generation,
        };
        binding.validate()?;
        let runtime_generation = command
            .permit
            .gateway_runtime_generation()
            .filter(|generation| *generation > 0 && *generation <= i64::MAX as u64)
            .ok_or(AgentFailure::PolicyDenied)?;

        let challenge_id = verified.challenge_id();
        let operation = product_operation(&verified);
        let admission_id = product_admission_id(&verified).to_string();
        let expires_at_unix_ms = verified.expires_at_unix_ms();
        let request_digest = hex(&Sha256::digest(&command.canonical_bytes));
        let expected_receipt = ProductAuthorizationReceipt {
            challenge: verified.clone(),
            canonical_bytes_b64url: URL_SAFE_NO_PAD.encode(&command.canonical_bytes),
            producer_signature_b64url: URL_SAFE_NO_PAD.encode(&command.producer_signature),
            producer: supplied_producer.clone(),
            person_id: claims.person_id.clone(),
            device_id: claims.device_id.clone(),
            owner_key_id: owner.key_id.clone(),
            gateway_runtime_generation: runtime_generation,
            signature: AuthorizationSignature {
                key_id: owner.key_id.clone(),
                signature: String::new(),
            },
        };

        let mut connection = self.vault.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            self.vault.check_access()?;
            let now = chrono::Utc::now().timestamp_millis();
            self.vault.advance_clock(&tx, now).await?;
            product_signing_lifetime(&command, &verified, now)?;
            if expires_at_unix_ms <= now {
                return Err(AgentFailure::PolicyDenied);
            }

            let mut pin_rows = tx
                .query(
                    "SELECT identity_json,revision FROM remote_authority_producer WHERE id=1",
                    (),
                )
                .await
                .map_err(storage)?;
            let pin_row = pin_rows
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::PolicyDenied)?;
            let current_producer: RemoteProducerIdentity = bounded_decode(
                &pin_row.get::<String>(0).map_err(storage)?,
            )?;
            let current_pin_revision = pin_row.get::<i64>(1).map_err(storage)?;
            if current_producer != pin.producer
                || current_pin_revision <= 0
                || current_pin_revision as u64 != pin.revision
            {
                return Err(AgentFailure::PolicyDenied);
            }
            validate_producer(&current_producer)?;
            drop(pin_rows);
            let authority = current_credential_in_transaction(&tx, self.vault.person_id).await?;
            if authority.binding != binding || authority.pin.producer != supplied_producer
                || authority.pin.revision != pin.revision || authority.issuer != owner {
                return Err(AgentFailure::PolicyDenied);
            }
            let (owner_key, issuer) = self.vault.enrollment_key_on(&tx, authority.operation_id).await?;
            if issuer != owner { return Err(AgentFailure::PolicyDenied); }

            let mut existing_rows = tx
                .query(
                    "SELECT request_digest,operation,admission_id,person_id,device_id,owner_key_id,gateway_runtime_generation,expires_at_unix_ms,payload FROM gateway_product_authorization_receipts WHERE challenge_id=?",
                    (challenge_id.to_string(),),
                )
                .await
                .map_err(storage)?;
            if let Some(row) = existing_rows.next().await.map_err(storage)? {
                let request_digest_row = row.get::<String>(0).map_err(storage)?;
                let operation_row = row.get::<String>(1).map_err(storage)?;
                let admission_id_row = row.get::<String>(2).map_err(storage)?;
                let person_id_row = row.get::<String>(3).map_err(storage)?;
                let device_id_row = row.get::<String>(4).map_err(storage)?;
                let owner_key_id_row = row.get::<String>(5).map_err(storage)?;
                let generation_row = row.get::<i64>(6).map_err(storage)?;
                let expires_row = row.get::<i64>(7).map_err(storage)?;
                let prior: ProductAuthorizationReceipt =
                    bounded_decode(&row.get::<String>(8).map_err(storage)?)?;
                if request_digest_row != request_digest
                    || operation_row != operation
                    || admission_id_row != admission_id
                    || person_id_row != claims.person_id
                    || device_id_row != claims.device_id
                    || owner_key_id_row != owner.key_id
                    || generation_row <= 0
                    || generation_row as u64 != runtime_generation
                    || expires_row != expires_at_unix_ms
                    || !prior.matches_command(&expected_receipt)
                    || prior.signature.signature.is_empty()
                {
                    return Err(AgentFailure::Conflict);
                }
                verify_owner_receipt_signature(
                    &owner,
                    &command.canonical_bytes,
                    &prior.signature.signature,
                )?;
                decode_exact(&prior.signature.signature, 64)
                    .map_err(|_| AgentFailure::VaultUnavailable)?;
                drop(existing_rows);
                return Ok(prior.signature);
            }
            drop(existing_rows);

            if let ProductCalendarChallenge::Release { admission_id, .. } = &verified {
                let admission_id_text = admission_id.to_string();
                let mut admission_rows = tx
                    .query(
                        "SELECT request_digest,operation,admission_id,person_id,device_id,owner_key_id,gateway_runtime_generation,expires_at_unix_ms,payload FROM gateway_product_authorization_receipts WHERE challenge_id=?",
                        (admission_id_text.clone(),),
                    )
                    .await
                    .map_err(storage)?;
                let row = admission_rows
                    .next()
                    .await
                    .map_err(storage)?
                    .ok_or(AgentFailure::PolicyDenied)?;
                let admission_digest_row = row.get::<String>(0).map_err(storage)?;
                let operation_row = row.get::<String>(1).map_err(storage)?;
                let admission_id_row = row.get::<String>(2).map_err(storage)?;
                let person_id_row = row.get::<String>(3).map_err(storage)?;
                let device_id_row = row.get::<String>(4).map_err(storage)?;
                let owner_key_id_row = row.get::<String>(5).map_err(storage)?;
                let generation_row = row.get::<i64>(6).map_err(storage)?;
                let expires_row = row.get::<i64>(7).map_err(storage)?;
                let admitted: ProductAuthorizationReceipt =
                    bounded_decode(&row.get::<String>(8).map_err(storage)?)?;
                let admitted_bytes = decode_canonical(&admitted.canonical_bytes_b64url, 64 * 1024)
                    .map_err(|_| AgentFailure::VaultUnavailable)?;
                let admitted_challenge: ProductCalendarChallenge =
                    serde_json::from_slice(&admitted_bytes)
                        .map_err(|_| AgentFailure::VaultUnavailable)?;
                if hex(&Sha256::digest(&admitted_bytes)) != admission_digest_row
                    || admitted_challenge != admitted.challenge
                {
                    return Err(AgentFailure::VaultUnavailable);
                }
                if operation_row != "day_calendar_admission"
                    || admission_id_row != admission_id_text
                    || person_id_row != claims.person_id
                    || device_id_row != claims.device_id
                    || owner_key_id_row != owner.key_id
                    || generation_row <= 0
                    || generation_row as u64 != runtime_generation
                    || expires_row <= now
                    || admitted.gateway_runtime_generation != runtime_generation
                    || admitted.owner_key_id != owner.key_id
                    || admitted.producer != pin.producer
                    || admitted.person_id != claims.person_id
                    || admitted.device_id != claims.device_id
                    || admitted.signature.key_id != owner.key_id
                    || !matches!(
                        &admitted.challenge,
                        ProductCalendarChallenge::Admission { challenge_id: id, .. }
                            if id.to_string() == admission_id_text
                    )
                    || admitted.challenge.claims() != claims
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                let admitted_producer_signature =
                    decode_exact(&admitted.producer_signature_b64url, 64)
                        .map_err(|_| AgentFailure::VaultUnavailable)?;
                verify_producer_signature(
                    &admitted.producer,
                    &admitted_bytes,
                    &admitted_producer_signature,
                )?;
                verify_owner_receipt_signature(
                    &owner,
                    &admitted_bytes,
                    &admitted.signature.signature,
                )?;
                drop(admission_rows);

                let mut released_rows = tx
                    .query(
                        "SELECT challenge_id FROM gateway_product_authorization_receipts WHERE operation='day_calendar_release' AND admission_id=? LIMIT 1",
                        (admission_id_text,),
                    )
                    .await
                    .map_err(storage)?;
                if released_rows.next().await.map_err(storage)?.is_some() {
                    return Err(AgentFailure::Conflict);
                }
            }

            let mut receipt = expected_receipt.clone();
            let mut signed_bytes =
                Vec::with_capacity(OWNER_SIGNATURE_DOMAIN.len() + command.canonical_bytes.len());
            signed_bytes.extend_from_slice(OWNER_SIGNATURE_DOMAIN);
            signed_bytes.extend_from_slice(&command.canonical_bytes);
            receipt.signature.signature =
                URL_SAFE_NO_PAD.encode(owner_key.sign(&signed_bytes).as_ref());
            let payload = bounded_encode(&receipt)?;
            tx.execute(
                "INSERT INTO gateway_product_authorization_receipts(challenge_id,request_digest,operation,admission_id,person_id,device_id,owner_key_id,gateway_runtime_generation,expires_at_unix_ms,payload) VALUES(?,?,?,?,?,?,?,?,?,?)",
                (
                    challenge_id.to_string(),
                    request_digest,
                    operation,
                    admission_id,
                    claims.person_id.clone(),
                    claims.device_id.clone(),
                    owner.key_id.clone(),
                    runtime_generation as i64,
                    expires_at_unix_ms,
                    payload,
                ),
            )
            .await
            .map_err(storage)?;
            Ok(receipt.signature)
        }
        .await;
        let signature = self
            .vault
            .finish_access_grant_transaction(tx, result)
            .await?;
        let current_owner = self.vault.committed_issuer().await?;
        if current_owner != owner {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(signature)
    }
}
impl<K: VaultKeyProvider> AuthorizationSigner for VaultAuthorizationSigner<K> {
    fn sign_authorization<'a>(
        &'a self,
        command: AuthorizationSigningCommand<'a>,
    ) -> BoxFuture<'a, Result<AuthorizationSignature, AgentFailure>> {
        Box::pin(async move {
            match command {
                AuthorizationSigningCommand::AssistantView(command) => {
                    self.sign_assistant_authorization(command).await
                }
                AuthorizationSigningCommand::DayCalendarRefresh(command) => {
                    let supplied_producer = command.producer.clone();
                    self.sign_product_authorization(command, supplied_producer)
                        .await
                }
            }
        })
    }
}
fn same_release(
    admitted: &floe_access::RemoteViewAuthorizationExpectation,
    released: &floe_access::RemoteViewAuthorizationExpectation,
) -> bool {
    admitted.operation == "admission"
        && released.operation == "release"
        && admitted.challenge_id == released.admission_id
        && admitted.result_sha256.is_empty()
        && admitted.client_id == released.client_id
        && admitted.device_id == released.device_id
        && admitted.query_sha256 == released.query_sha256
        && admitted.grant_id == released.grant_id
        && admitted.grant_incarnation == released.grant_incarnation
        && admitted.grant_epoch == released.grant_epoch
        && admitted.source_connector == released.source_connector
        && admitted.source_connection == released.source_connection
        && admitted.source_execution_owner == released.source_execution_owner
        && admitted.source_incarnation == released.source_incarnation
        && admitted.source_epoch == released.source_epoch
        && admitted.resources == released.resources
        && admitted.max_items == released.max_items
        && admitted.max_bytes == released.max_bytes
}

fn product_operation(challenge: &ProductCalendarChallenge) -> &'static str {
    match challenge {
        ProductCalendarChallenge::Admission { .. } => "day_calendar_admission",
        ProductCalendarChallenge::Release { .. } => "day_calendar_release",
    }
}

fn product_admission_id(challenge: &ProductCalendarChallenge) -> Uuid {
    match challenge {
        ProductCalendarChallenge::Admission { challenge_id, .. } => *challenge_id,
        ProductCalendarChallenge::Release { admission_id, .. } => *admission_id,
    }
}

fn validate_product_result_digest(
    challenge: &ProductCalendarChallenge,
) -> Result<(), AgentFailure> {
    if let ProductCalendarChallenge::Release { result_sha256, .. } = challenge {
        if result_sha256.len() != 64
            || !result_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || result_sha256.bytes().all(|byte| byte == b'0')
        {
            return Err(AgentFailure::PolicyDenied);
        }
    }
    Ok(())
}

fn verify_product_pin_binding(
    challenge: &ProductCalendarChallenge,
    producer: &RemoteProducerIdentity,
) -> Result<(), AgentFailure> {
    validate_producer(producer)?;
    let claims = challenge.claims();
    if claims.producer_instance != producer.instance_id
        || claims.producer_key_fingerprint != producer.fingerprint
        || claims.audience != producer.audience
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn product_signing_lifetime(
    command: &ProductCalendarSigningCommand<'_>,
    challenge: &ProductCalendarChallenge,
    now_unix_ms: i64,
) -> Result<(), AgentFailure> {
    let now = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(now_unix_ms)
        .ok_or(AgentFailure::InvalidInput)?;
    command.permit.check_lifetime(now)?;
    if command.scope.cancellation().is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if tokio::time::Instant::now() >= command.scope.deadline() {
        return Err(AgentFailure::DeadlineExceeded);
    }
    challenge.validate_permit(command.permit, now)
}

fn verify_owner_receipt_signature(
    owner: &floe_access::RemoteOwnerPublicKey,
    canonical_bytes: &[u8],
    encoded_signature: &str,
) -> Result<(), AgentFailure> {
    let public_key =
        decode_exact(&owner.public_key, 32).map_err(|_| AgentFailure::VaultUnavailable)?;
    let proof = decode_exact(encoded_signature, 64).map_err(|_| AgentFailure::VaultUnavailable)?;
    let mut message = Vec::with_capacity(OWNER_SIGNATURE_DOMAIN.len() + canonical_bytes.len());
    message.extend_from_slice(OWNER_SIGNATURE_DOMAIN);
    message.extend_from_slice(canonical_bytes);
    signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
        .verify(&message, &proof)
        .map_err(|_| AgentFailure::VaultUnavailable)
}

struct CommittedAuthority {
    operation_id: Uuid,
    issuer: floe_access::RemoteOwnerPublicKey,
    binding: VerifiedGatewayBinding,
    pin: GatewayPin,
}
async fn current_credential_in_transaction(
    tx: &Transaction<'_>,
    person: floe_kernel::PersonId,
) -> Result<CommittedAuthority, AgentFailure> {
    let mut pins = tx
        .query(
            "SELECT identity_json,revision FROM remote_authority_producer WHERE id=1",
            (),
        )
        .await
        .map_err(storage)?;
    let row = pins
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::PolicyDenied)?;
    let producer: RemoteProducerIdentity = bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
    let pin_revision = row.get::<i64>(1).map_err(storage)?;
    if pin_revision <= 0 {
        return Err(AgentFailure::PolicyDenied);
    }
    validate_producer(&producer)?;
    drop(pins);
    let mut expectation_rows = tx
        .query(
            "SELECT payload FROM gateway_credential_expectation WHERE id=1",
            (),
        )
        .await
        .map_err(storage)?;
    let expectation: floe_access::GatewayCredentialExpectation = bounded_decode(
        &expectation_rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::PolicyDenied)?
            .get::<String>(0)
            .map_err(storage)?,
    )?;
    let (operation_id, generation) = match expectation {
        floe_access::GatewayCredentialExpectation::Committed {
            operation_id,
            generation,
        } if generation > 0 => (operation_id, generation),
        _ => return Err(AgentFailure::PolicyDenied),
    };
    drop(expectation_rows);
    let owner = enrollment_issuer_on(tx, operation_id).await?;

    let mut pairing_rows = tx
        .query(
            "SELECT revision,state,payload FROM gateway_pairing_operations WHERE operation_id=? AND person_id=?",
            (operation_id.to_string(), person.to_string()),
        )
        .await
        .map_err(storage)?;
    let pairing_row = pairing_rows
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::PolicyDenied)?;
    let revision = pairing_row.get::<i64>(0).map_err(storage)?;
    let state = pairing_row.get::<String>(1).map_err(storage)?;
    let pairing: PairingRecord = bounded_decode(&pairing_row.get::<String>(2).map_err(storage)?)?;
    if revision <= 0
        || revision as u64 != pairing.revision
        || state != pairing_state(pairing.state)
        || pairing.operation_id != operation_id
        || pairing.person_id != person
        || pairing.state != PairingState::Paired
        || pairing.cancellation_command.is_some()
        || pairing.forgotten_command.is_some()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    pairing.validate().map_err(|_| AgentFailure::PolicyDenied)?;
    let enrollment = pairing
        .enrollment
        .as_ref()
        .ok_or(AgentFailure::PolicyDenied)?;
    let expected = &enrollment.binding;
    enrollment
        .binding
        .validate()
        .map_err(|_| AgentFailure::PolicyDenied)?;
    let gateway = pairing.gateway.as_ref().ok_or(AgentFailure::PolicyDenied)?;
    if enrollment.operation_id != operation_id
        || expected.person_id != person.to_string()
        || expected.device_id != pairing.device_id
        || expected.credential_generation != generation
        || expected.producer_instance != producer.instance_id
        || expected.producer_key_fingerprint != producer.fingerprint
        || expected.producer_audience != producer.audience
        || enrollment.issuer.key_id != owner.key_id
        || enrollment.issuer.public_key != owner.public_key
        || enrollment.pin_revision != pin_revision as u64
        || gateway.gateway_ref != operation_id
        || gateway.revision != generation
        || gateway.state != GatewayState::Paired
    {
        return Err(AgentFailure::PolicyDenied);
    }
    drop(pairing_rows);

    let mut enrollment_rows = tx
        .query(
            "SELECT command_json FROM gateway_enrollment_receipts WHERE operation_id=?",
            (operation_id.to_string(),),
        )
        .await
        .map_err(storage)?;
    let enrollment_command: EnrollmentSigningCommand = bounded_decode(
        &enrollment_rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::PolicyDenied)?
            .get::<String>(0)
            .map_err(storage)?,
    )?;
    if enrollment_command.operation_id != operation_id
        || enrollment_command.person_id.to_string() != expected.person_id
        || enrollment_command.device_id != expected.device_id
        || enrollment_command.client_id != expected.client_id
        || enrollment_command.issuer.key_id != owner.key_id
        || enrollment_command.issuer.public_key != owner.public_key
        || enrollment_command.producer != producer
        || enrollment_command.challenge_id.is_nil()
        || enrollment_command.challenge_id.to_string() != expected.enrollment_id
        || enrollment_command.canonical_bytes.is_empty()
        || enrollment_command.producer_signature.len() != 64
        || enrollment_command.request_digest
            != <[u8; 32]>::from(Sha256::digest(&enrollment_command.canonical_bytes))
    {
        return Err(AgentFailure::PolicyDenied);
    }
    verify_producer_signature(
        &enrollment_command.producer,
        &enrollment_command.canonical_bytes,
        &enrollment_command.producer_signature,
    )?;
    drop(enrollment_rows);
    let mut private = tx.query("SELECT enrollment_json, length(credential) FROM gateway_pairing_private WHERE operation_id=?", (operation_id.to_string(),)).await.map_err(storage)?;
    let row = private
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::PolicyDenied)?;
    let admitted: EnrollmentSigningCommand = bounded_decode(
        &row.get::<Option<String>>(0)
            .map_err(storage)?
            .ok_or(AgentFailure::PolicyDenied)?,
    )?;
    let length = row
        .get::<Option<i64>>(1)
        .map_err(storage)?
        .ok_or(AgentFailure::PolicyDenied)?;
    if admitted != enrollment_command || !(32..=256).contains(&length) {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(CommittedAuthority {
        operation_id,
        issuer: owner,
        binding: expected.clone(),
        pin: GatewayPin {
            producer,
            revision: pin_revision as u64,
        },
    })
}

pub(super) async fn command_rejection_on(
    connection: &turso::Connection,
    person: floe_kernel::PersonId,
    command_id: Uuid,
) -> Result<Option<ConnectionsCommandRejection>, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT payload FROM connections_command_rejections WHERE person_id=? AND command_id=?",
            (person.to_string(), command_id.to_string()),
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    let rejection: ConnectionsCommandRejection =
        bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
    rejection.identity.validate()?;
    if rejection.identity.person_id != person
        || rejection.identity.command_id != command_id
        || rejection.identity.journal != ConnectionsCommandJournal::Product
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(rejection))
}

impl<K: VaultKeyProvider> ConnectionsProductRepository for EncryptedAgentVault<K> {
    fn rejected_command<'a>(
        &'a self,
        identity: ConnectionsCommandIdentity,
    ) -> BoxFuture<'a, Result<Option<AgentFailure>, AgentFailure>> {
        Box::pin(async move {
            identity.validate()?;
            if identity.person_id != self.person_id
                || identity.journal != ConnectionsCommandJournal::Product
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let rejection =
                command_rejection_on(&self.connection()?, identity.person_id, identity.command_id)
                    .await?;
            self.check_access()?;
            match rejection {
                Some(rejection) if rejection.identity == identity => Ok(Some(rejection.reason)),
                Some(_) => Err(AgentFailure::Conflict),
                None => Ok(None),
            }
        })
    }
    fn reject_unadmitted_command<'a>(
        &'a self,
        identity: ConnectionsCommandIdentity,
        reason: AgentFailure,
    ) -> BoxFuture<'a, Result<ConnectionsCommandResolution, AgentFailure>> {
        Box::pin(async move {
            identity.validate()?;
            if identity.person_id != self.person_id
                || identity.journal != ConnectionsCommandJournal::Product
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let mut connection = self.connection()?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(product_begin)?;
            let result = async {
                let mut rows = tx.query("SELECT payload FROM connections_product_records WHERE record_ref=? OR (person_id=? AND command_id=?)",
                    (identity.record_ref.to_string(), identity.person_id.to_string(), identity.command_id.to_string())).await.map_err(storage)?;
                if let Some(row) = rows.next().await.map_err(storage)? {
                    let record: ConnectionsRecord = bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
                    record.validate()?;
                    if !identity.matches(&record) { return Err(AgentFailure::Conflict); }
                    return Ok(ConnectionsCommandResolution::Admitted);
                }
                drop(rows);
                let mut reviews = tx.query("SELECT review_id FROM access_connection_reviews WHERE person_id=? AND command_id=?",
                    (identity.person_id.to_string(), identity.command_id.to_string())).await.map_err(storage)?;
                if reviews.next().await.map_err(storage)?.is_some() {
                    return Ok(ConnectionsCommandResolution::Admitted);
                }
                drop(reviews);
                if let Some(rejection) = command_rejection_on(&tx, identity.person_id, identity.command_id).await? {
                    if rejection.identity != identity { return Err(AgentFailure::Conflict); }
                    return Ok(ConnectionsCommandResolution::NotApplied(rejection.reason));
                }
                let rejection = ConnectionsCommandRejection { identity, reason };
                tx.execute("INSERT INTO connections_command_rejections VALUES(?,?,?)", (
                    rejection.identity.person_id.to_string(), rejection.identity.command_id.to_string(), bounded_encode(&rejection)?,
                )).await.map_err(storage)?;
                self.check_access()?;
                Ok(ConnectionsCommandResolution::NotApplied(reason))
            }.await;
            self.finish_access_grant_transaction(tx, result).await
        })
    }
    fn load<'a>(
        &'a self,
        person: floe_kernel::PersonId,
        id: Uuid,
    ) -> BoxFuture<'a, Result<Option<ConnectionsRecord>, AgentFailure>> {
        Box::pin(async move {
            if person != self.person_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let mut rows=self.connection()?.query("SELECT revision,payload FROM connections_product_records WHERE record_ref=? AND person_id=?",(id.to_string(),person.to_string())).await.map_err(storage)?;
            let Some(row) = rows.next().await.map_err(storage)? else {
                return Ok(None);
            };
            let record: ConnectionsRecord =
                bounded_decode(&row.get::<String>(1).map_err(storage)?)?;
            record.validate()?;
            if record.record_ref != id
                || record.person_id != person
                || record.revision as i64 != row.get::<i64>(0).map_err(storage)?
            {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(Some(record))
        })
    }
    fn list_page<'a>(
        &'a self,
        person: floe_kernel::PersonId,
        after: Option<Uuid>,
        limit: usize,
    ) -> BoxFuture<'a, Result<ConnectionsRecordPage, AgentFailure>> {
        Box::pin(async move {
            if person != self.person_id || limit == 0 || limit > 1024 {
                return Err(AgentFailure::InvalidInput);
            }
            let mut rows=self.connection()?.query("SELECT record_ref,revision,payload FROM connections_product_records WHERE person_id=? AND record_ref>? ORDER BY record_ref LIMIT ?",(person.to_string(),after.map(|id| id.to_string()).unwrap_or_default(),(limit + 1) as i64)).await.map_err(storage)?;
            let mut records = Vec::new();
            while let Some(row) = rows.next().await.map_err(storage)? {
                let record: ConnectionsRecord =
                    bounded_decode(&row.get::<String>(2).map_err(storage)?)?;
                record.validate()?;
                if record.record_ref.to_string() != row.get::<String>(0).map_err(storage)?
                    || record.revision as i64 != row.get::<i64>(1).map_err(storage)?
                    || record.person_id != person
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                records.push(record)
            }
            let next_after = if records.len() > limit {
                records.pop();
                records.last().map(|record| record.record_ref)
            } else {
                None
            };
            Ok(ConnectionsRecordPage {
                records,
                next_after,
            })
        })
    }
    fn admit_cancellation<'a>(
        &'a self,
        receipt: ConnectionsRecord,
    ) -> BoxFuture<'a, Result<ConnectionsRecord, AgentFailure>> {
        Box::pin(async move {
            receipt.validate()?;
            if receipt.person_id != self.person_id || receipt.revision != 1 {
                return Err(AgentFailure::PolicyDenied);
            }
            let ConnectionsPayload::CancellationIntent { operation_ref, .. } = receipt.payload
            else {
                return Err(AgentFailure::InvalidInput);
            };
            let mut connection = self.connection()?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(product_begin)?;
            let result=async {
                if command_rejection_on(&tx,receipt.person_id,receipt.command_id).await?.is_some() { return Err(AgentFailure::Conflict); }
                let mut rows=tx.query("SELECT payload FROM connections_product_records WHERE record_ref=? OR (person_id=? AND command_id=?)",(receipt.record_ref.to_string(),receipt.person_id.to_string(),receipt.command_id.to_string())).await.map_err(storage)?;
                if let Some(row)=rows.next().await.map_err(storage)? {
                    let existing:ConnectionsRecord=bounded_decode(&row.get::<String>(0).map_err(storage)?)?;existing.validate()?;
                    if existing.record_ref!=receipt.record_ref || existing.person_id!=receipt.person_id || existing.device_id!=receipt.device_id || existing.command_id!=receipt.command_id || existing.intent_digest!=receipt.intent_digest {return Err(AgentFailure::Conflict);}
                    return Ok(existing);
                }
                drop(rows);
                let mut rows=tx.query("SELECT payload FROM connections_product_records WHERE record_ref=? AND person_id=?",(operation_ref.to_string(),receipt.person_id.to_string())).await.map_err(storage)?;
                let target:ConnectionsRecord=bounded_decode(&rows.next().await.map_err(storage)?.ok_or(AgentFailure::NotFound)?.get::<String>(0).map_err(storage)?)?;drop(rows);
                let next=target.with_cancellation(&receipt)?;
                let changed=tx.execute("UPDATE connections_product_records SET revision=?,payload=? WHERE record_ref=? AND revision=?",(next.revision as i64,bounded_encode(&next)?,operation_ref.to_string(),target.revision as i64)).await.map_err(storage)?;
                if changed!=1 {return Err(AgentFailure::Conflict);}
                tx.execute("INSERT INTO connections_product_records VALUES(?,?,?,?,?)",(receipt.record_ref.to_string(),receipt.person_id.to_string(),receipt.command_id.to_string(),receipt.revision as i64,bounded_encode(&receipt)?)).await.map_err(storage)?;
                self.check_access()?;Ok(receipt)
            }.await;
            self.finish_access_grant_transaction(tx, result).await
        })
    }
    fn insert<'a>(
        &'a self,
        record: ConnectionsRecord,
    ) -> BoxFuture<'a, Result<ConnectionsRecord, AgentFailure>> {
        Box::pin(async move {
            record.validate()?;
            if record.person_id != self.person_id || record.revision != 1 {
                return Err(AgentFailure::PolicyDenied);
            }
            let mut connection = self.connection()?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(product_begin)?;
            let result=async{
            if command_rejection_on(&tx, record.person_id, record.command_id).await?.is_some() {
                return Err(AgentFailure::Conflict);
            }
            let mut rows=tx.query("SELECT payload FROM connections_product_records WHERE record_ref=? OR (person_id=? AND command_id=?)",(record.record_ref.to_string(),record.person_id.to_string(),record.command_id.to_string())).await.map_err(storage)?;
            if let Some(row)=rows.next().await.map_err(storage)?{let current:ConnectionsRecord=bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
                if current.record_ref!=record.record_ref||current.command_id!=record.command_id||current.intent_digest!=record.intent_digest||current.person_id!=record.person_id||current.device_id!=record.device_id{return Err(AgentFailure::Conflict)}return Ok(current)}drop(rows);
            if matches!(record.payload, ConnectionsPayload::GatewayForgotten(_) | ConnectionsPayload::CancellationIntent { .. }) {
                return Err(AgentFailure::PolicyDenied);
            }
            tx.execute("INSERT INTO connections_product_records VALUES(?,?,?,?,?)",(record.record_ref.to_string(),record.person_id.to_string(),record.command_id.to_string(),record.revision as i64,bounded_encode(&record)?)).await.map_err(storage)?;Ok(record)
        }.await;
            self.finish_access_grant_transaction(tx, result).await
        })
    }
    fn compare_and_swap<'a>(
        &'a self,
        expected_revision: u64,
        record: ConnectionsRecord,
    ) -> BoxFuture<'a, Result<ConnectionsRecord, AgentFailure>> {
        Box::pin(async move {
            record.validate()?;
            if record.person_id != self.person_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let mut connection = self.connection()?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(product_begin)?;
            let result=async{
            let mut rows=tx.query("SELECT payload FROM connections_product_records WHERE record_ref=? AND person_id=?",(record.record_ref.to_string(),record.person_id.to_string())).await.map_err(storage)?;
            let current:ConnectionsRecord=bounded_decode(&rows.next().await.map_err(storage)?.ok_or(AgentFailure::Conflict)?.get::<String>(0).map_err(storage)?)?;drop(rows);
            if current.revision!=expected_revision{return Err(AgentFailure::Conflict)}current.validate_successor(&record)?;
            let changed=tx.execute("UPDATE connections_product_records SET revision=?,payload=? WHERE record_ref=? AND revision=?",(record.revision as i64,bounded_encode(&record)?,record.record_ref.to_string(),expected_revision as i64)).await.map_err(storage)?;
            if changed!=1{return Err(AgentFailure::Conflict)}Ok(record)
        }.await;
            self.finish_access_grant_transaction(tx, result).await
        })
    }
}

fn product_begin(error: turso::Error) -> AgentFailure {
    match error {
        turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => AgentFailure::Conflict,
        other => storage(other),
    }
}
