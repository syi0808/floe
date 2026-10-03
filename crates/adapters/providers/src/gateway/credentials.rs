//! Read-only credential admission at the Gateway transport boundary.
//!
//! Connections owns credential lifecycle. The Vault adapter supplies one
//! transactionally consistent snapshot of its encrypted state; this adapter
//! never reads Keychain items or mutates a second credential record.
use floe_access::{GatewayAdmission, GatewayTrustReader, VerifiedGatewayBinding};
use floe_agent_contract::{AgentFailure, ModelBindingDigest};
use floe_connections::{GatewayCredentialRead, GatewayPrivateReader, PairingError};
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{OwnerActor, PersonId};
use std::sync::Arc;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GatewayCredentialError {
    Locked,
    Unavailable,
    Timeout,
    Malformed,
    Unverified,
    ForeignIdentity,
    Conflict,
    Cancelled,
    Indeterminate,
}

#[derive(Clone)]
pub struct GatewayCredentialStore {
    trust: Arc<dyn GatewayTrustReader>,
    reader: Arc<dyn GatewayPrivateReader>,
    person_id: PersonId,
    device_id: String,
}

/// Secret-bearing transport snapshot. Deliberately neither Debug nor Serialize.
#[derive(Clone)]
pub(crate) struct GatewayConnection {
    pub(crate) binding: VerifiedGatewayBinding,
    pub(crate) endpoint: String,
    pub(crate) bearer: Zeroizing<String>,
}

impl GatewayCredentialStore {
    pub fn new(
        trust: Arc<dyn GatewayTrustReader>,
        reader: Arc<dyn GatewayPrivateReader>,
        actor: &OwnerActor,
    ) -> Result<Self, AgentFailure> {
        actor.validate()?;
        Ok(Self {
            trust,
            reader,
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
        })
    }

    pub(crate) fn trust(&self) -> Arc<dyn GatewayTrustReader> {
        self.trust.clone()
    }

    pub(crate) async fn load(
        &self,
        person: &str,
        device: &str,
    ) -> Result<Option<GatewayConnection>, GatewayCredentialError> {
        if self.person_id.to_string() != person || self.device_id != device {
            return Err(GatewayCredentialError::ForeignIdentity);
        }
        match self
            .reader
            .credential(self.person_id, device)
            .await
            .map_err(read_failure)?
        {
            GatewayCredentialRead::Absent { expectation } => {
                // Missing private material under Pending/Committed authority is
                // never valid model-primary absence or fallback eligibility.
                match expectation {
                    floe_access::GatewayCredentialExpectation::Unpaired
                    | floe_access::GatewayCredentialExpectation::Forgotten { .. } => Ok(None),
                    _ => Err(GatewayCredentialError::Unverified),
                }
            }
            GatewayCredentialRead::RepairRequired { .. } => Err(GatewayCredentialError::Unverified),
            GatewayCredentialRead::Active(snapshot) => {
                if snapshot.operation_id.is_nil()
                    || snapshot.binding.validate().is_err()
                    || snapshot.binding.person_id != person
                    || snapshot.binding.device_id != device
                    || !super::http::valid_endpoint(&snapshot.endpoint)
                {
                    return Err(GatewayCredentialError::ForeignIdentity);
                }
                // The reader has already checked expectation, operation, pin,
                // and material together in one read transaction. A separate
                // read here would introduce a torn observation rather than
                // strengthen that authority proof.
                Ok(Some(GatewayConnection {
                    binding: snapshot.binding,
                    endpoint: snapshot.endpoint,
                    bearer: Zeroizing::new(snapshot.bearer.as_str().to_owned()),
                }))
            }
        }
    }

    pub async fn current_binding(
        &self,
        person: &str,
        device: &str,
    ) -> Result<Option<VerifiedGatewayBinding>, GatewayCredentialError> {
        Ok(self
            .load(person, device)
            .await?
            .map(|connection| connection.binding))
    }
}

impl GatewayConnection {
    pub(crate) fn binding_digest(&self) -> ModelBindingDigest {
        binding_digest(&self.binding)
    }
}

pub(crate) fn binding_digest(binding: &VerifiedGatewayBinding) -> ModelBindingDigest {
    ModelBindingDigest(binding.binding_digest())
}

fn read_failure(error: PairingError) -> GatewayCredentialError {
    match error {
        PairingError::ForeignIdentity | PairingError::ChangedProducer => {
            GatewayCredentialError::ForeignIdentity
        }
        PairingError::InvalidInput | PairingError::Rejected => GatewayCredentialError::Malformed,
        PairingError::Conflict => GatewayCredentialError::Conflict,
        PairingError::Cancelled => GatewayCredentialError::Cancelled,
        PairingError::DeadlineExceeded => GatewayCredentialError::Timeout,
        PairingError::RepairRequired | PairingError::Expired => GatewayCredentialError::Unverified,
        PairingError::Indeterminate => GatewayCredentialError::Indeterminate,
        PairingError::CredentialUnavailable
        | PairingError::StorageUnavailable
        | PairingError::TransportUnavailable => GatewayCredentialError::Unavailable,
    }
}

impl GatewayAdmission for GatewayCredentialStore {
    fn admit<'a>(
        &'a self,
        expected: &'a VerifiedGatewayBinding,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<VerifiedGatewayBinding, AgentFailure>> {
        Box::pin(async move {
            if scope.cancellation().is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            if scope.deadline() <= tokio::time::Instant::now() {
                return Err(AgentFailure::DeadlineExceeded);
            }
            expected.validate()?;
            let current = self
                .load(&expected.person_id, &expected.device_id)
                .await
                .map_err(|_| AgentFailure::PolicyDenied)?
                .ok_or(AgentFailure::PolicyDenied)?;
            if &current.binding != expected {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(current.binding)
        })
    }
}
