//! Gateway protocol translation. Secrets are read from the encrypted Vault;
//! only Connections and its transactional repository may advance local state.
use super::{http::GatewayHttpTransport, proof};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use floe_access::{RemoteOwnerPublicKey, RemoteProducerIdentity};
use floe_agent_contract::{AgentFailure, BoxFuture};
use floe_connections::*;
use floe_kernel::{OwnerActor, PersonId};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Deserialize)]
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
#[derive(Deserialize)]
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

pub struct GatewayPairingAdapter {
    reader: Arc<dyn GatewayPrivateReader>,
    signer: Arc<dyn EnrollmentSigner>,
    authority: Arc<dyn GatewayAuthorityRepository>,
    person: PersonId,
    device: String,
}
impl GatewayPairingAdapter {
    pub fn new(
        reader: Arc<dyn GatewayPrivateReader>,
        signer: Arc<dyn EnrollmentSigner>,
        authority: Arc<dyn GatewayAuthorityRepository>,
        actor: &OwnerActor,
    ) -> Result<Self, AgentFailure> {
        actor.validate()?;
        Ok(Self {
            reader,
            signer,
            authority,
            person: actor.person_id,
            device: actor.device_id.clone(),
        })
    }

    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        address: &str,
        path: &str,
        body: serde_json::Value,
        scope: &OperationScope,
    ) -> Result<T, PairingError> {
        check_scope(scope)?;
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
            .map_err(|error| match pairing_failure(error) {
                PairingError::Cancelled
                | PairingError::DeadlineExceeded
                | PairingError::TransportUnavailable => PairingError::Indeterminate,
                other => other,
            })?;
        super::json::strict_json_bytes(&bytes, 65536).map_err(|_| PairingError::Indeterminate)?;
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
                serde_json::from_slice(&bytes).map_err(|_| PairingError::Indeterminate)?;
            if status == 409 && wire.error.code == "pairing_repair_required" {
                return Err(PairingError::RepairRequired);
            }
            return Err(match status {
                400 => PairingError::InvalidInput,
                401 | 403 => PairingError::Rejected,
                409 => PairingError::Conflict,
                _ => PairingError::Indeterminate,
            });
        }
        serde_json::from_slice(&bytes).map_err(|_| PairingError::Indeterminate)
    }

    async fn private(
        &self,
        handle: &PairingHandle,
    ) -> Result<PairingPrivateSnapshot, PairingError> {
        let snapshot = self
            .reader
            .pairing_private(
                handle.operation_id,
                self.person,
                &self.device,
                handle.generation,
            )
            .await?;
        if snapshot.operation.handle.as_ref() != Some(handle)
            || snapshot.operation.person_id != self.person
            || snapshot.operation.device_id != self.device
        {
            return Err(PairingError::ForeignIdentity);
        }
        Ok(snapshot)
    }

    fn signing_command(
        request: &PairingStartRequest,
        wire: &StartWire,
    ) -> Result<EnrollmentSigningCommand, PairingError> {
        let bytes =
            proof::decode_canonical(&wire.challenge_b64url, 65536).map_err(pairing_failure)?;
        let signature =
            proof::decode_exact(&wire.producer_signature, 64).map_err(pairing_failure)?;
        let challenge = proof::parse_challenge(&bytes).map_err(pairing_failure)?;
        proof::verify_signature(&wire.producer, &bytes, &signature).map_err(pairing_failure)?;
        if challenge.operation != "enrollment"
            || challenge.purpose != "owner_enrollment"
            || challenge._consumer != "owner"
            || challenge.person_id != request.person_id.to_string()
            || challenge.device_id != request.device_id
            || challenge.client_id != wire.pairing_id
            || challenge.key_id != wire.issuer.key_id
            || challenge.audience != wire.producer.audience
            || challenge.challenge_id != wire.challenge_id
            || challenge.expires_at_unix_ms != wire.expires_at_unix_ms
            || challenge.issued_at_unix_ms
                > chrono::Utc::now().timestamp_millis().saturating_add(5000)
        {
            return Err(PairingError::ChangedProducer);
        }
        // A delayed Start reply may already have expired. Preserve its exact
        // challenge for truthful observation; signing separately checks expiry.
        Ok(EnrollmentSigningCommand {
            operation_id: request.operation_id,
            request_digest: Sha256::digest(&bytes).into(),
            person_id: request.person_id,
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
        let snapshot = self.private(handle).await?;
        let operation = &snapshot.operation;
        let reviewed = operation
            .reviewed
            .clone()
            .ok_or(PairingError::RepairRequired)?;
        let proof = Zeroizing::new(URL_SAFE_NO_PAD.encode(snapshot.proof.as_bytes()));
        let status: StatusWire = match self.request(
            &operation.setup.display_address, "/pair/poll",
            serde_json::json!({"schema_version":1,"pairing_id":handle.attempt_id,"proof":proof.as_str()}), scope,
        ).await {
            Ok(status) => status,
            Err(PairingError::RepairRequired) => return Ok(PairingObservation {
                handle: handle.clone(), reviewed, outcome: PairingOutcome::RepairRequired,
            }),
            Err(error) => return Err(error),
        };
        if status.schema_version != 1
            || status.pairing_id != handle.attempt_id.to_string()
            || status.person_id != operation.person_id.to_string()
            || status.device_id != operation.device_id
        {
            return Err(PairingError::ForeignIdentity);
        }
        let outcome = match status.status.as_str() {
            "pending" => PairingOutcome::AwaitingLocalConfirmation,
            "local_confirmed" => PairingOutcome::AwaitingApproval,
            "rejected" => PairingOutcome::Rejected,
            "expired" => PairingOutcome::Expired,
            "cancelled" => PairingOutcome::Cancelled,
            "repair_required" => PairingOutcome::RepairRequired,
            "approved" => {
                if operation.confirmation_command.is_none()
                    || status.producer.as_ref() != Some(&reviewed.producer)
                    || status.issuer.as_ref().map(IssuerWire::key) != Some(reviewed.issuer.clone())
                    || status.issuer_fingerprint.as_deref()
                        != Some(reviewed.issuer.fingerprint().as_str())
                    || status
                        .issuer
                        .as_ref()
                        .is_none_or(|issuer| issuer.fingerprint != reviewed.issuer.fingerprint())
                    || status.client_id.as_deref() != Some(handle.attempt_id.to_string().as_str())
                {
                    return Err(PairingError::ChangedProducer);
                }
                let enrollment = snapshot
                    .enrollment
                    .as_ref()
                    .ok_or(PairingError::RepairRequired)?;
                let signature = self
                    .signer
                    .readback(handle.operation_id)
                    .await?
                    .ok_or(PairingError::RepairRequired)?;
                if signature.key_id != enrollment.issuer.key_id
                    || signature.request_digest != enrollment.request_digest
                {
                    return Err(PairingError::ForeignIdentity);
                }
                let token = status.token.ok_or(PairingError::Rejected)?;
                let credential = GatewayCredentialMaterial::new(token.into_bytes())?;
                return Ok(PairingObservation {
                    handle: handle.clone(),
                    reviewed,
                    outcome: PairingOutcome::Approved(credential),
                });
            }
            _ => return Err(PairingError::Rejected),
        };
        if status.token.is_some()
            || status.client_id.is_some()
            || status.producer.is_some()
            || status.issuer.is_some()
            || status.issuer_fingerprint.is_some()
        {
            return Err(PairingError::ForeignIdentity);
        }
        Ok(PairingObservation {
            handle: handle.clone(),
            reviewed,
            outcome,
        })
    }
}

impl GatewayPairingPort for GatewayPairingAdapter {
    fn prepare_start<'a>(
        &'a self,
        request: PairingStartRequest,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PreparedPairingStart, PairingError>> {
        Box::pin(async move {
            check_scope(scope)?;
            request.setup.validate()?;
            if request.person_id != self.person || request.device_id != self.device {
                return Err(PairingError::ForeignIdentity);
            }
            let snapshot = self
                .reader
                .pairing_private(
                    request.operation_id,
                    self.person,
                    &self.device,
                    request.generation,
                )
                .await?;
            let operation = &snapshot.operation;
            if operation.setup != request.setup
                || operation.state != PairingState::Pending
                || operation.handle.is_some()
                || operation.forgotten_command.is_some()
                || operation.cancellation_command.is_some()
            {
                return Err(PairingError::Conflict);
            }
            let issuer = snapshot.issuer;
            let pin = self.authority.current_pin().await?;
            check_scope(scope)?;
            Ok(PreparedPairingStart {
                request,
                proof: snapshot.proof,
                issuer,
                expected_pin_revision: pin.map_or(0, |pin| pin.revision),
            })
        })
    }
    fn start<'a>(
        &'a self,
        prepared: PreparedPairingStart,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<StartedPairing, PairingError>> {
        Box::pin(async move {
            let PreparedPairingStart {
                request,
                proof,
                issuer,
                expected_pin_revision,
            } = prepared;
            let polling_proof = Zeroizing::new(URL_SAFE_NO_PAD.encode(proof.as_bytes()));
            let wire: StartWire = self.request(&request.setup.display_address, "/pair/start", serde_json::json!({
                "schema_version":1,"operation_id":request.operation_id,"proof":polling_proof.as_str(),
                "person_id":request.person_id.to_string(),"device_id":request.device_id,
                "issuer_key_id":issuer.key_id,"issuer_public_key":issuer.public_key,
            }), scope).await?;
            if wire.schema_version != 1
                || wire.person_id != self.person.to_string()
                || wire.device_id != self.device
                || wire.issuer.key() != issuer
                || wire.issuer.fingerprint != issuer.fingerprint()
                || wire.code.is_empty()
                || wire.code.len() > 32
                || !wire.code.is_ascii()
                || wire.code.chars().any(char::is_control)
                || wire.proof != *polling_proof
            {
                return Err(PairingError::ForeignIdentity);
            }
            let enrollment = Self::signing_command(&request, &wire)?;
            Ok(StartedPairing {
                challenge: PairingChallenge {
                    handle: PairingHandle {
                        operation_id: request.operation_id,
                        attempt_id: Uuid::parse_str(&wire.pairing_id)
                            .map_err(|_| PairingError::Rejected)?,
                        generation: request.generation,
                    },
                    display_code: wire.code,
                    expires_at_unix_ms: wire.expires_at_unix_ms,
                    reviewed: ReviewedGatewayIdentity {
                        producer: wire.producer,
                        issuer,
                        expected_pin_revision,
                    },
                },
                enrollment,
            })
        })
    }

    fn confirm<'a>(
        &'a self,
        request: PairingConfirmation,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingObservation, PairingError>> {
        Box::pin(async move {
            let snapshot = self.private(&request.handle).await?;
            if snapshot.operation.reviewed.as_ref() != Some(&request.reviewed)
                || snapshot.operation.confirmation_command.is_none()
                || snapshot.operation.cancellation_command.is_some()
                || snapshot.operation.forgotten_command.is_some()
            {
                return Err(PairingError::Conflict);
            }
            let observed = self.poll(&request.handle, scope).await?;
            if !matches!(observed.outcome, PairingOutcome::AwaitingLocalConfirmation) {
                return Ok(observed);
            }
            let command = snapshot.enrollment.ok_or(PairingError::RepairRequired)?;
            let challenge_id = command.challenge_id;
            let signature = self.signer.sign_enrollment(command).await?;
            let proof = Zeroizing::new(URL_SAFE_NO_PAD.encode(snapshot.proof.as_bytes()));
            let response: ConfirmationWire = self.request(&snapshot.operation.setup.display_address, "/pair/confirm", serde_json::json!({
                "schema_version":1,"pairing_id":request.handle.attempt_id,"proof":proof.as_str(),
                "challenge_id":challenge_id,"key_id":signature.key_id,"signature":signature.signature,
            }), scope).await?;
            if response.schema_version != 1
                || response.pairing_id != request.handle.attempt_id.to_string()
                || !matches!(response.status.as_str(), "local_confirmed" | "approved")
            {
                return Err(PairingError::Rejected);
            }
            self.poll(&request.handle, scope).await
        })
    }

    fn observe<'a>(
        &'a self,
        handle: &'a PairingHandle,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingObservation, PairingError>> {
        Box::pin(self.poll(handle, scope))
    }

    fn cancel<'a>(
        &'a self,
        handle: &'a PairingHandle,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingObservation, PairingError>> {
        Box::pin(async move {
            let snapshot = self.private(handle).await?;
            if snapshot.operation.cancellation_command.is_none() {
                return Err(PairingError::Conflict);
            }
            let current = self.poll(handle, scope).await?;
            if matches!(
                current.outcome,
                PairingOutcome::Approved(_)
                    | PairingOutcome::Cancelled
                    | PairingOutcome::Rejected
                    | PairingOutcome::Expired
                    | PairingOutcome::RepairRequired
            ) {
                // Approved is an observed race, not permission to activate.
                // The owner records revocation evidence without publishing it.
                return Ok(current);
            }
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Cancelled {
                ok: bool,
            }
            let proof = Zeroizing::new(URL_SAFE_NO_PAD.encode(snapshot.proof.as_bytes()));
            let result: Result<Cancelled, PairingError> = self.request(&snapshot.operation.setup.display_address, "/pair/cancel",
                serde_json::json!({"schema_version":1,"pairing_id":handle.attempt_id,"proof":proof.as_str()}), scope).await;
            match result {
                Ok(reply) if reply.ok => Ok(PairingObservation {
                    handle: handle.clone(),
                    reviewed: snapshot
                        .operation
                        .reviewed
                        .ok_or(PairingError::RepairRequired)?,
                    outcome: PairingOutcome::Cancelled,
                }),
                Err(PairingError::Conflict) => self.poll(handle, scope).await,
                Err(error) => Err(error),
                _ => Err(PairingError::Indeterminate),
            }
        })
    }
}

fn check_scope(scope: &OperationScope) -> Result<(), PairingError> {
    if scope.cancellation().is_cancelled() {
        return Err(PairingError::Cancelled);
    }
    if scope.deadline() <= tokio::time::Instant::now() {
        return Err(PairingError::DeadlineExceeded);
    }
    Ok(())
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
