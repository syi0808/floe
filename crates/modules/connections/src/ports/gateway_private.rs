//! Private credential material crosses only real storage and transport ports.
//! None of these types is a product DTO, serializable envelope or Debug value.
use crate::{
    EnrollmentSigningCommand, PairingError, PairingHandle, PairingRecord, ReviewedGatewayIdentity,
};
use floe_access::{GatewayCredentialExpectation, VerifiedGatewayBinding};
use floe_execution::BoxFuture;
use floe_kernel::PersonId;
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct PairingProof(Zeroizing<[u8; 32]>);
impl PairingProof {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
#[derive(Clone)]
pub struct GatewayCredentialMaterial(Zeroizing<Vec<u8>>);
impl GatewayCredentialMaterial {
    /// Go currently issues two rand.Text values. Retain the existing bounded
    /// token alphabet/range, which includes its actual 52-character token.
    pub fn new(bytes: Vec<u8>) -> Result<Self, PairingError> {
        let bytes = Zeroizing::new(bytes);
        if !(32..=256).contains(&bytes.len())
            || !bytes
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
        {
            return Err(PairingError::Rejected);
        }
        Ok(Self(bytes))
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("validated ASCII credential")
    }
}
pub struct PairingPrivateSnapshot {
    pub operation: PairingRecord,
    pub proof: PairingProof,
    pub enrollment: Option<EnrollmentSigningCommand>,
}
#[derive(Clone)]
pub struct GatewayCredentialSnapshot {
    pub operation_id: Uuid,
    pub binding: VerifiedGatewayBinding,
    pub endpoint: String,
    pub bearer: GatewayCredentialMaterial,
}
pub enum GatewayCredentialRead {
    Absent {
        expectation: GatewayCredentialExpectation,
    },
    Active(GatewayCredentialSnapshot),
    RepairRequired {
        expectation: GatewayCredentialExpectation,
    },
}
pub trait GatewayPrivateReader: Send + Sync {
    fn pairing_private<'a>(
        &'a self,
        operation_id: Uuid,
        person: PersonId,
        device: &'a str,
        generation: u64,
    ) -> BoxFuture<'a, Result<PairingPrivateSnapshot, PairingError>>;
    /// One coherent transaction reads expectation, operation, secret and pin.
    fn credential<'a>(
        &'a self,
        person: PersonId,
        device: &'a str,
    ) -> BoxFuture<'a, Result<GatewayCredentialRead, PairingError>>;
}
pub struct StartedPairing {
    pub challenge: crate::PairingChallenge,
    pub enrollment: EnrollmentSigningCommand,
}
pub struct PairingApproval {
    pub handle: PairingHandle,
    pub reviewed: ReviewedGatewayIdentity,
    pub person_id: PersonId,
    pub device_id: String,
    pub credential: GatewayCredentialMaterial,
}
pub enum PairingActivationResult {
    Activated(PairingRecord),
    /// The local cancellation or Forget decision won. This stores only remote revocation
    /// evidence, never current credential authority or a replacement pin.
    HistoricalRevocationEvidence(PairingRecord),
}

/// Fully read and validated private inputs for one external Start handoff.
/// This transport-only value is neither serializable nor a product DTO.
pub struct PreparedPairingStart {
    pub request: crate::PairingStartRequest,
    pub proof: PairingProof,
    pub issuer: floe_access::RemoteOwnerPublicKey,
    pub expected_pin_revision: u64,
}
