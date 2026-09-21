//! Driving one pairing to its end: confirm it, watch it, finalize it.
//!
//! A producer reports what it sees; Connections decides what that report means
//! for this Person's pairing. The pairing a report settles has to be the one
//! this device is driving, for this Person and this device, and the challenge
//! a finalization stands on has to name the Person's own owner key.

use std::future::Future;

use floe_access::{RemoteEnrollmentSignature, RemoteOwnerPublicKey, RemotePairingChallenge};
use floe_execution::Cancellation;
use floe_kernel::AgentFailure;
use tokio::time::Instant;

use crate::application::pairing::PairingService;
use crate::{
    PairingConfirmationRequest, PairingIssuer, PairingStatus, PairingStatusRequest,
    ProducerIdentity, RemoteControl, admit_pairing_status,
};

/// The pairing this device is driving, as the Person's route names it.
#[derive(Clone, Copy)]
pub struct PairingIdentity<'a> {
    pub person_id: &'a str,
    pub client_id: &'a str,
    pub device_id: &'a str,
}

/// The owner key that speaks for this Person in a pairing, and the record of
/// how the pairing settled.
pub trait PairingOwnerKeys: Sync {
    fn owner_public_key(
        &self,
    ) -> impl Future<Output = Result<RemoteOwnerPublicKey, AgentFailure>> + Send;

    fn sign_pairing(
        &self,
        challenge: &RemotePairingChallenge,
        pairing: PairingIdentity<'_>,
    ) -> impl Future<Output = Result<RemoteEnrollmentSignature, AgentFailure>> + Send;

    /// Record that this pairing reached its end, and whether it was approved.
    fn settle_pairing(
        &self,
        pairing_id: &str,
        challenge: &RemotePairingChallenge,
        approved: bool,
    ) -> impl Future<Output = Result<(), AgentFailure>> + Send;
}

/// The pairing status a producer's report settles to, for this pairing.
///
/// The producer speaks for the pairing; it does not get to rename the Person or
/// the device the pairing belongs to.
pub fn admit_pairing_report(
    person_id: &str,
    pairing: PairingIdentity<'_>,
    observed: PairingStatus,
) -> Result<PairingStatus, AgentFailure> {
    let status = admit_pairing_status(observed)?;
    if status.person_id != person_id
        || pairing.person_id != status.person_id
        || pairing.device_id != status.device_id
        || pairing.client_id != status.pairing_id
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(status)
}

