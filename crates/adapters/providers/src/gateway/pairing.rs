//! Pairing wire secrets live only in the single OS-protected adapter record.
use super::{
    credentials::{
        GatewayConnection, GatewayCredentialError, GatewayCredentialStore, SetupStorageStage,
        read_record, setup_failure, valid_token,
    },
    http::GatewayHttpTransport,
    proof,
};
use floe_access::{RemoteOwnerPublicKey, RemoteProducerIdentity, VerifiedGatewayBinding};
use floe_agent_contract::{AgentFailure, BoxFuture};
use floe_connections::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StagedPairing {
    target_ref: Uuid,
    address: String,
    setup_expires: i64,
    polling_proof: String,
    request: Option<StartIdentity>,
    challenge: Option<StartWire>,
    reviewed: Option<ReviewedGatewayIdentity>,
    locally_confirmed: bool,
    approved_token: Option<String>,
    cancelled: bool,
}
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartIdentity {
    operation_id: Uuid,
    person_id: String,
    device_id: String,
    generation: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IssuerWire {
    key_id: String,
    public_key: String,
    fingerprint: String,
}
impl IssuerWire {
    fn key(&self) -> RemoteOwnerPublicKey {
        RemoteOwnerPublicKey {
            key_id: self.key_id.clone(),
            public_key: self.public_key.clone(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartWire {
    schema_version: u32,
    pairing_id: String,
    code: String,
    proof: String,
    expires_at_unix_ms: i64,
    person_id: String,
    device_id: String,
    producer: RemoteProducerIdentity,
    issuer: IssuerWire,
    challenge_id: String,
    challenge_b64url: String,
    producer_signature: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusWire {
    schema_version: u32,
    pairing_id: String,
    status: String,
    person_id: String,
    device_id: String,
    #[serde(default)]
    producer: Option<RemoteProducerIdentity>,
    #[serde(default)]
    issuer: Option<IssuerWire>,
    #[serde(default)]
    issuer_fingerprint: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    token: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmationWire {
    schema_version: u32,
    pairing_id: String,
    status: String,
}
#[derive(Clone)]
pub struct GatewayPairingAdapter {
    store: GatewayCredentialStore,
    signer: Arc<dyn EnrollmentSigner>,
    authority: Arc<dyn GatewayAuthorityRepository>,
}
impl GatewayPairingAdapter {
    pub fn new(
        store: GatewayCredentialStore,
        signer: Arc<dyn EnrollmentSigner>,
        authority: Arc<dyn GatewayAuthorityRepository>,
    ) -> Self {
        Self {
            store,
            signer,
            authority,
        }
    }
    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        address: &str,
        path: &str,
        body: serde_json::Value,
        scope: &OperationScope,
    ) -> Result<T, PairingError> {
        let http = GatewayHttpTransport::new().map_err(pairing_failure)?;
        let body = serde_json::to_vec(&body).map_err(|_| PairingError::InvalidInput)?;
        let (status, bytes) = http
            .request(
                address,
                None,
                reqwest::Method::POST,
                path,
                Some(body),
                scope.deadline(),
                scope.cancellation(),
            )
            .await
            .map_err(pairing_failure)?;
        super::json::strict_json_bytes(&bytes, 65536).map_err(|_| PairingError::Rejected)?;
        if status != 200 {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct ErrorCode {
                code: String,
            }
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct ErrorWire {
                error: ErrorCode,
            }
            let wire: ErrorWire =
                serde_json::from_slice(&bytes).map_err(|_| PairingError::Rejected)?;
            if status == 409 && wire.error.code == "pairing_repair_required" {
                return Err(PairingError::RepairRequired);
            }
            return Err(match status {
                400 => PairingError::InvalidInput,
                401 | 403 => PairingError::Rejected,
                409 => PairingError::Conflict,
                _ => PairingError::TransportUnavailable,
            });
        }
        serde_json::from_slice(&bytes).map_err(|_| PairingError::Rejected)
    }
    async fn stage(&self, handle: &PairingHandle) -> Result<StagedPairing, PairingError> {
        let stage = read_record()
            .await
            .map_err(credential_failure)?
            .staging
            .ok_or(PairingError::Conflict)?;
        let request = stage.request.as_ref().ok_or(PairingError::Conflict)?;
        let challenge = stage
            .challenge
            .as_ref()
            .ok_or(PairingError::TransportUnavailable)?;
        if request.operation_id != handle.operation_id
            || request.generation != handle.generation
            || challenge.pairing_id != handle.attempt_id.to_string()
        {
            return Err(PairingError::Conflict);
        }
        Ok(stage)
    }
    fn public_challenge(stage: &StagedPairing) -> Result<PairingChallenge, PairingError> {
        let request = stage.request.as_ref().ok_or(PairingError::Conflict)?;
        let wire = stage
            .challenge
            .as_ref()
            .ok_or(PairingError::TransportUnavailable)?;
        Ok(PairingChallenge {
            handle: PairingHandle {
                operation_id: request.operation_id,
                attempt_id: Uuid::parse_str(&wire.pairing_id)
                    .map_err(|_| PairingError::Rejected)?,
                generation: request.generation,
            },
            display_code: wire.code.clone(),
            expires_at_unix_ms: wire.expires_at_unix_ms,
            reviewed: stage.reviewed.clone().ok_or(PairingError::Conflict)?,
        })
    }
    fn signing_command(
        stage: &StagedPairing,
        enforce_expiry: bool,
    ) -> Result<EnrollmentSigningCommand, PairingError> {
        let request = stage.request.as_ref().ok_or(PairingError::Conflict)?;
        let wire = stage.challenge.as_ref().ok_or(PairingError::Conflict)?;
        let bytes =
            proof::decode_canonical(&wire.challenge_b64url, 65536).map_err(pairing_failure)?;
        let signature =
            proof::decode_exact(&wire.producer_signature, 64).map_err(pairing_failure)?;
        let challenge = proof::parse_challenge(&bytes).map_err(pairing_failure)?;
        proof::verify_signature(&wire.producer, &bytes, &signature).map_err(pairing_failure)?;
        let now = chrono::Utc::now().timestamp_millis();
        if challenge.operation != "enrollment"
            || challenge.purpose != "owner_enrollment"
            || challenge._consumer != "owner"
            || challenge.person_id != request.person_id
            || challenge.device_id != request.device_id
            || challenge.client_id != wire.pairing_id
            || challenge.key_id != wire.issuer.key_id
            || challenge.audience != wire.producer.audience
            || challenge.challenge_id != wire.challenge_id
            || challenge.expires_at_unix_ms != wire.expires_at_unix_ms
            || (enforce_expiry && challenge.expires_at_unix_ms <= now)
            || challenge.issued_at_unix_ms > now.saturating_add(5000)
        {
            return Err(PairingError::ChangedProducer);
        }
        let person_uuid =
            Uuid::parse_str(&request.person_id).map_err(|_| PairingError::ForeignIdentity)?;
        Ok(EnrollmentSigningCommand {
            operation_id: request.operation_id,
            request_digest: Sha256::digest(&bytes).into(),
            person_id: floe_kernel::PersonId(person_uuid),
            device_id: request.device_id.clone(),
            client_id: wire.pairing_id.clone(),
            issuer: wire.issuer.key(),
            producer: wire.producer.clone(),
            challenge_id: Uuid::parse_str(&wire.challenge_id)
                .map_err(|_| PairingError::Rejected)?,
            canonical_bytes: bytes,
            producer_signature: signature,
            issued_at_unix_ms: challenge.issued_at_unix_ms,
            expires_at_unix_ms: challenge.expires_at_unix_ms,
        })
    }
    async fn poll(
        &self,
        handle: &PairingHandle,
        scope: &OperationScope,
    ) -> Result<PairingObservation, PairingError> {
        let stage = self.stage(handle).await?;
        let wire = stage.challenge.as_ref().ok_or(PairingError::Conflict)?;
        let reviewed = stage.reviewed.clone().ok_or(PairingError::Conflict)?;
        if stage.cancelled {
            return Ok(PairingObservation {
                handle: handle.clone(),
                progress: PairingProgress::Cancelled,
                reviewed,
            });
        }
        let status:StatusWire=match self.request(&stage.address,"/pair/poll",serde_json::json!({"schema_version":1,"pairing_id":wire.pairing_id,"proof":wire.proof}),scope).await{
            Ok(status)=>status,Err(PairingError::RepairRequired)=>return Ok(PairingObservation{handle:handle.clone(),progress:PairingProgress::RepairRequired,reviewed}),Err(error)=>return Err(error),
        };
        if status.schema_version != 1
            || status.pairing_id != wire.pairing_id
            || status.person_id != wire.person_id
            || status.device_id != wire.device_id
        {
            return Err(PairingError::ForeignIdentity);
        }
        let progress = match status.status.as_str() {
            "pending" => PairingProgress::AwaitingLocalConfirmation,
            "local_confirmed" => PairingProgress::AwaitingApproval,
            "rejected" => PairingProgress::Rejected,
            "expired" => PairingProgress::Expired,
            "cancelled" => PairingProgress::Cancelled,
            "repair_required" => PairingProgress::RepairRequired,
            "approved" => {
                if status.producer.as_ref() != Some(&wire.producer)
                    || status.issuer.as_ref().map(IssuerWire::key) != Some(wire.issuer.key())
                    || status.issuer_fingerprint.as_deref() != Some(&wire.issuer.fingerprint)
                    || status.client_id.as_deref() != Some(&wire.pairing_id)
                    || status
                        .issuer
                        .as_ref()
                        .is_none_or(|issuer| issuer.fingerprint != wire.issuer.fingerprint)
                    || status
                        .token
                        .as_deref()
                        .is_none_or(|token| !valid_token(token))
                {
                    return Err(PairingError::ChangedProducer);
                }
                let signature = self
                    .signer
                    .readback(handle.operation_id)
                    .await?
                    .ok_or(PairingError::Conflict)?;
                if signature.key_id != wire.issuer.key_id
                    || signature.request_digest
                        != <[u8; 32]>::from(Sha256::digest(
                            proof::decode_canonical(&wire.challenge_b64url, 65536)
                                .map_err(pairing_failure)?,
                        ))
                {
                    return Err(PairingError::Conflict);
                }
                self.store
                    .mutate(|record| {
                        let current = record
                            .staging
                            .as_mut()
                            .ok_or(GatewayCredentialError::Conflict)?;
                        if current.request != stage.request {
                            return Err(GatewayCredentialError::Conflict);
                        }
                        current.approved_token = status.token.clone();
                        current.locally_confirmed = true;
                        Ok(())
                    })
                    .await
                    .map_err(credential_failure)?;
                PairingProgress::Approved
            }
            _ => return Err(PairingError::Rejected),
        };
        if progress != PairingProgress::Approved
            && (status.token.is_some()
                || status.client_id.is_some()
                || status.producer.is_some()
                || status.issuer.is_some()
                || status.issuer_fingerprint.is_some())
        {
            return Err(PairingError::Rejected);
        }
        Ok(PairingObservation {
            handle: handle.clone(),
            progress,
            reviewed,
        })
    }
}
impl GatewayPairingPort for GatewayPairingAdapter {
    fn prepare_setup<'a>(
        &'a self,
        command_id: Uuid,
        address: &'a str,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<GatewaySetup, PairingError>> {
        Box::pin(async move {
            if scope.cancellation().is_cancelled() {
                return Err(PairingError::Cancelled);
            }
            if scope.deadline() <= tokio::time::Instant::now() {
                return Err(PairingError::DeadlineExceeded);
            }
            if command_id.is_nil() || address.len() > 2048 || !super::http::valid_endpoint(address)
            {
                return Err(PairingError::InvalidInput);
            }
            let address = address.trim_end_matches('/').to_owned();
            let expectation = self.store.expectation().await.map_err(|failure| {
                credential_failure(setup_failure(
                    Some(command_id),
                    SetupStorageStage::Expectation,
                    failure,
                ))
            })?;
            self.store
                .mutate_setup(command_id, |record| {
                    if let Some(stage) = &record.staging {
                        if stage.target_ref == command_id {
                            if stage.address != address {
                                return Err(GatewayCredentialError::Conflict);
                            }
                            return Ok(GatewaySetup {
                                target_ref: command_id,
                                display_address: address.clone(),
                                expires_at: chrono::DateTime::from_timestamp_millis(
                                    stage.setup_expires,
                                )
                                .ok_or(GatewayCredentialError::Malformed)?,
                            });
                        }
                        if stage.request.is_some()
                            && !stage.cancelled
                            && stage.approved_token.is_none()
                        {
                            return Err(GatewayCredentialError::Conflict);
                        }
                    }
                    if !matches!(
                        expectation,
                        floe_access::GatewayCredentialExpectation::Unpaired
                            | floe_access::GatewayCredentialExpectation::Forgotten { .. }
                    ) {
                        return Err(GatewayCredentialError::Conflict);
                    }
                    let expires = chrono::Utc::now().timestamp_millis() + 300_000;
                    record.staging = Some(StagedPairing {
                        target_ref: command_id,
                        address: address.clone(),
                        setup_expires: expires,
                        polling_proof: new_pairing_proof()?,
                        request: None,
                        challenge: None,
                        reviewed: None,
                        locally_confirmed: false,
                        approved_token: None,
                        cancelled: false,
                    });
                    Ok(GatewaySetup {
                        target_ref: command_id,
                        display_address: address.clone(),
                        expires_at: chrono::DateTime::from_timestamp_millis(expires)
                            .ok_or(GatewayCredentialError::Malformed)?,
                    })
                })
                .await
                .map_err(credential_failure)
        })
    }
    fn start<'a>(
        &'a self,
        request: PairingStartRequest,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingChallenge, PairingError>> {
        Box::pin(async move {
            let issuer = self.signer.public_key().await?;
            let pin = self.authority.current_pin().await?;
            let identity = StartIdentity {
                operation_id: request.operation_id,
                person_id: request.person_id.to_string(),
                device_id: request.device_id,
                generation: request.generation,
            };
            let (stage, fresh) = self
                .store
                .mutate(|record| {
                    let stage = record
                        .staging
                        .as_mut()
                        .ok_or(GatewayCredentialError::Conflict)?;
                    if stage.target_ref != request.target_ref {
                        return Err(GatewayCredentialError::Conflict);
                    }
                    if let Some(current) = &stage.request {
                        if current != &identity {
                            return Err(GatewayCredentialError::Conflict);
                        }
                        return Ok((stage.clone(), false));
                    }
                    if stage.setup_expires <= chrono::Utc::now().timestamp_millis() {
                        return Err(GatewayCredentialError::Conflict);
                    }
                    stage.request = Some(identity.clone());
                    Ok((stage.clone(), true))
                })
                .await
                .map_err(credential_failure)?;
            if !fresh && stage.challenge.is_some() {
                return Self::public_challenge(&stage);
            }
            let wire:StartWire=self.request(&stage.address,"/pair/start",serde_json::json!({"schema_version":1,"operation_id":identity.operation_id,"proof":stage.polling_proof,"person_id":identity.person_id,"device_id":identity.device_id,"issuer_key_id":issuer.key_id,"issuer_public_key":issuer.public_key}),scope).await?;
            if wire.schema_version != 1
                || wire.person_id != identity.person_id
                || wire.device_id != identity.device_id
                || wire.issuer.key() != issuer
                || wire.issuer.fingerprint != issuer.fingerprint()
                || wire.code.is_empty()
                || wire.code.len() > 32
                || !wire.code.is_ascii()
                || wire.code.chars().any(char::is_control)
                || !valid_token(&wire.proof)
                || wire.proof != stage.polling_proof
                || Uuid::parse_str(&wire.pairing_id).is_err()
            {
                return Err(PairingError::ForeignIdentity);
            }
            let reviewed = ReviewedGatewayIdentity {
                producer: wire.producer.clone(),
                issuer,
                expected_pin_revision: pin.map_or(0, |pin| pin.revision),
            };
            let mut next = stage;
            next.challenge = Some(wire);
            next.reviewed = Some(reviewed);
            Self::signing_command(&next, false)?;
            self.store
                .mutate(|record| {
                    let current = record
                        .staging
                        .as_ref()
                        .ok_or(GatewayCredentialError::Conflict)?;
                    if current.request != next.request || current.challenge.is_some() {
                        return Err(GatewayCredentialError::Conflict);
                    }
                    record.staging = Some(next.clone());
                    Ok(())
                })
                .await
                .map_err(credential_failure)?;
            Self::public_challenge(&next)
        })
    }
    fn confirm<'a>(
        &'a self,
        request: PairingConfirmation,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingObservation, PairingError>> {
        Box::pin(async move {
            let stage = self.stage(&request.handle).await?;
            if stage.reviewed.as_ref() != Some(&request.reviewed) {
                return Err(PairingError::ChangedProducer);
            }
            let observed = self.poll(&request.handle, scope).await?;
            if observed.progress != PairingProgress::AwaitingLocalConfirmation {
                return Ok(observed);
            }
            let signature = self
                .signer
                .sign_enrollment(Self::signing_command(&stage, true)?)
                .await?;
            let wire = stage.challenge.as_ref().ok_or(PairingError::Conflict)?;
            let response:ConfirmationWire=self.request(&stage.address,"/pair/confirm",serde_json::json!({"schema_version":1,"pairing_id":wire.pairing_id,"proof":wire.proof,"challenge_id":wire.challenge_id,"key_id":signature.key_id,"signature":signature.signature}),scope).await?;
            if response.schema_version != 1
                || response.pairing_id != wire.pairing_id
                || !matches!(response.status.as_str(), "local_confirmed" | "approved")
            {
                return Err(PairingError::Rejected);
            }
            self.store
                .mutate(|record| {
                    let current = record
                        .staging
                        .as_mut()
                        .ok_or(GatewayCredentialError::Conflict)?;
                    if current.request != stage.request {
                        return Err(GatewayCredentialError::Conflict);
                    }
                    current.locally_confirmed = true;
                    Ok(())
                })
                .await
                .map_err(credential_failure)?;
            self.poll(&request.handle, scope).await
        })
    }
    fn observe<'a>(
        &'a self,
        pairing: &'a PairingHandle,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingObservation, PairingError>> {
        Box::pin(self.poll(pairing, scope))
    }
    fn cancel<'a>(
        &'a self,
        pairing: &'a PairingHandle,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingObservation, PairingError>> {
        Box::pin(async move {
            let current = self.poll(pairing, scope).await?;
            if current.progress == PairingProgress::Approved {
                return Ok(current);
            }
            let stage = self.stage(pairing).await?;
            let wire = stage.challenge.as_ref().ok_or(PairingError::Conflict)?;
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Cancelled {
                ok: bool,
            }
            let result:Result<Cancelled,PairingError>=self.request(&stage.address,"/pair/cancel",serde_json::json!({"schema_version":1,"pairing_id":wire.pairing_id,"proof":wire.proof}),scope).await;
            match result {
                Ok(reply) if reply.ok => {}
                Err(PairingError::Conflict) => return self.poll(pairing, scope).await,
                _ => return Err(PairingError::TransportUnavailable),
            }
            self.store
                .mutate(|record| {
                    let current = record
                        .staging
                        .as_mut()
                        .ok_or(GatewayCredentialError::Conflict)?;
                    if current.request != stage.request {
                        return Err(GatewayCredentialError::Conflict);
                    }
                    current.cancelled = true;
                    Ok(())
                })
                .await
                .map_err(credential_failure)?;
            Ok(PairingObservation {
                handle: pairing.clone(),
                progress: PairingProgress::Cancelled,
                reviewed: stage.reviewed.ok_or(PairingError::Conflict)?,
            })
        })
    }
}
impl GatewayEnrollmentPort for GatewayPairingAdapter {
    fn complete<'a>(
        &'a self,
        pairing: &'a PairingHandle,
        reviewed: &'a ReviewedGatewayIdentity,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<VerifiedEnrollment, PairingError>> {
        Box::pin(async move {
            let observation = self.poll(pairing, scope).await?;
            if observation.progress != PairingProgress::Approved
                || &observation.reviewed != reviewed
            {
                return Err(PairingError::ChangedProducer);
            }
            let stage = self.stage(pairing).await?;
            let request = stage.request.as_ref().ok_or(PairingError::Conflict)?;
            let wire = stage.challenge.as_ref().ok_or(PairingError::Conflict)?;
            let signature = self
                .signer
                .readback(pairing.operation_id)
                .await?
                .ok_or(PairingError::Conflict)?;
            let command = GatewayPinCommit {
                operation_id: pairing.operation_id,
                request_digest: signature.request_digest,
                expected_revision: reviewed.expected_pin_revision,
                producer: reviewed.producer.clone(),
            };
            let _ = self.authority.commit_pin(command.clone()).await?;
            let receipt = self
                .authority
                .readback(pairing.operation_id)
                .await?
                .ok_or(PairingError::StorageUnavailable)?;
            if receipt.command != command {
                return Err(PairingError::Conflict);
            }
            let record = read_record().await.map_err(credential_failure)?;
            let generation = match record.committed {
                Some(connection) if connection.operation_id == pairing.operation_id => {
                    connection.binding.credential_generation
                }
                _ => self
                    .store
                    .next_generation()
                    .await
                    .map_err(credential_failure)?,
            };
            Ok(VerifiedEnrollment {
                operation_id: pairing.operation_id,
                binding: VerifiedGatewayBinding {
                    person_id: request.person_id.clone(),
                    device_id: request.device_id.clone(),
                    client_id: wire.pairing_id.clone(),
                    producer_instance: wire.producer.instance_id.clone(),
                    producer_key_fingerprint: wire.producer.fingerprint.clone(),
                    producer_audience: wire.producer.audience.clone(),
                    enrollment_id: wire.challenge_id.clone(),
                    credential_generation: generation,
                },
                issuer: wire.issuer.key(),
                pin_revision: receipt.revision,
            })
        })
    }
}
impl GatewayCredentialCommit for GatewayPairingAdapter {
    fn commit<'a>(
        &'a self,
        staged: StagedCredentialRef,
        expected: &'a VerifiedEnrollment,
    ) -> BoxFuture<'a, Result<GatewaySummary, PairingError>> {
        Box::pin(async move {
            if staged.operation_id != expected.operation_id {
                return Err(PairingError::Conflict);
            }
            if let Some(connection) = self
                .store
                .committed_operation(staged.operation_id)
                .await
                .map_err(credential_failure)?
            {
                if connection.binding != expected.binding {
                    return Err(PairingError::Conflict);
                }
                return Ok(summary(&connection));
            }
            self.store
                .validate_pin(&expected.binding)
                .await
                .map_err(credential_failure)?;
            let next_generation = self
                .store
                .next_generation()
                .await
                .map_err(credential_failure)?;
            if next_generation != expected.binding.credential_generation {
                return Err(PairingError::Conflict);
            }
            self.store
                .mutate(|record| {
                    let stage = record
                        .staging
                        .as_ref()
                        .ok_or(GatewayCredentialError::Conflict)?;
                    let request = stage
                        .request
                        .as_ref()
                        .ok_or(GatewayCredentialError::Conflict)?;
                    let wire = stage
                        .challenge
                        .as_ref()
                        .ok_or(GatewayCredentialError::Conflict)?;
                    if request.operation_id != staged.operation_id
                        || request.generation != staged.generation
                        || !stage.locally_confirmed
                        || stage.cancelled
                        || wire.issuer.key() != expected.issuer
                        || request.person_id != expected.binding.person_id
                        || request.device_id != expected.binding.device_id
                        || wire.pairing_id != expected.binding.client_id
                        || wire.producer.instance_id != expected.binding.producer_instance
                        || wire.producer.fingerprint != expected.binding.producer_key_fingerprint
                        || wire.producer.audience != expected.binding.producer_audience
                        || expected.binding.enrollment_id != wire.challenge_id
                        || next_generation != expected.binding.credential_generation
                    {
                        return Err(GatewayCredentialError::Conflict);
                    }
                    let connection = GatewayConnection {
                        operation_id: staged.operation_id,
                        binding: expected.binding.clone(),
                        endpoint: stage.address.clone(),
                        bearer: stage
                            .approved_token
                            .clone()
                            .ok_or(GatewayCredentialError::Unverified)?,
                    };
                    record.generation = connection.binding.credential_generation;
                    record.committed = Some(connection.clone());
                    record.staging = None;
                    Ok(summary(&connection))
                })
                .await
                .map_err(credential_failure)
        })
    }
    fn readback<'a>(
        &'a self,
        operation_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<GatewaySummary>, PairingError>> {
        Box::pin(async move {
            Ok(self
                .store
                .committed_operation(operation_id)
                .await
                .map_err(credential_failure)?
                .as_ref()
                .map(summary))
        })
    }
}
fn summary(connection: &GatewayConnection) -> GatewaySummary {
    GatewaySummary {
        gateway_ref: connection.operation_id,
        revision: connection.binding.credential_generation,
        display_name: "Gateway".into(),
        state: GatewayState::Paired,
        remote_revocation_pending: false,
        allowed_actions: vec![ConnectionAction::Forget, ConnectionAction::Manage],
        failure: None,
    }
}
fn credential_failure(error: GatewayCredentialError) -> PairingError {
    match error {
        GatewayCredentialError::Locked | GatewayCredentialError::Unavailable => {
            PairingError::CredentialUnavailable
        }
        GatewayCredentialError::ForeignIdentity => PairingError::ForeignIdentity,
        GatewayCredentialError::Conflict => PairingError::Conflict,
        GatewayCredentialError::Timeout => PairingError::DeadlineExceeded,
        GatewayCredentialError::Malformed | GatewayCredentialError::Unverified => {
            PairingError::Rejected
        }
    }
}
fn pairing_failure(error: AgentFailure) -> PairingError {
    match error {
        AgentFailure::Cancelled => PairingError::Cancelled,
        AgentFailure::DeadlineExceeded | AgentFailure::ServerModelTimeout => {
            PairingError::DeadlineExceeded
        }
        AgentFailure::PolicyDenied => PairingError::ChangedProducer,
        AgentFailure::InvalidInput => PairingError::InvalidInput,
        _ => PairingError::TransportUnavailable,
    }
}

fn new_pairing_proof() -> Result<String, GatewayCredentialError> {
    use base64::Engine;
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| GatewayCredentialError::Unavailable)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}
