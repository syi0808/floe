use crate::ports::gateway_pairing::*;
use crate::{ConnectionsCommandFailure, PairingActivationResult, PairingApproval, StartedPairing};
use floe_execution::BoxFuture;
use floe_kernel::OwnerActor;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingState {
    #[serde(rename = "starting")]
    Pending,
    AwaitingLocalConfirmation,
    #[serde(rename = "awaiting_gateway_approval")]
    AwaitingApproval,
    Cancelling,
    #[serde(rename = "connected")]
    Paired,
    Rejected,
    Expired,
    Cancelled,
    RepairRequired,
    RevocationPending,
    Forgotten,
}
impl PairingState {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Paired
                | Self::Rejected
                | Self::Expired
                | Self::Cancelled
                | Self::RepairRequired
                | Self::RevocationPending
                | Self::Forgotten
        )
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingStartPhase {
    Staged,
    Dispatched,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingRecord {
    pub operation_id: Uuid,
    pub command_id: Uuid,
    pub person_id: floe_kernel::PersonId,
    pub device_id: String,
    pub setup: GatewaySetup,
    pub start_phase: PairingStartPhase,
    pub last_failure: Option<PairingError>,
    pub prior_credential_expectation: floe_access::GatewayCredentialExpectation,
    pub revision: u64,
    pub generation: u64,
    pub state: PairingState,
    pub handle: Option<PairingHandle>,
    pub display_code: Option<String>,
    pub expires_at_unix_ms: Option<i64>,
    pub reviewed: Option<ReviewedGatewayIdentity>,
    pub enrollment: Option<VerifiedEnrollment>,
    pub gateway: Option<GatewaySummary>,
    /// Durable authorization for the client key-possession step, derived from Start.
    /// This is not evidence that a human compared the displayed codes.
    pub confirmation_command: Option<Uuid>,
    pub cancellation_command: Option<Uuid>,
    pub forgotten_command: Option<Uuid>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PairingAdmission {
    pub operation_id: Uuid,
    pub command_id: Uuid,
    pub person_id: floe_kernel::PersonId,
    pub device_id: String,
    pub setup: GatewaySetup,
}
impl PairingRecord {
    pub fn admit(
        intent: PairingAdmission,
        prior: floe_access::GatewayCredentialExpectation,
        generation: u64,
    ) -> Result<Self, PairingError> {
        let record = Self {
            operation_id: intent.operation_id,
            command_id: intent.command_id,
            person_id: intent.person_id,
            device_id: intent.device_id,
            setup: intent.setup,
            start_phase: PairingStartPhase::Staged,
            last_failure: None,
            prior_credential_expectation: prior,
            revision: 1,
            generation,
            state: PairingState::Pending,
            handle: None,
            display_code: None,
            expires_at_unix_ms: None,
            reviewed: None,
            enrollment: None,
            gateway: None,
            confirmation_command: None,
            cancellation_command: None,
            forgotten_command: None,
        };
        record.validate()?;
        Ok(record)
    }
    pub fn validate(&self) -> Result<(), PairingError> {
        if self.operation_id.is_nil()
            || self.command_id.is_nil()
            || self.setup.validate().is_err()
            || !self.person_id.is_valid()
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.revision == 0
            || self.revision > i64::MAX as u64
            || self.generation == 0
            || self.generation > i64::MAX as u64
        {
            return Err(PairingError::InvalidInput);
        }
        let prior_generation = match self.prior_credential_expectation {
            floe_access::GatewayCredentialExpectation::Unpaired => 0,
            floe_access::GatewayCredentialExpectation::Forgotten {
                operation_id,
                generation,
            } if !operation_id.is_nil() && generation > 0 => generation,
            _ => return Err(PairingError::Conflict),
        };
        if self.generation <= prior_generation
            || self.handle.as_ref().is_some_and(|h| {
                h.operation_id != self.operation_id
                    || h.generation != self.generation
                    || h.attempt_id.is_nil()
            })
            || (self.handle.is_some() && self.start_phase != PairingStartPhase::Dispatched)
            || (!matches!(
                self.state,
                PairingState::Pending
                    | PairingState::RepairRequired
                    | PairingState::Cancelled
                    | PairingState::Forgotten
            ) && self.handle.is_none())
            || (self.gateway.is_some() != self.enrollment.is_some())
            || (self.state == PairingState::Paired && self.enrollment.is_none())
            || (self.enrollment.is_some()
                && !matches!(self.state, PairingState::Paired | PairingState::Forgotten))
            || (self.state == PairingState::Forgotten && self.forgotten_command.is_none())
            || (self.state == PairingState::Cancelling && self.cancellation_command.is_none())
            || self.confirmation_command.is_some_and(|id| id.is_nil())
            || self.cancellation_command.is_some_and(|id| id.is_nil())
            || self.forgotten_command.is_some_and(|id| id.is_nil())
        {
            return Err(PairingError::Conflict);
        }
        Ok(())
    }
    pub fn can_reconcile_repair(&self) -> bool {
        self.state == PairingState::RepairRequired
            && self.handle.is_some()
            && self.reviewed.is_some()
            && self.confirmation_command.is_some()
            && self.cancellation_command.is_none()
            && self.forgotten_command.is_none()
            && !matches!(
                self.last_failure,
                Some(
                    PairingError::ForeignIdentity
                        | PairingError::ChangedProducer
                        | PairingError::Rejected
                )
            )
    }
    pub fn validate_successor(&self, next: &Self, expected: u64) -> Result<(), PairingError> {
        self.validate()?;
        next.validate()?;
        if self.revision != expected
            || self.revision.checked_add(1) != Some(next.revision)
            || (self.state.terminal() && !self.can_reconcile_repair())
            || self.operation_id != next.operation_id
            || self.command_id != next.command_id
            || self.person_id != next.person_id
            || self.device_id != next.device_id
            || self.setup != next.setup
            || self.prior_credential_expectation != next.prior_credential_expectation
            || self.generation != next.generation
            || self.forgotten_command != next.forgotten_command
            || self
                .handle
                .as_ref()
                .is_some_and(|v| next.handle.as_ref() != Some(v))
            || self
                .reviewed
                .as_ref()
                .is_some_and(|v| next.reviewed.as_ref() != Some(v))
            || self
                .confirmation_command
                .is_some_and(|v| next.confirmation_command != Some(v))
            || self
                .cancellation_command
                .is_some_and(|v| next.cancellation_command != Some(v))
            || (self.start_phase != next.start_phase
                && !(self.start_phase == PairingStartPhase::Staged
                    && next.start_phase == PairingStartPhase::Dispatched
                    && self.state == PairingState::Pending
                    && next.state == PairingState::Pending
                    && self.cancellation_command.is_none()))
        {
            return Err(PairingError::Conflict);
        }
        if next.state == PairingState::Cancelling
            && (next.cancellation_command.is_none() || next.handle.is_none())
        {
            return Err(PairingError::Conflict);
        }
        if next.state == PairingState::Paired {
            return Err(PairingError::Conflict);
        } // activate is the sole atomic publication path
        if !matches!(
            (self.state, next.state),
            (
                PairingState::Pending,
                PairingState::AwaitingLocalConfirmation
            ) | (
                PairingState::AwaitingLocalConfirmation,
                PairingState::AwaitingApproval
            )
        ) && next.state != self.state
            && !matches!(
                next.state,
                PairingState::Cancelling
                    | PairingState::Rejected
                    | PairingState::Expired
                    | PairingState::Cancelled
                    | PairingState::RepairRequired
            )
        {
            return Err(PairingError::Conflict);
        }
        Ok(())
    }
    pub fn accept_started(&self, started: &StartedPairing) -> Result<Self, PairingError> {
        let c = &started.challenge;
        let e = &started.enrollment;
        if !(self.state == PairingState::Pending
            || (self.state == PairingState::RepairRequired && self.cancellation_command.is_some())
            || (self.state == PairingState::Forgotten && self.forgotten_command.is_some()))
            || self.handle.is_some()
            || self.start_phase != PairingStartPhase::Dispatched
            || c.handle.operation_id != self.operation_id
            || c.handle.generation != self.generation
            || e.operation_id != self.operation_id
            || e.person_id != self.person_id
            || e.device_id != self.device_id
            || e.client_id != c.handle.attempt_id.to_string()
            || e.issuer != c.reviewed.issuer
            || e.producer != c.reviewed.producer
            || e.expires_at_unix_ms != c.expires_at_unix_ms
            || c.display_code.is_empty()
            || c.display_code.len() > 32
        {
            return Err(PairingError::ForeignIdentity);
        }
        let mut next = self.clone();
        next.revision = next.revision.checked_add(1).ok_or(PairingError::Conflict)?;
        next.handle = Some(c.handle.clone());
        next.reviewed = Some(c.reviewed.clone());
        next.display_code = Some(c.display_code.clone());
        next.expires_at_unix_ms = Some(c.expires_at_unix_ms);
        next.state = if self.forgotten_command.is_some() {
            PairingState::Forgotten
        } else if self.cancellation_command.is_some() {
            PairingState::Cancelling
        } else {
            PairingState::AwaitingLocalConfirmation
        };
        next.last_failure = None;
        // A late exact Start response adds recovery evidence. It never grants
        // confirmation or reverses cancellation/Forget.
        next.validate()?;
        Ok(next)
    }
    pub fn validate_approval(
        &self,
        approval: &PairingApproval,
        enrolled: &EnrollmentSigningCommand,
    ) -> Result<(), PairingError> {
        if self.handle.as_ref() != Some(&approval.handle)
            || self.reviewed.as_ref() != Some(&approval.reviewed)
            || self.person_id != approval.person_id
            || self.device_id != approval.device_id
            || self.confirmation_command.is_none()
            || enrolled.operation_id != self.operation_id
            || enrolled.person_id != self.person_id
            || enrolled.device_id != self.device_id
            || enrolled.client_id != approval.handle.attempt_id.to_string()
            || enrolled.issuer != approval.reviewed.issuer
            || enrolled.producer != approval.reviewed.producer
        {
            return Err(PairingError::ForeignIdentity);
        }
        Ok(())
    }
    pub fn activate(
        &self,
        approval: &PairingApproval,
        enrolled: &EnrollmentSigningCommand,
        pin_revision: u64,
    ) -> Result<Self, PairingError> {
        self.validate_approval(approval, enrolled)?;
        if self.forgotten_command.is_some()
            || self.cancellation_command.is_some()
            || !(self.state == PairingState::AwaitingApproval || self.can_reconcile_repair())
        {
            return Err(PairingError::Conflict);
        }
        let binding = floe_access::VerifiedGatewayBinding {
            person_id: self.person_id.to_string(),
            device_id: self.device_id.clone(),
            client_id: enrolled.client_id.clone(),
            producer_instance: enrolled.producer.instance_id.clone(),
            producer_key_fingerprint: enrolled.producer.fingerprint.clone(),
            producer_audience: enrolled.producer.audience.clone(),
            enrollment_id: enrolled.challenge_id.to_string(),
            credential_generation: self.generation,
        };
        binding
            .validate()
            .map_err(|_| PairingError::ForeignIdentity)?;
        let mut next = self.clone();
        next.revision = next.revision.checked_add(1).ok_or(PairingError::Conflict)?;
        next.enrollment = Some(VerifiedEnrollment {
            operation_id: self.operation_id,
            binding,
            issuer: enrolled.issuer.clone(),
            pin_revision,
        });
        next.gateway = Some(GatewaySummary {
            display_address: Some(self.setup.display_address.clone()),
            gateway_ref: self.operation_id,
            revision: self.generation,
            display_name: "Gateway".into(),
            state: GatewayState::Paired,
            remote_revocation_pending: false,
            allowed_actions: vec![ConnectionAction::Forget, ConnectionAction::Manage],
            failure: None,
        });
        next.state = PairingState::Paired;
        next.last_failure = None;
        next.validate()?;
        Ok(next)
    }
    pub fn forget(&self, command: Uuid) -> Result<Self, PairingError> {
        if command.is_nil() || self.forgotten_command.is_some() {
            return Err(PairingError::Conflict);
        }
        let mut next = self.clone();
        next.revision = next.revision.checked_add(1).ok_or(PairingError::Conflict)?;
        next.forgotten_command = Some(command);
        next.state = PairingState::Forgotten;
        if let Some(gateway) = next.gateway.as_mut() {
            gateway.state = GatewayState::Forgotten;
            gateway.revision = gateway
                .revision
                .checked_add(1)
                .ok_or(PairingError::Conflict)?;
            gateway.remote_revocation_pending = self.start_phase == PairingStartPhase::Dispatched;
            gateway.allowed_actions = vec![ConnectionAction::Pair];
        }
        next.last_failure = (self.start_phase == PairingStartPhase::Dispatched
            && self.state != PairingState::Paired)
            .then_some(PairingError::Indeterminate);
        next.validate()?;
        Ok(next)
    }
    pub fn allowed_actions(&self) -> Vec<ConnectionAction> {
        if self.forgotten_command.is_some() {
            vec![]
        } else {
            match self.state {
                PairingState::AwaitingLocalConfirmation => {
                    vec![ConnectionAction::Cancel, ConnectionAction::Reobserve]
                }
                PairingState::Pending | PairingState::AwaitingApproval => {
                    if self.cancellation_command.is_some() {
                        vec![ConnectionAction::Reobserve]
                    } else {
                        vec![ConnectionAction::Cancel, ConnectionAction::Reobserve]
                    }
                }
                PairingState::Cancelling => vec![ConnectionAction::Reobserve],
                PairingState::RepairRequired if self.can_reconcile_repair() => {
                    vec![ConnectionAction::Cancel, ConnectionAction::Reobserve]
                }
                _ => vec![],
            }
        }
    }
    pub fn snapshot(&self) -> PairingSnapshot {
        let actions = self.allowed_actions();
        PairingSnapshot {
            operation_ref: self.operation_id,
            revision: self.revision,
            state: self.state,
            display_code: self.display_code.clone(),
            expires_at: self
                .expires_at_unix_ms
                .and_then(chrono::DateTime::from_timestamp_millis),
            gateway: self.gateway.clone().map(|mut gateway| {
                gateway.display_address = Some(self.setup.display_address.clone());
                gateway
            }),
            failure: pairing_failure_projection(self),
            allowed_actions: actions,
            next_observation_after_ms: ((!self.state.terminal() || self.can_reconcile_repair())
                && self.forgotten_command.is_none())
            .then_some(2000),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingSnapshot {
    pub operation_ref: Uuid,
    pub revision: u64,
    pub state: PairingState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<GatewaySummary>,
    pub allowed_actions: Vec<ConnectionAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<crate::ConnectionFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_observation_after_ms: Option<u32>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewaySetupRecord {
    pub command_id: Uuid,
    pub person_id: floe_kernel::PersonId,
    pub device_id: String,
    pub address_digest: [u8; 32],
    pub setup: GatewaySetup,
}
/// Immutable owner intent; a stored terminal rejection fences every later replay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingMutation {
    pub command_id: Uuid,
    pub person_id: floe_kernel::PersonId,
    pub device_id: String,
    pub action: PairingMutationAction,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PairingMutationAction {
    Start {
        operation_id: Uuid,
        setup: GatewaySetup,
    },
    Confirm {
        operation_id: Uuid,
        expected_revision: u64,
    },
    Cancel {
        operation_id: Uuid,
        expected_revision: u64,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingMutationReceipt {
    pub command: PairingMutation,
    pub outcome: Result<PairingSnapshot, PairingError>,
}
impl PairingRecord {
    pub fn decide_command(&self, command: &PairingMutation) -> Result<Self, PairingError> {
        if self.person_id != command.person_id || self.device_id != command.device_id {
            return Err(PairingError::ForeignIdentity);
        }
        let (operation_id, expected, cancelling) = match command.action {
            PairingMutationAction::Confirm {
                operation_id,
                expected_revision,
            } => (operation_id, expected_revision, false),
            PairingMutationAction::Cancel {
                operation_id,
                expected_revision,
            } => (operation_id, expected_revision, true),
            PairingMutationAction::Start { .. } => return Err(PairingError::InvalidInput),
        };
        if operation_id != self.operation_id
            || self.revision != expected
            || self.forgotten_command.is_some()
        {
            return Err(PairingError::Conflict);
        }
        let mut next = self.clone();
        next.revision = self.revision.checked_add(1).ok_or(PairingError::Conflict)?;
        if cancelling {
            if self.state.terminal() && !self.can_reconcile_repair() {
                return Err(PairingError::Conflict);
            }
            if self.cancellation_command.is_some() {
                return Err(PairingError::Conflict);
            }
            next.cancellation_command = Some(command.command_id);
            if self.start_phase == PairingStartPhase::Staged {
                next.state = PairingState::Cancelled;
            } else if self.handle.is_some() {
                next.state = PairingState::Cancelling;
            }
            next.last_failure = None;
        } else {
            if self.state != PairingState::AwaitingLocalConfirmation
                || self.confirmation_command.is_some()
                || self.cancellation_command.is_some()
            {
                return Err(PairingError::Conflict);
            }
            next.confirmation_command = Some(command.command_id);
            next.state = PairingState::AwaitingApproval;
        }
        self.validate_successor(&next, self.revision)?;
        Ok(next)
    }
}
pub trait PairingRepository: Send + Sync {
    fn setup<'a>(
        &'a self,
        target: Uuid,
    ) -> BoxFuture<'a, Result<Option<GatewaySetupRecord>, PairingError>>;
    fn store_setup<'a>(
        &'a self,
        record: GatewaySetupRecord,
    ) -> BoxFuture<'a, Result<GatewaySetupRecord, PairingError>>;
    /// Both success and semantic rejection are committed with this exact intent.
    /// An outer error means the receipt cannot be established, not a rejection.
    fn execute_command<'a>(
        &'a self,
        command: PairingMutation,
    ) -> BoxFuture<'a, Result<PairingMutationReceipt, PairingError>>;
    fn load<'a>(
        &'a self,
        operation: Uuid,
    ) -> BoxFuture<'a, Result<Option<PairingRecord>, PairingError>>;
    fn compare_and_swap<'a>(
        &'a self,
        expected: u64,
        next: PairingRecord,
    ) -> BoxFuture<'a, Result<PairingRecord, PairingError>>;
    fn accept_started<'a>(
        &'a self,
        expected: u64,
        started: StartedPairing,
    ) -> BoxFuture<'a, Result<PairingRecord, PairingError>>;
    fn activate<'a>(
        &'a self,
        expected: u64,
        approval: PairingApproval,
    ) -> BoxFuture<'a, Result<PairingActivationResult, PairingError>>;
    fn pending<'a>(
        &'a self,
        person: floe_kernel::PersonId,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<PairingRecord>, PairingError>>;
}
pub struct GatewayPairingService {
    repository: Arc<dyn PairingRepository>,
    transport: Arc<dyn GatewayPairingPort>,
    reconciling: std::sync::Mutex<std::collections::HashSet<Uuid>>,
}
impl GatewayPairingService {
    pub fn new(
        repository: Arc<dyn PairingRepository>,
        transport: Arc<dyn GatewayPairingPort>,
    ) -> Self {
        Self {
            repository,
            transport,
            reconciling: Default::default(),
        }
    }
    pub async fn pending(
        &self,
        person: floe_kernel::PersonId,
    ) -> Result<Vec<PairingRecord>, PairingError> {
        self.repository.pending(person, 64).await
    }
    async fn prepare_gateway_setup(
        &self,
        actor: &OwnerActor,
        command: Uuid,
        address: &str,
        scope: &OperationScope,
    ) -> Result<GatewaySetup, ConnectionsCommandFailure> {
        use sha2::{Digest, Sha256};
        check(actor, scope).map_err(not_admitted)?;
        if command.is_nil() || address.len() > 2048 {
            return Err(not_admitted(PairingError::InvalidInput));
        }
        let target =
            super::source_operation::derived_id(b"floe.gateway.setup.v1", actor.person_id, command);
        let digest = Sha256::digest(address.as_bytes()).into();
        if let Some(saved) = self.repository.setup(target).await.map_err(indeterminate)? {
            if saved.command_id != command
                || saved.person_id != actor.person_id
                || saved.device_id != actor.device_id
                || saved.address_digest != digest
            {
                return Err(not_admitted(PairingError::Conflict));
            }
            saved.setup.validate().map_err(indeterminate)?;
            return Ok(saved.setup);
        }
        let setup = GatewaySetup {
            target_ref: target,
            display_address: GatewaySetup::canonical_address(address).map_err(not_admitted)?,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
        };
        check(actor, scope).map_err(not_admitted)?;
        Ok(self
            .repository
            .store_setup(GatewaySetupRecord {
                command_id: command,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                address_digest: digest,
                setup,
            })
            .await
            .map_err(indeterminate)?
            .setup)
    }
    pub async fn start_pairing(
        &self,
        actor: &OwnerActor,
        command: Uuid,
        address: &str,
        scope: &OperationScope,
    ) -> Result<PairingSnapshot, ConnectionsCommandFailure> {
        check(actor, scope).map_err(not_admitted)?;
        if command.is_nil() {
            return Err(not_admitted(PairingError::InvalidInput));
        }
        let operation = super::source_operation::derived_id(
            b"floe.pairing.operation.v1",
            actor.person_id,
            command,
        );
        // Preparation is an internal replayable prerequisite of this one
        // user intent. Its identity is stable across lost replies and retries.
        let preparation = super::source_operation::derived_id(
            b"floe.pairing.prepare.v1",
            actor.person_id,
            command,
        );
        let setup = self
            .prepare_gateway_setup(actor, preparation, address, scope)
            .await?;
        self.execute_command(
            actor,
            command,
            PairingMutationAction::Start {
                operation_id: operation,
                setup,
            },
        )
        .await
    }
    async fn execute_command(
        &self,
        actor: &OwnerActor,
        command: Uuid,
        action: PairingMutationAction,
    ) -> Result<PairingSnapshot, ConnectionsCommandFailure> {
        let intent = PairingMutation {
            command_id: command,
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            action,
        };
        let receipt = self
            .repository
            .execute_command(intent.clone())
            .await
            .map_err(indeterminate)?;
        if receipt.command != intent {
            return Err(indeterminate(PairingError::Conflict));
        }
        receipt.outcome.map_err(|error| {
            ConnectionsCommandFailure::NotApplied(super::product::pairing_error(error))
        })
    }
    pub async fn cancel_pairing(
        &self,
        actor: &OwnerActor,
        command: Uuid,
        id: Uuid,
        expected: u64,
        scope: &OperationScope,
    ) -> Result<PairingSnapshot, ConnectionsCommandFailure> {
        check(actor, scope).map_err(not_admitted)?;
        if command.is_nil() {
            return Err(not_admitted(PairingError::InvalidInput));
        }
        self.execute_command(
            actor,
            command,
            PairingMutationAction::Cancel {
                operation_id: id,
                expected_revision: expected,
            },
        )
        .await
    }
    pub async fn get_pairing(
        &self,
        actor: &OwnerActor,
        id: Uuid,
        scope: &OperationScope,
    ) -> Result<PairingSnapshot, PairingError> {
        check(actor, scope)?;
        Ok(self.current(actor, id).await?.snapshot())
    }
    pub async fn reconcile_pairing(
        &self,
        actor: &OwnerActor,
        id: Uuid,
        scope: &OperationScope,
    ) -> Result<PairingSnapshot, PairingError> {
        check(actor, scope)?;
        let mut r = self.current(actor, id).await?;
        let _flight = {
            let mut active = self
                .reconciling
                .lock()
                .map_err(|_| PairingError::StorageUnavailable)?;
            if !active.insert(id) {
                return Ok(r.snapshot());
            }
            PairingReconcileGuard {
                active: &self.reconciling,
                id,
            }
        };
        if r.forgotten_command.is_some() || (r.state.terminal() && !r.can_reconcile_repair()) {
            return Ok(r.snapshot());
        }
        if r.state == PairingState::Pending {
            if r.cancellation_command.is_some() {
                r.state = PairingState::RepairRequired;
                r.last_failure = Some(PairingError::Indeterminate);
                return Ok(self.save(r).await?.snapshot());
            }
            let prepared = match self.transport.prepare_start(start_request(&r), scope).await {
                Ok(value) => value,
                Err(error) => return self.record_failure(r, error).await,
            };
            check(actor, scope)?;
            if r.start_phase == PairingStartPhase::Staged {
                r.start_phase = PairingStartPhase::Dispatched;
                r = self.save(r).await?;
            }
            check(actor, scope)?;
            let started = match self.transport.start(prepared, scope).await {
                Ok(v) => v,
                Err(e) => return self.record_failure(r, e).await,
            };
            r = self.repository.accept_started(r.revision, started).await?;
            if r.forgotten_command.is_some() {
                return Ok(r.snapshot());
            }
        }
        // Start is the explicit user intent. Once its exact signed challenge
        // is accepted, key-possession proof is a machine step, not another
        // human permission. Journal that step before signing or sending it.
        if r.state == PairingState::AwaitingLocalConfirmation
            && r.confirmation_command.is_none()
            && r.cancellation_command.is_none()
            && r.forgotten_command.is_none()
        {
            check(actor, scope)?;
            let domain = format!("floe.pairing.proof.v1:{}", r.revision);
            let command = PairingMutation {
                command_id: super::source_operation::derived_id(
                    domain.as_bytes(),
                    actor.person_id,
                    r.operation_id,
                ),
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                action: PairingMutationAction::Confirm {
                    operation_id: r.operation_id,
                    expected_revision: r.revision,
                },
            };
            let receipt = self
                .repository
                .execute_command(command.clone())
                .await
                .map_err(|error| match error {
                    // The step may have committed. Keep the owner driver alive
                    // so exact durable readback can resolve that uncertainty.
                    PairingError::StorageUnavailable => PairingError::Indeterminate,
                    other => other,
                })?;
            if receipt.command != command {
                return Err(PairingError::Conflict);
            }
            match receipt.outcome {
                Ok(_) => {}
                // A concurrent cancellation or observation won. Reload on the
                // next round; never borrow the losing step's authorization.
                Err(PairingError::Conflict) => {
                    return Ok(self.current(actor, id).await?.snapshot());
                }
                Err(error) => return Err(error),
            }
            r = self.current(actor, id).await?;
            if r.forgotten_command.is_some() || (r.state.terminal() && !r.can_reconcile_repair()) {
                return Ok(r.snapshot());
            }
        }
        let handle = r.handle.clone().ok_or(PairingError::RepairRequired)?;
        let observed = if r.cancellation_command.is_some() {
            self.transport.cancel(&handle, scope).await
        } else {
            self.transport.observe(&handle, scope).await
        };
        let mut observed = match observed {
            Ok(v) => v,
            Err(e) => return self.record_failure(r, e).await,
        };
        if r.handle.as_ref() != Some(&observed.handle)
            || r.reviewed.as_ref() != Some(&observed.reviewed)
        {
            return Err(PairingError::ChangedProducer);
        }
        if r.confirmation_command.is_some()
            && r.cancellation_command.is_none()
            && matches!(observed.outcome, PairingOutcome::AwaitingLocalConfirmation)
        {
            observed = match self
                .transport
                .confirm(
                    PairingConfirmation {
                        handle,
                        reviewed: r.reviewed.clone().ok_or(PairingError::Conflict)?,
                    },
                    scope,
                )
                .await
            {
                Ok(v) => v,
                Err(e) => return self.record_failure(r, e).await,
            };
        }
        if r.handle.as_ref() != Some(&observed.handle)
            || r.reviewed.as_ref() != Some(&observed.reviewed)
        {
            return Err(PairingError::ChangedProducer);
        }
        match observed.outcome {
            PairingOutcome::Approved(credential) => {
                let result = self
                    .repository
                    .activate(
                        r.revision,
                        PairingApproval {
                            handle: observed.handle,
                            reviewed: observed.reviewed,
                            person_id: actor.person_id,
                            device_id: actor.device_id.clone(),
                            credential,
                        },
                    )
                    .await;
                match result {
                    Ok(
                        PairingActivationResult::Activated(v)
                        | PairingActivationResult::HistoricalRevocationEvidence(v),
                    ) => Ok(v.snapshot()),
                    Err(e) => {
                        let actual = self.current(actor, id).await?;
                        self.record_failure(actual, e).await
                    }
                }
            }
            outcome => {
                let next_state = match outcome {
                    PairingOutcome::AwaitingLocalConfirmation => r.state,
                    PairingOutcome::AwaitingApproval if r.cancellation_command.is_some() => {
                        PairingState::Cancelling
                    }
                    PairingOutcome::AwaitingApproval => PairingState::AwaitingApproval,
                    PairingOutcome::Rejected => PairingState::Rejected,
                    PairingOutcome::Expired => PairingState::Expired,
                    PairingOutcome::Cancelled => PairingState::Cancelled,
                    PairingOutcome::RepairRequired => PairingState::RepairRequired,
                    PairingOutcome::Approved(_) => unreachable!(),
                };
                if r.state == next_state && r.last_failure.is_none() {
                    return Ok(r.snapshot());
                }
                r.state = next_state;
                r.last_failure = None;
                Ok(self.save(r).await?.snapshot())
            }
        }
    }
    async fn current(&self, actor: &OwnerActor, id: Uuid) -> Result<PairingRecord, PairingError> {
        let r = self
            .repository
            .load(id)
            .await?
            .ok_or(PairingError::InvalidInput)?;
        authorize(actor, &r)?;
        Ok(r)
    }
    async fn save(&self, mut r: PairingRecord) -> Result<PairingRecord, PairingError> {
        let revision = r.revision;
        r.revision = r.revision.checked_add(1).ok_or(PairingError::Conflict)?;
        r.validate()?;
        self.repository.compare_and_swap(revision, r).await
    }
    async fn record_failure(
        &self,
        mut r: PairingRecord,
        error: PairingError,
    ) -> Result<PairingSnapshot, PairingError> {
        if r.forgotten_command.is_some() || (r.state.terminal() && !r.can_reconcile_repair()) {
            return Ok(r.snapshot());
        }
        let previous_state = r.state;
        if matches!(
            error,
            PairingError::RepairRequired
                | PairingError::ForeignIdentity
                | PairingError::ChangedProducer
                | PairingError::Rejected
        ) {
            r.state = PairingState::RepairRequired;
        }
        if r.last_failure != Some(error) || r.state != previous_state {
            r.last_failure = Some(error);
            r = self.save(r).await?;
        }
        if r.state == PairingState::RepairRequired {
            Ok(r.snapshot())
        } else {
            Err(error)
        }
    }
}
fn start_request(r: &PairingRecord) -> PairingStartRequest {
    PairingStartRequest {
        operation_id: r.operation_id,
        setup: r.setup.clone(),
        person_id: r.person_id,
        device_id: r.device_id.clone(),
        generation: r.generation,
    }
}
fn authorize(actor: &OwnerActor, r: &PairingRecord) -> Result<(), PairingError> {
    if actor.person_id != r.person_id || actor.device_id != r.device_id {
        Err(PairingError::ForeignIdentity)
    } else {
        Ok(())
    }
}
fn check(actor: &OwnerActor, scope: &OperationScope) -> Result<(), PairingError> {
    actor
        .validate()
        .map_err(|_| PairingError::ForeignIdentity)?;
    if scope.cancellation().is_cancelled() {
        return Err(PairingError::Cancelled);
    }
    if scope.deadline() <= tokio::time::Instant::now() {
        return Err(PairingError::DeadlineExceeded);
    }
    Ok(())
}
fn not_admitted(e: PairingError) -> ConnectionsCommandFailure {
    ConnectionsCommandFailure::NotAdmitted(super::product::pairing_error(e))
}
fn indeterminate(e: PairingError) -> ConnectionsCommandFailure {
    ConnectionsCommandFailure::Indeterminate(super::product::pairing_error(e))
}
fn pairing_failure_projection(record: &PairingRecord) -> Option<crate::ConnectionFailure> {
    let (reason, recovery) = match record.state {
        PairingState::Rejected => (
            crate::ConnectionFailureReason::Rejected,
            crate::ConnectionRecovery::NewReview,
        ),
        PairingState::Expired => (
            crate::ConnectionFailureReason::Expired,
            crate::ConnectionRecovery::NewReview,
        ),
        PairingState::RepairRequired | PairingState::RevocationPending => (
            crate::ConnectionFailureReason::OperationUncertain,
            if record.can_reconcile_repair() {
                crate::ConnectionRecovery::Reconcile
            } else {
                crate::ConnectionRecovery::None
            },
        ),
        _ if record.last_failure == Some(PairingError::Indeterminate) => (
            crate::ConnectionFailureReason::OperationUncertain,
            crate::ConnectionRecovery::Reobserve,
        ),
        _ if record.last_failure.is_some() => (
            match record.last_failure {
                Some(PairingError::ForeignIdentity | PairingError::ChangedProducer) => {
                    crate::ConnectionFailureReason::IdentityChanged
                }
                Some(PairingError::Rejected) => crate::ConnectionFailureReason::Rejected,
                Some(PairingError::Expired) => crate::ConnectionFailureReason::Expired,
                Some(PairingError::Conflict) => crate::ConnectionFailureReason::Conflict,
                _ => crate::ConnectionFailureReason::StorageUnavailable,
            },
            crate::ConnectionRecovery::Reobserve,
        ),
        _ => return None,
    };
    let actions = record.allowed_actions();
    let recovery = if actions.is_empty() {
        crate::ConnectionRecovery::None
    } else {
        recovery
    };
    Some(crate::ConnectionFailure {
        domain: crate::ConnectionFailureDomain::Connections,
        category: floe_kernel::AgentFailureCategory::Integrity,
        reason,
        incident_id: record.operation_id,
        correlation_id: record.operation_id,
        reload_required: true,
        seal_session: false,
        recovery,
        safe_actions: actions,
    })
}

// Serializes observation/dispatch for one operation, not owner decisions or DB transactions.
struct PairingReconcileGuard<'a> {
    active: &'a std::sync::Mutex<std::collections::HashSet<Uuid>>,
    id: Uuid,
}
impl Drop for PairingReconcileGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&self.id);
        }
    }
}