/// Confirm a pairing under this Person's own owner key.
///
/// Nothing is signed until the challenge is known to belong to the pairing this
/// device is driving, and the status this returns is the local one: a
/// confirmation is not an approval, so it never carries credentials.
#[allow(clippy::too_many_arguments)]
pub async fn confirm_pairing(
    service: &PairingService<impl RemoteControl>,
    keys: &impl PairingOwnerKeys,
    person_id: &str,
    pairing: PairingIdentity<'_>,
    challenge: &RemotePairingChallenge,
    polling_proof: &str,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<PairingStatus, AgentFailure> {
    if pairing.person_id != person_id
        || pairing.client_id != challenge.pairing_id
        || pairing.device_id.is_empty()
        || challenge.issuer.key_id.is_empty()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let signature = keys.sign_pairing(challenge, pairing).await?;
    let confirmation = service
        .confirm(
            PairingConfirmationRequest {
                pairing_id: challenge.pairing_id.clone(),
                polling_proof: polling_proof.to_owned(),
                challenge_id: challenge.challenge_id.clone(),
                key_id: signature.key_id,
                signature: signature.signature,
            },
            deadline,
            cancellation,
        )
        .await?;
    Ok(PairingStatus {
        schema_version: confirmation.schema_version,
        pairing_id: confirmation.pairing_id,
        status: confirmation.status,
        person_id: pairing.person_id.to_owned(),
        device_id: pairing.device_id.to_owned(),
        producer: Some(challenged_producer(challenge)),
        issuer: Some(PairingIssuer {
            key_id: challenge.issuer.key_id.clone(),
            public_key: challenge.issuer.public_key.clone(),
            fingerprint: challenge.issuer.fingerprint(),
        }),
        issuer_fingerprint: Some(challenge.issuer.fingerprint()),
        client_id: None,
        token: None,
    })
}

/// What the producer now reports about a pairing this device is driving.
pub async fn read_pairing_status(
    service: &PairingService<impl RemoteControl>,
    person_id: &str,
    pairing: PairingIdentity<'_>,
    pairing_id: &str,
    polling_proof: &str,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<PairingStatus, AgentFailure> {
    let observed = service
        .status(
            PairingStatusRequest {
                pairing_id: pairing_id.to_owned(),
                polling_proof: polling_proof.to_owned(),
            },
            deadline,
            cancellation,
        )
        .await?;
    admit_pairing_report(person_id, pairing, observed)
}

/// Settle a pairing against the challenge the Person's own key was asked to
/// sign.
///
/// The producer's report decides whether the pairing was approved; the
/// challenge it is settled against has to name this pairing and this Person's
/// own issuer key, or the report is about some other pairing.
#[allow(clippy::too_many_arguments)]
pub async fn finalize_pairing(
    service: &PairingService<impl RemoteControl>,
    keys: &impl PairingOwnerKeys,
    person_id: &str,
    pairing: PairingIdentity<'_>,
    pairing_id: &str,
    polling_proof: &str,
    challenge: &RemotePairingChallenge,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<PairingStatus, AgentFailure> {
    let status = read_pairing_status(
        service,
        person_id,
        pairing,
        pairing_id,
        polling_proof,
        deadline,
        cancellation,
    )
    .await?;
    let owner = keys.owner_public_key().await?;
    if status.pairing_id != challenge.pairing_id
        || challenge.pairing_id != pairing_id
        || challenge.issuer != owner
    {
        return Err(AgentFailure::PolicyDenied);
    }
    keys.settle_pairing(pairing_id, challenge, status.status == "approved")
        .await?;
    Ok(status)
}

/// The producer a challenge was issued by, as a pairing report names one.
fn challenged_producer(challenge: &RemotePairingChallenge) -> ProducerIdentity {
    ProducerIdentity {
        schema_version: challenge.producer.schema_version,
        instance_id: challenge.producer.instance_id.clone(),
        execution_owner: challenge.producer.execution_owner.clone(),
        audience: challenge.producer.audience.clone(),
        key_id: challenge.producer.key_id.clone(),
        public_key: challenge.producer.public_key.clone(),
        fingerprint: challenge.producer.fingerprint.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Remote(PairingStatus);

    impl RemoteControl for Remote {
        async fn confirm(
            &self,
            request: PairingConfirmationRequest,
            _: Instant,
            _: &Cancellation,
        ) -> Result<crate::PairingConfirmation, AgentFailure> {
            Ok(crate::PairingConfirmation {
                schema_version: 1,
                pairing_id: request.pairing_id,
                status: "local_confirmed".into(),
            })
        }
        async fn status(
            &self,
            _: PairingStatusRequest,
            _: Instant,
            _: &Cancellation,
        ) -> Result<PairingStatus, AgentFailure> {
            Ok(self.0.clone())
        }
    }

    struct Keys {
        owner: RemoteOwnerPublicKey,
        signatures: AtomicUsize,
        settlements: AtomicUsize,
    }

    impl PairingOwnerKeys for Keys {
        async fn owner_public_key(&self) -> Result<RemoteOwnerPublicKey, AgentFailure> {
            Ok(self.owner.clone())
        }
        async fn sign_pairing(
            &self,
            _: &RemotePairingChallenge,
            _: PairingIdentity<'_>,
        ) -> Result<RemoteEnrollmentSignature, AgentFailure> {
            self.signatures.fetch_add(1, Ordering::SeqCst);
            Ok(RemoteEnrollmentSignature {
                key_id: self.owner.key_id.clone(),
                signature: "signature".into(),
            })
        }
        async fn settle_pairing(
            &self,
            _: &str,
            _: &RemotePairingChallenge,
            _: bool,
        ) -> Result<(), AgentFailure> {
            self.settlements.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn status(pairing_id: &str) -> PairingStatus {
        PairingStatus {
            schema_version: 1,
            pairing_id: pairing_id.into(),
            status: "approved".into(),
            person_id: "verified-person".into(),
            device_id: "verified-device".into(),
            producer: None,
            issuer: None,
            issuer_fingerprint: None,
            client_id: Some(pairing_id.into()),
            token: Some("new_private_token".into()),
        }
    }

    #[tokio::test]
    async fn foreign_status_and_challenge_never_settle_or_release_credentials() {
        let pairing_id = uuid::Uuid::new_v4().to_string();
        let pairing = PairingIdentity {
            person_id: "verified-person",
            device_id: "verified-device",
            client_id: &pairing_id,
        };
        let keys = Keys {
            owner: RemoteOwnerPublicKey {
                key_id: uuid::Uuid::new_v4().to_string(),
                public_key: "public".into(),
            },
            signatures: AtomicUsize::new(0),
            settlements: AtomicUsize::new(0),
        };
        let challenge = RemotePairingChallenge {
            pairing_id: pairing_id.clone(),
            challenge_id: uuid::Uuid::new_v4().to_string(),
            challenge_b64url: "signed-evidence".into(),
            producer_signature: "signature".into(),
            producer: floe_access::RemoteProducerIdentity {
                schema_version: 1,
                instance_id: "instance".into(),
                execution_owner: "owner".into(),
                audience: "audience".into(),
                key_id: "producer".into(),
                public_key: "public".into(),
                fingerprint: "fingerprint".into(),
            },
            issuer: keys.owner.clone(),
            expires_at_unix_ms: i64::MAX,
        };
        for field in ["person", "device", "client"] {
            let mut foreign = status(&pairing_id);
            match field {
                "person" => foreign.person_id = "foreign".into(),
                "device" => foreign.device_id = "foreign".into(),
                _ => foreign.client_id = Some(uuid::Uuid::new_v4().to_string()),
            }
            let result = finalize_pairing(
                &PairingService::new(Remote(foreign)),
                &keys,
                "verified-person",
                pairing,
                &pairing_id,
                "proof",
                &challenge,
                Instant::now() + std::time::Duration::from_secs(1),
                &Cancellation::default(),
            )
            .await;
            assert!(matches!(
                result,
                Err(AgentFailure::PolicyDenied | AgentFailure::CapabilityUnavailable)
            ));
            assert_eq!(keys.settlements.load(Ordering::SeqCst), 0);
        }
        for field in ["pairing", "issuer"] {
            let mut foreign = challenge.clone();
            match field {
                "pairing" => foreign.pairing_id = uuid::Uuid::new_v4().to_string(),
                _ => foreign.issuer.key_id = uuid::Uuid::new_v4().to_string(),
            }
            assert_eq!(
                finalize_pairing(
                    &PairingService::new(Remote(status(&pairing_id))),
                    &keys,
                    "verified-person",
                    pairing,
                    &pairing_id,
                    "proof",
                    &foreign,
                    Instant::now() + std::time::Duration::from_secs(1),
                    &Cancellation::default()
                )
                .await,
                Err(AgentFailure::PolicyDenied)
            );
            assert_eq!(keys.settlements.load(Ordering::SeqCst), 0);
        }
        let mut foreign = challenge.clone();
        foreign.pairing_id = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            confirm_pairing(
                &PairingService::new(Remote(status(&pairing_id))),
                &keys,
                "verified-person",
                pairing,
                &foreign,
                "proof",
                Instant::now() + std::time::Duration::from_secs(1),
                &Cancellation::default()
            )
            .await,
            Err(AgentFailure::PolicyDenied)
        );
        assert_eq!(keys.signatures.load(Ordering::SeqCst), 0);
        let approved = finalize_pairing(
            &PairingService::new(Remote(status(&pairing_id))),
            &keys,
            "verified-person",
            pairing,
            &pairing_id,
            "proof",
            &challenge,
            Instant::now() + std::time::Duration::from_secs(1),
            &Cancellation::default(),
        )
        .await
        .unwrap();
        assert_eq!(keys.settlements.load(Ordering::SeqCst), 1);
        assert_eq!(approved.token.as_deref(), Some("new_private_token"));
        assert!(!format!("{approved:?}").contains("new_private_token"));
    }

    #[tokio::test]
    async fn nonapproved_and_oversized_pairing_credentials_are_rejected() {
        let pairing_id = uuid::Uuid::new_v4().to_string();
        for invalid in [
            PairingStatus {
                status: "pending".into(),
                ..status(&pairing_id)
            },
            PairingStatus {
                token: Some("x".repeat(257)),
                ..status(&pairing_id)
            },
        ] {
            assert_eq!(
                PairingService::new(Remote(invalid))
                    .status(
                        PairingStatusRequest {
                            pairing_id: pairing_id.clone(),
                            polling_proof: "proof".into()
                        },
                        Instant::now() + std::time::Duration::from_secs(1),
                        &Cancellation::default(),
                    )
                    .await,
                Err(AgentFailure::CapabilityUnavailable)
            );
        }
        let state = crate::PairingOperationState::Approved {
            client_id: pairing_id,
            token: "private_state_token".into(),
        };
        assert!(!format!("{state:?}").contains("private_state_token"));
    }
}
