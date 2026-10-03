use floe_access::{RemoteOwnerPublicKey, RemoteProducerIdentity, VerifiedGatewayBinding};
use floe_execution::BoxFuture;
use floe_kernel::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
pub type OperationScope = floe_execution::ExecutionScope;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingError {
    InvalidInput,
    ForeignIdentity,
    ChangedProducer,
    Expired,
    Rejected,
    Conflict,
    CredentialUnavailable,
    StorageUnavailable,
    TransportUnavailable,
    Cancelled,
    DeadlineExceeded,
    RepairRequired,
    /// A protected or remote handoff may have completed; keep the operation.
    Indeterminate,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewaySetup {
    pub target_ref: Uuid,
    pub display_address: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}
impl GatewaySetup {
    pub fn canonical_address(address: &str) -> Result<String, PairingError> {
        if address.len() > 2048 {
            return Err(PairingError::InvalidInput);
        }
        let authority = address
            .strip_prefix("http://")
            .ok_or(PairingError::InvalidInput)?;
        let authority = authority.strip_suffix('/').unwrap_or(authority);
        let (host, port) = authority
            .split_once(':')
            .ok_or(PairingError::InvalidInput)?;
        if !matches!(host, "127.0.0.1" | "localhost")
            || port.is_empty()
            || !port.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(PairingError::InvalidInput);
        }
        let port: u16 = port.parse().map_err(|_| PairingError::InvalidInput)?;
        if port == 0 {
            return Err(PairingError::InvalidInput);
        }
        Ok(format!("http://127.0.0.1:{port}"))
    }
    pub fn validate(&self) -> Result<(), PairingError> {
        if self.target_ref.is_nil()
            || Self::canonical_address(&self.display_address)? != self.display_address
        {
            return Err(PairingError::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingHandle {
    pub operation_id: Uuid,
    pub attempt_id: Uuid,
    pub generation: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedGatewayIdentity {
    pub producer: RemoteProducerIdentity,
    pub issuer: RemoteOwnerPublicKey,
    pub expected_pin_revision: u64,
}
#[derive(Clone, Debug)]
pub struct PairingStartRequest {
    pub operation_id: Uuid,
    pub setup: GatewaySetup,
    pub person_id: PersonId,
    pub device_id: String,
    pub generation: u64,
}
#[derive(Clone, Debug)]
pub struct PairingChallenge {
    pub handle: PairingHandle,
    pub display_code: String,
    pub expires_at_unix_ms: i64,
    pub reviewed: ReviewedGatewayIdentity,
}
#[derive(Clone, Debug)]
pub struct PairingConfirmation {
    pub handle: PairingHandle,
    pub reviewed: ReviewedGatewayIdentity,
}
pub enum PairingOutcome {
    AwaitingLocalConfirmation,
    AwaitingApproval,
    Approved(crate::GatewayCredentialMaterial),
    Rejected,
    Expired,
    Cancelled,
    RepairRequired,
}
pub struct PairingObservation {
    pub handle: PairingHandle,
    pub reviewed: ReviewedGatewayIdentity,
    pub outcome: PairingOutcome,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedEnrollment {
    pub operation_id: Uuid,
    pub binding: VerifiedGatewayBinding,
    pub issuer: RemoteOwnerPublicKey,
    pub pin_revision: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayState {
    Unpaired,
    Paired,
    RepairRequired,
    Forgotten,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionAction {
    Pair,
    Forget,
    Manage,
    Confirm,
    Cancel,
    Reobserve,
    Configure,
    Disconnect,
    PrepareObserveReview,
    PauseObserve,
    Allow,
    Decline,
    Start,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewaySummary {
    pub gateway_ref: Uuid,
    pub revision: u64,
    pub display_name: String,
    pub state: GatewayState,
    pub remote_revocation_pending: bool,
    pub allowed_actions: Vec<ConnectionAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<crate::ConnectionFailure>,
}

pub trait GatewayPairingPort: Send + Sync {
    /// Performs local reads/validation only, before the owner commits dispatch intent.
    fn prepare_start<'a>(
        &'a self,
        request: PairingStartRequest,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<crate::PreparedPairingStart, PairingError>>;
    fn start<'a>(
        &'a self,
        prepared: crate::PreparedPairingStart,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<crate::StartedPairing, PairingError>>;
    fn confirm<'a>(
        &'a self,
        request: PairingConfirmation,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingObservation, PairingError>>;
    fn observe<'a>(
        &'a self,
        pairing: &'a PairingHandle,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingObservation, PairingError>>;
    fn cancel<'a>(
        &'a self,
        pairing: &'a PairingHandle,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingObservation, PairingError>>;
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayPin {
    pub producer: RemoteProducerIdentity,
    pub revision: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayPinCommit {
    pub operation_id: Uuid,
    pub request_digest: [u8; 32],
    pub expected_revision: u64,
    pub producer: RemoteProducerIdentity,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayPinReceipt {
    pub command: GatewayPinCommit,
    pub revision: u64,
}
pub trait GatewayAuthorityRepository: Send + Sync {
    fn current_pin<'a>(&'a self) -> BoxFuture<'a, Result<Option<GatewayPin>, PairingError>>;
}
/// Exact validated canonical bytes are an internal signing command, never a
/// product DTO. Raw private keys cannot cross this boundary.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentSigningCommand {
    pub operation_id: Uuid,
    pub request_digest: [u8; 32],
    pub person_id: PersonId,
    pub device_id: String,
    pub client_id: String,
    pub issuer: RemoteOwnerPublicKey,
    pub producer: RemoteProducerIdentity,
    pub challenge_id: Uuid,
    pub canonical_bytes: Vec<u8>,
    pub producer_signature: Vec<u8>,
    pub issued_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
}
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentSignature {
    pub operation_id: Uuid,
    pub request_digest: [u8; 32],
    pub key_id: String,
    pub signature: String,
}
#[derive(Clone, Eq, PartialEq)]
pub struct EnrollmentReceipt {
    pub operation_id: Uuid,
    pub request_digest: [u8; 32],
    pub key_id: String,
}
pub trait EnrollmentSigner: Send + Sync {
    fn public_key<'a>(&'a self) -> BoxFuture<'a, Result<RemoteOwnerPublicKey, PairingError>>;
    fn sign_enrollment<'a>(
        &'a self,
        command: EnrollmentSigningCommand,
    ) -> BoxFuture<'a, Result<EnrollmentSignature, PairingError>>;
    fn readback<'a>(
        &'a self,
        operation_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<EnrollmentReceipt>, PairingError>>;
}
/// The real Gateway adapter checks strict signed-wire framing and every claim
/// against this exact internal command before the key holder signs it.
pub trait EnrollmentProofVerifier: Send + Sync {
    fn verify(&self, command: &EnrollmentSigningCommand) -> Result<(), PairingError>;
}
