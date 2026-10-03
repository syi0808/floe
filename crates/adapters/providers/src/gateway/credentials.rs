//! The only private Gateway credential. An exact missing slot differs from an
//! unreadable store, malformed record, unverified staging or rebound identity.
use floe_access::{GatewayAdmission, GatewayTrustReader, VerifiedGatewayBinding};
use floe_agent_contract::{AgentFailure, ModelBindingDigest};
use floe_execution::{BoxFuture, ExecutionScope};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;
use zeroize::Zeroizing;

const SERVICE: &str = "app.floe.local-server";
const ACCOUNT: &str = "connection-v1";
const MAX_RECORD_BYTES: usize = 131_072;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GatewayCredentialError {
    Locked,
    Unavailable,
    Timeout,
    Malformed,
    Unverified,
    ForeignIdentity,
    Conflict,
}

pub(super) enum SetupStorageStage {
    Expectation,
    SlotLock,
    SlotRead,
    ExpectationRecheck,
    Staging,
    SlotWrite,
    SlotReadback,
    SlotWorker,
}

/// Diagnostic context is supplied only by Prepare; it never changes admission
/// or the error returned to the owner. No credential values are formatted.
pub(super) fn setup_failure(
    target_ref: Option<Uuid>,
    stage: SetupStorageStage,
    failure: GatewayCredentialError,
) -> GatewayCredentialError {
    #[cfg(debug_assertions)]
    if let Some(target_ref) = target_ref {
        tracing::warn!(
            %target_ref,
            stage = match stage {
                SetupStorageStage::Expectation => "expectation",
                SetupStorageStage::SlotLock => "slot_lock",
                SetupStorageStage::SlotRead => "slot_read",
                SetupStorageStage::ExpectationRecheck => "expectation_recheck",
                SetupStorageStage::Staging => "staging",
                SetupStorageStage::SlotWrite => "slot_write",
                SetupStorageStage::SlotReadback => "slot_readback",
                SetupStorageStage::SlotWorker => "slot_worker",
            },
            kind = match failure {
                GatewayCredentialError::Locked => "locked",
                GatewayCredentialError::Unavailable => "unavailable",
                GatewayCredentialError::Timeout => "timeout",
                GatewayCredentialError::Malformed => "malformed",
                GatewayCredentialError::Unverified => "unverified",
                GatewayCredentialError::ForeignIdentity => "foreign_identity",
                GatewayCredentialError::Conflict => "conflict",
            },
            "gateway_setup_storage_failure"
        );
    }
    #[cfg(not(debug_assertions))]
    let _ = (target_ref, stage);
    failure
}
#[derive(Clone)]
pub struct GatewayCredentialStore {
    trust: Arc<dyn GatewayTrustReader>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GatewayConnection {
    pub(crate) operation_id: Uuid,
    pub(crate) binding: VerifiedGatewayBinding,
    pub(crate) endpoint: String,
    pub(crate) bearer: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SecureGatewayRecord {
    pub schema_version: u32,
    pub generation: u64,
    #[serde(deserialize_with = "required_option")]
    pub forgotten: Option<ForgottenGateway>,
    #[serde(deserialize_with = "required_option")]
    pub committed: Option<GatewayConnection>,
    #[serde(deserialize_with = "required_option")]
    pub staging: Option<super::pairing::StagedPairing>,
}
impl Default for SecureGatewayRecord {
    fn default() -> Self {
        Self {
            schema_version: 1,
            generation: 0,
            forgotten: None,
            committed: None,
            staging: None,
        }
    }
}
fn store_lock() -> &'static Arc<tokio::sync::Mutex<()>> {
    static LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();
    LOCK.get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
}
impl GatewayCredentialStore {
    pub fn new(trust: Arc<dyn GatewayTrustReader>) -> Self {
        Self { trust }
    }
    pub(crate) fn trust(&self) -> Arc<dyn GatewayTrustReader> {
        self.trust.clone()
    }
    pub(crate) async fn load(
        &self,
        person: &str,
        device: &str,
    ) -> Result<Option<GatewayConnection>, GatewayCredentialError> {
        let record = read_record().await?;
        let expectation = self
            .trust
            .credential_expectation()
            .await
            .map_err(|_| GatewayCredentialError::Unavailable)?;
        let Some(connection) = record.committed else {
            if record.staging.is_some() {
                return Err(GatewayCredentialError::Unverified);
            }
            return match expectation {
                floe_access::GatewayCredentialExpectation::Unpaired
                    if record.generation == 0 && record.forgotten.is_none() =>
                {
                    Ok(None)
                }
                floe_access::GatewayCredentialExpectation::Forgotten {
                    operation_id,
                    generation,
                } => {
                    if record.forgotten.as_ref().is_some_and(|receipt| {
                        receipt.summary.gateway_ref != operation_id
                            || receipt
                                .expected
                                .as_ref()
                                .is_some_and(|binding| binding.credential_generation != generation)
                    }) {
                        Err(GatewayCredentialError::ForeignIdentity)
                    } else {
                        Ok(None)
                    }
                }
                _ => Err(GatewayCredentialError::Unverified),
            };
        };
        if expectation
            != (floe_access::GatewayCredentialExpectation::Committed {
                operation_id: connection.operation_id,
                generation: connection.binding.credential_generation,
            })
        {
            return Err(GatewayCredentialError::ForeignIdentity);
        }
        connection.validate()?;
        if connection.binding.person_id != person || connection.binding.device_id != device {
            return Err(GatewayCredentialError::ForeignIdentity);
        }
        self.validate_pin(&connection.binding).await?;
        Ok(Some(connection))
    }
    pub(crate) async fn expectation(
        &self,
    ) -> Result<floe_access::GatewayCredentialExpectation, GatewayCredentialError> {
        self.trust
            .credential_expectation()
            .await
            .map_err(|_| GatewayCredentialError::Unavailable)
    }
    pub(crate) async fn next_generation(&self) -> Result<u64, GatewayCredentialError> {
        let record = read_record().await?;
        let prior = match self
            .trust
            .credential_expectation()
            .await
            .map_err(|_| GatewayCredentialError::Unavailable)?
        {
            floe_access::GatewayCredentialExpectation::Committed { generation, .. }
            | floe_access::GatewayCredentialExpectation::Forgotten { generation, .. } => generation,
            _ => 0,
        };
        record
            .generation
            .max(prior)
            .checked_add(1)
            .ok_or(GatewayCredentialError::Conflict)
    }
    pub(crate) async fn validate_pin(
        &self,
        binding: &VerifiedGatewayBinding,
    ) -> Result<(), GatewayCredentialError> {
        let pin = self
            .trust
            .pinned_producer()
            .await
            .map_err(|_| GatewayCredentialError::Unavailable)?;
        if pin.instance_id != binding.producer_instance
            || pin.fingerprint != binding.producer_key_fingerprint
            || pin.audience != binding.producer_audience
        {
            return Err(GatewayCredentialError::ForeignIdentity);
        }
        Ok(())
    }
    pub(crate) async fn mutate<T: Send + 'static>(
        &self,
        update: impl FnOnce(&mut SecureGatewayRecord) -> Result<T, GatewayCredentialError>,
    ) -> Result<T, GatewayCredentialError> {
        self.mutate_record(None, update).await
    }
    pub(super) async fn mutate_setup<T: Send + 'static>(
        &self,
        target_ref: Uuid,
        update: impl FnOnce(&mut SecureGatewayRecord) -> Result<T, GatewayCredentialError>,
    ) -> Result<T, GatewayCredentialError> {
        self.mutate_record(Some(target_ref), update).await
    }
    async fn mutate_record<T: Send + 'static>(
        &self,
        target_ref: Option<Uuid>,
        update: impl FnOnce(&mut SecureGatewayRecord) -> Result<T, GatewayCredentialError>,
    ) -> Result<T, GatewayCredentialError> {
        let guard = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            store_lock().clone().lock_owned(),
        )
        .await
        .map_err(|_| setup_failure(target_ref, SetupStorageStage::SlotLock, GatewayCredentialError::Timeout))?;
        let mut record = read_record().await
            .map_err(|failure| setup_failure(target_ref, SetupStorageStage::SlotRead, failure))?;
        let floor = match self
            .trust
            .credential_expectation()
            .await
            .map_err(|_| setup_failure(target_ref, SetupStorageStage::ExpectationRecheck, GatewayCredentialError::Unavailable))?
        {
            floe_access::GatewayCredentialExpectation::Committed { generation, .. }
            | floe_access::GatewayCredentialExpectation::Forgotten { generation, .. } => generation,
            _ => 0,
        };
        record.generation = record.generation.max(floor);
        let result = update(&mut record)
            .map_err(|failure| setup_failure(target_ref, SetupStorageStage::Staging, failure))?;
        let bytes = Zeroizing::new(
            serde_json::to_vec(&record).map_err(|_| setup_failure(target_ref, SetupStorageStage::Staging, GatewayCredentialError::Malformed))?,
        );
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(setup_failure(target_ref, SetupStorageStage::Staging, GatewayCredentialError::Malformed));
        }
        // The owned guard stays with the worker even if the observer times out.
        // A late OS write cannot race a subsequent mutation or become absence.
        let work = tokio::task::spawn_blocking(move || {
            let _guard = guard;
            floe_native::write_generic_password(SERVICE, ACCOUNT, &bytes)
                .map_err(|error| setup_failure(target_ref, SetupStorageStage::SlotWrite, native_error(error)))?;
            let readback = Zeroizing::new(
                floe_native::read_generic_password(SERVICE, ACCOUNT, MAX_RECORD_BYTES)
                    .map_err(|error| setup_failure(target_ref, SetupStorageStage::SlotReadback, native_error(error)))?
                    .ok_or_else(|| setup_failure(target_ref, SetupStorageStage::SlotReadback, GatewayCredentialError::Unavailable))?,
            );
            if readback.as_slice() != bytes.as_slice() {
                return Err(setup_failure(target_ref, SetupStorageStage::SlotReadback, GatewayCredentialError::Conflict));
            }
            Ok(result)
        });
        tokio::time::timeout(std::time::Duration::from_secs(3), work)
            .await
            .map_err(|_| setup_failure(target_ref, SetupStorageStage::SlotWorker, GatewayCredentialError::Timeout))?
            .map_err(|_| setup_failure(target_ref, SetupStorageStage::SlotWorker, GatewayCredentialError::Unavailable))?
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
    pub(crate) async fn committed_operation(
        &self,
        operation_id: Uuid,
    ) -> Result<Option<GatewayConnection>, GatewayCredentialError> {
        let record = read_record().await?;
        match record.committed {
            Some(connection) if connection.operation_id == operation_id => {
                connection.validate()?;
                self.validate_pin(&connection.binding).await?;
                Ok(Some(connection))
            }
            _ => Ok(None),
        }
    }
}
impl GatewayConnection {
    fn validate(&self) -> Result<(), GatewayCredentialError> {
        if self.operation_id.is_nil()
            || self.binding.validate().is_err()
            || !super::http::valid_endpoint(&self.endpoint)
            || !valid_token(&self.bearer)
        {
            return Err(GatewayCredentialError::Malformed);
        }
        Ok(())
    }
    pub(crate) fn binding_digest(&self) -> ModelBindingDigest {
        binding_digest(&self.binding)
    }
}
pub(crate) fn binding_digest(binding: &VerifiedGatewayBinding) -> ModelBindingDigest {
    ModelBindingDigest(binding.binding_digest())
}
pub(crate) fn valid_token(value: &str) -> bool {
    (32..=256).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn read_record_blocking() -> Result<SecureGatewayRecord, GatewayCredentialError> {
    let Some(bytes) = floe_native::read_generic_password(SERVICE, ACCOUNT, MAX_RECORD_BYTES)
        .map_err(native_error)?
    else {
        return Ok(SecureGatewayRecord::default());
    };
    let bytes = Zeroizing::new(bytes);
    parse_record(&bytes)
}
fn parse_record(bytes: &[u8]) -> Result<SecureGatewayRecord, GatewayCredentialError> {
    super::json::strict_json_bytes(bytes, MAX_RECORD_BYTES)
        .map_err(|_| GatewayCredentialError::Malformed)?;
    let record: SecureGatewayRecord =
        serde_json::from_slice(bytes).map_err(|_| GatewayCredentialError::Malformed)?;
    if record.schema_version != 1 {
        return Err(GatewayCredentialError::Malformed);
    }
    if let Some(connection) = &record.committed {
        connection.validate()?;
        if connection.binding.credential_generation != record.generation {
            return Err(GatewayCredentialError::Malformed);
        }
    }
    Ok(record)
}

fn native_error(error: floe_native::KeychainError) -> GatewayCredentialError {
    match error {
        floe_native::KeychainError::Locked => GatewayCredentialError::Locked,
        floe_native::KeychainError::Ambiguous | floe_native::KeychainError::TooLarge => {
            GatewayCredentialError::Malformed
        }
        floe_native::KeychainError::Unavailable => GatewayCredentialError::Unavailable,
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

fn required_option<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ForgottenGateway {
    operation_id: Uuid,
    expected: Option<VerifiedGatewayBinding>,
    summary: floe_connections::GatewaySummary,
}
impl floe_connections::GatewayRegistry for GatewayCredentialStore {
    fn current<'a>(
        &'a self,
        person: floe_kernel::PersonId,
        device: &'a str,
    ) -> BoxFuture<
        'a,
        Result<Option<floe_connections::GatewayObservation>, floe_connections::PairingError>,
    > {
        Box::pin(async move {
            use floe_access::GatewayCredentialExpectation as E;
            use floe_connections::*;
            let expectation = self
                .trust
                .credential_expectation()
                .await
                .map_err(|_| PairingError::StorageUnavailable)?;
            let raw = read_raw().await.map_err(registry_failure)?;
            let slot_digest = raw
                .as_ref()
                .map(|bytes| <[u8; 32]>::from(Sha256::digest(bytes.as_slice())));
            let parsed = match raw.as_ref() {
                Some(bytes) => parse_record(bytes),
                None => Ok(SecureGatewayRecord::default()),
            };
            if let Ok(record) = parsed {
                if let Some(connection) = record.committed {
                    if expectation
                        == (E::Committed {
                            operation_id: connection.operation_id,
                            generation: connection.binding.credential_generation,
                        })
                    {
                        if connection.binding.person_id != person.to_string()
                            || connection.binding.device_id != device
                        {
                            return Err(PairingError::ForeignIdentity);
                        }
                        self.validate_pin(&connection.binding)
                            .await
                            .map_err(registry_failure)?;
                        return Ok(Some(GatewayObservation::Paired {
                            summary: gateway_summary(
                                connection.operation_id,
                                connection.binding.credential_generation,
                                GatewayState::Paired,
                                false,
                            ),
                            binding: connection.binding,
                        }));
                    }
                } else if matches!(expectation, E::Unpaired | E::Forgotten { .. }) {
                    return Ok(None);
                }
            }
            let (id, revision) = match expectation {
                E::Committed {
                    operation_id,
                    generation,
                }
                | E::Forgotten {
                    operation_id,
                    generation,
                } => (operation_id, generation.max(1)),
                E::Pending { operation_id } => (operation_id, 1),
                E::Unpaired => {
                    let hash =
                        Sha256::digest(format!("floe.gateway.repair.v1\0{person}\0{device}"));
                    let mut bytes = [0u8; 16];
                    bytes.copy_from_slice(&hash[..16]);
                    bytes[6] = (bytes[6] & 15) | 64;
                    bytes[8] = (bytes[8] & 63) | 128;
                    (Uuid::from_bytes(bytes), 1)
                }
            };
            let mut summary = gateway_summary(id, revision, GatewayState::RepairRequired, true);
            summary.allowed_actions = vec![ConnectionAction::Forget];
            summary.failure = Some(ConnectionFailure {
                domain: ConnectionFailureDomain::Connections,
                category: floe_kernel::AgentFailureCategory::Integrity,
                reason: ConnectionFailureReason::IdentityChanged,
                incident_id: id,
                correlation_id: id,
                reload_required: true,
                seal_session: false,
                recovery: ConnectionRecovery::Reconcile,
                safe_actions: vec![ConnectionAction::Forget],
            });
            Ok(Some(GatewayObservation::RepairRequired {
                summary,
                slot_digest,
                expectation,
            }))
        })
    }
    fn forget<'a>(
        &'a self,
        operation_id: Uuid,
        expected: floe_connections::GatewayForgetExpectation,
    ) -> BoxFuture<'a, Result<floe_connections::GatewaySummary, floe_connections::PairingError>>
    {
        Box::pin(async move {
            use floe_connections::*;
            if operation_id.is_nil() {
                return Err(PairingError::InvalidInput);
            }
            match expected {
                GatewayForgetExpectation::Paired(expected) => self
                    .mutate(|record| {
                        if let Some(previous) = &record.forgotten {
                            if previous.operation_id == operation_id {
                                if previous.expected.as_ref() != Some(&expected) {
                                    return Err(GatewayCredentialError::Conflict);
                                }
                                return Ok(previous.summary.clone());
                            }
                        }
                        let current = record
                            .committed
                            .as_ref()
                            .ok_or(GatewayCredentialError::Conflict)?;
                        if current.binding != expected {
                            return Err(GatewayCredentialError::ForeignIdentity);
                        }
                        let revision = record
                            .generation
                            .checked_add(1)
                            .ok_or(GatewayCredentialError::Conflict)?;
                        let summary = gateway_summary(
                            current.operation_id,
                            revision,
                            GatewayState::Forgotten,
                            true,
                        );
                        record.forgotten = Some(ForgottenGateway {
                            operation_id,
                            expected: Some(expected.clone()),
                            summary: summary.clone(),
                        });
                        record.committed = None;
                        record.generation = revision;
                        Ok(summary)
                    })
                    .await
                    .map_err(registry_failure),
                GatewayForgetExpectation::Unreadable {
                    gateway_ref,
                    revision,
                    slot_digest,
                    expectation,
                } => {
                    let guard = tokio::time::timeout(
                        std::time::Duration::from_secs(3),
                        store_lock().clone().lock_owned(),
                    )
                    .await
                    .map_err(|_| PairingError::CredentialUnavailable)?;
                    if self
                        .trust
                        .credential_expectation()
                        .await
                        .map_err(|_| PairingError::StorageUnavailable)?
                        != expectation
                    {
                        return Err(PairingError::Conflict);
                    }
                    let raw = read_raw().await.map_err(registry_failure)?;
                    if raw
                        .as_ref()
                        .map(|bytes| <[u8; 32]>::from(Sha256::digest(bytes.as_slice())))
                        != slot_digest
                    {
                        return Err(PairingError::Conflict);
                    }
                    let next = revision.checked_add(1).ok_or(PairingError::Conflict)?;
                    let summary = gateway_summary(gateway_ref, next, GatewayState::Forgotten, true);
                    let record = SecureGatewayRecord {
                        schema_version: 1,
                        generation: next,
                        forgotten: Some(ForgottenGateway {
                            operation_id,
                            expected: None,
                            summary: summary.clone(),
                        }),
                        committed: None,
                        staging: None,
                    };
                    let bytes = Zeroizing::new(
                        serde_json::to_vec(&record)
                            .map_err(|_| PairingError::StorageUnavailable)?,
                    );
                    let work = tokio::task::spawn_blocking(move || {
                        let _guard = guard;
                        let current =
                            floe_native::read_generic_password(SERVICE, ACCOUNT, MAX_RECORD_BYTES)
                                .map_err(native_error)?;
                        if current
                            .as_ref()
                            .map(|bytes| <[u8; 32]>::from(Sha256::digest(bytes.as_slice())))
                            != slot_digest
                        {
                            return Err(GatewayCredentialError::Conflict);
                        }
                        floe_native::write_generic_password(SERVICE, ACCOUNT, &bytes)
                            .map_err(native_error)?;
                        let actual = Zeroizing::new(
                            floe_native::read_generic_password(SERVICE, ACCOUNT, MAX_RECORD_BYTES)
                                .map_err(native_error)?
                                .ok_or(GatewayCredentialError::Unavailable)?,
                        );
                        if actual.as_slice() != bytes.as_slice() {
                            return Err(GatewayCredentialError::Conflict);
                        }
                        Ok(summary)
                    });
                    tokio::time::timeout(std::time::Duration::from_secs(3), work)
                        .await
                        .map_err(|_| PairingError::CredentialUnavailable)?
                        .map_err(|_| PairingError::CredentialUnavailable)?
                        .map_err(registry_failure)
                }
            }
        })
    }
    fn forgotten<'a>(
        &'a self,
        operation_id: Uuid,
    ) -> BoxFuture<
        'a,
        Result<Option<floe_connections::GatewaySummary>, floe_connections::PairingError>,
    > {
        Box::pin(async move {
            Ok(read_record()
                .await
                .map_err(registry_failure)?
                .forgotten
                .filter(|receipt| receipt.operation_id == operation_id)
                .map(|receipt| receipt.summary))
        })
    }
}
fn gateway_summary(
    id: Uuid,
    revision: u64,
    state: floe_connections::GatewayState,
    remote_revocation_pending: bool,
) -> floe_connections::GatewaySummary {
    use floe_connections::*;
    GatewaySummary {
        gateway_ref: id,
        revision,
        display_name: "Gateway".into(),
        state,
        remote_revocation_pending,
        allowed_actions: if state == GatewayState::Paired {
            vec![ConnectionAction::Forget, ConnectionAction::Manage]
        } else {
            vec![ConnectionAction::Pair]
        },
        failure: None,
    }
}
fn registry_failure(error: GatewayCredentialError) -> floe_connections::PairingError {
    match error {
        GatewayCredentialError::ForeignIdentity => floe_connections::PairingError::ForeignIdentity,
        GatewayCredentialError::Conflict => floe_connections::PairingError::Conflict,
        GatewayCredentialError::Timeout => floe_connections::PairingError::DeadlineExceeded,
        _ => floe_connections::PairingError::CredentialUnavailable,
    }
}
pub(crate) async fn read_record() -> Result<SecureGatewayRecord, GatewayCredentialError> {
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::task::spawn_blocking(read_record_blocking),
    )
    .await
    .map_err(|_| GatewayCredentialError::Timeout)?
    .map_err(|_| GatewayCredentialError::Unavailable)?
}
async fn read_raw() -> Result<Option<Zeroizing<Vec<u8>>>, GatewayCredentialError> {
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::task::spawn_blocking(|| {
            floe_native::read_generic_password(SERVICE, ACCOUNT, MAX_RECORD_BYTES)
                .map(|bytes| bytes.map(Zeroizing::new))
                .map_err(native_error)
        }),
    )
    .await
    .map_err(|_| GatewayCredentialError::Timeout)?
    .map_err(|_| GatewayCredentialError::Unavailable)?
}
