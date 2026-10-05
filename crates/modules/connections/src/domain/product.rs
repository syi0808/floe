use crate::{
    ConnectionAction, ConnectionResource, GatewaySummary, IntegrationDescriptor,
    IntegrationOperationRef, SourceConnection, ValidatedManagementLaunch,
};
use chrono::{DateTime, Utc};
use floe_access::{ReviewRef, SourceExpectation, VerifiedGatewayBinding};
use floe_context_contract::{ConnectionId, DataClass, GrantDataCategory, ProcessingRestriction};
use floe_kernel::{AgentFailure, PersonId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Admission evidence for one command delivery. A caller must retain any prior
/// uncertainty until an exact receipt or identity-bound terminal observation.
/// The failure reason alone never establishes whether a command was admitted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectionsCommandFailure {
    NotAdmitted(AgentFailure),
    /// Durable exact-command rejection, including all earlier deliveries.
    NotApplied(AgentFailure),
    Admitted(AgentFailure),
    Indeterminate(AgentFailure),
}
impl ConnectionsCommandFailure {
    /// Internal owners with their own durable command state may project only
    /// the reason. Product transports must preserve the admission variant.
    pub fn into_failure(self) -> AgentFailure {
        match self {
            Self::NotApplied(failure)
            | Self::NotAdmitted(failure)
            | Self::Admitted(failure)
            | Self::Indeterminate(failure) => failure,
        }
    }
}
impl From<AgentFailure> for ConnectionsCommandFailure {
    fn from(failure: AgentFailure) -> Self {
        Self::Indeterminate(failure)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationState {
    Available,
    Unavailable,
    Connected,
    Connecting,
    Error,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationCapability {
    PrepareReview,
    Configure,
    Disconnect,
}
/// Stable presentation identity, never inferred from provider display labels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationServiceKind {
    AppleCalendar,
    AppleContacts,
    AppleHealth,
    AppleAttention,
    Hosted,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationSummary {
    pub service_kind: IntegrationServiceKind,
    pub integration_ref: Uuid,
    pub revision: u64,
    pub display_name: String,
    pub category: String,
    pub state: IntegrationState,
    pub capabilities: Vec<IntegrationCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceSummary>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceAvailability {
    Available,
    Unavailable,
    PermissionRequired,
    IdentityChanged,
    Disconnected,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObserveState {
    Disabled,
    Enabled,
    Paused,
    ReviewRequired,
}
/// Read-only account/group presentation; never a source or resource authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceGroupSummary {
    pub group_ref: Uuid,
    pub label: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSummary {
    pub resource_ref: Uuid,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<ResourceGroupSummary>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSummary {
    pub source_ref: Uuid,
    pub revision: u64,
    pub display_labels: Vec<String>,
    pub availability: SourceAvailability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_observed_at: Option<DateTime<Utc>>,
    pub selected_resources: Vec<ResourceSummary>,
    pub observe_state: ObserveState,
    pub allowed_actions: Vec<ConnectionAction>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionsOverview {
    pub gateways: Vec<GatewaySummary>,
    pub integrations: Vec<IntegrationSummary>,
    pub sources: Vec<SourceSummary>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingChoice {
    DeviceOnly,
    GatewayAllowed,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewProcessingDisclosure {
    pub view_id: String,
    pub data_class: DataClass,
    pub data_categories: Vec<GrantDataCategory>,
    pub current: Option<ProcessingRestriction>,
    pub requested: Option<ProcessingRestriction>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessingDisclosure {
    pub views: Vec<ViewProcessingDisclosure>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermittedResource {
    pub resource_ref: Uuid,
    pub label: String,
    pub selected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<ResourceGroupSummary>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceReview {
    pub review_ref: ReviewRef,
    pub source_ref: Uuid,
    pub source_revision: u64,
    pub labels: Vec<String>,
    pub permitted_choices: Vec<PermittedResource>,
    pub processing_disclosure: ProcessingDisclosure,
    pub expires_at: DateTime<Utc>,
    pub allowed_actions: Vec<ConnectionAction>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObserveReview {
    pub review_ref: ReviewRef,
    pub source_ref: Uuid,
    pub source_revision: u64,
    pub display_members: Vec<String>,
    pub processing_disclosure: ProcessingDisclosure,
    pub expires_at: DateTime<Utc>,
    pub allowed_actions: Vec<ConnectionAction>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntegrationTarget {
    Device {
        device_id: String,
    },
    Gateway {
        gateway_ref: Uuid,
        gateway_revision: u64,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntegrationBinding {
    Device {
        device_id: String,
    },
    Gateway {
        gateway_ref: Uuid,
        binding: VerifiedGatewayBinding,
    },
}
impl IntegrationBinding {
    pub fn public_target(&self) -> IntegrationTarget {
        match self {
            Self::Device { device_id } => IntegrationTarget::Device {
                device_id: device_id.clone(),
            },
            Self::Gateway {
                gateway_ref,
                binding,
            } => IntegrationTarget::Gateway {
                gateway_ref: *gateway_ref,
                gateway_revision: binding.credential_generation,
            },
        }
    }
    pub fn gateway(&self) -> Result<(Uuid, &VerifiedGatewayBinding), AgentFailure> {
        match self {
            Self::Gateway {
                gateway_ref,
                binding,
            } => Ok((*gateway_ref, binding)),
            _ => Err(AgentFailure::PolicyDenied),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationReview {
    pub review_ref: ReviewRef,
    pub integration_ref: Uuid,
    pub catalog_revision: u64,
    pub target: IntegrationTarget,
    pub display_name: String,
    pub setup_kind: crate::IntegrationSetupKind,
    pub expires_at: DateTime<Utc>,
    pub allowed_actions: Vec<ConnectionAction>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionOperationState {
    Pending,
    Running,
    AwaitingUser,
    Completed,
    Failed,
    Cancelled,
    RepairRequired,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionRecovery {
    None,
    Reobserve,
    Reconcile,
    Unlock,
    Reopen,
    NewReview,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionFailureDomain {
    Connections,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionFailure {
    pub domain: ConnectionFailureDomain,
    pub category: floe_kernel::AgentFailureCategory,
    pub reason: ConnectionFailureReason,
    pub incident_id: Uuid,
    pub correlation_id: Uuid,
    pub reload_required: bool,
    pub seal_session: bool,
    pub recovery: ConnectionRecovery,
    pub safe_actions: Vec<ConnectionAction>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionFailureReason {
    IdentityChanged,
    OperationUncertain,
    StorageUnavailable,
    Rejected,
    Expired,
    Conflict,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionOperationSnapshot {
    pub operation_ref: Uuid,
    pub revision: u64,
    pub state: ConnectionOperationState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_action: Option<ValidatedManagementLaunch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceSummary>,
    pub allowed_actions: Vec<ConnectionAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<ConnectionFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_observation_after_ms: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationRecord {
    pub integration_ref: Uuid,
    pub target: IntegrationBinding,
    pub revision: u64,
    pub descriptor: IntegrationDescriptor,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceReviewDescriptor {
    pub summary: SourceReview,
    pub source: SourceConnection,
    pub expected: Option<SourceExpectation>,
    pub resources: Vec<(Uuid, ConnectionResource)>,
    pub catalog_digest: [u8; 32],
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationReviewDescriptor {
    pub summary: IntegrationReview,
    pub integration: IntegrationRecord,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationOperationRecord {
    pub snapshot: ConnectionOperationSnapshot,
    pub remote: IntegrationOperationRef,
    pub reviewed: IntegrationReviewDescriptor,
    pub connection_id: Option<ConnectionId>,
    pub cancellation_command: Option<Uuid>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ConnectionsPayload {
    Integration(IntegrationRecord),
    SourceReview(SourceReviewDescriptor),
    IntegrationReview(IntegrationReviewDescriptor),
    IntegrationOperation(IntegrationOperationRecord),
    NativeSetup {
        snapshot: ConnectionOperationSnapshot,
        reviewed: IntegrationReviewDescriptor,
        source: SourceConnection,
        dispatched: bool,
    },
    CancellationIntent {
        operation_ref: Uuid,
        expected_revision: u64,
    },
    CancellationReceipt(ConnectionOperationSnapshot),
    ObserveReview {
        reference: ReviewRef,
        source_ref: Uuid,
        source_revision: u64,
        choice: ProcessingChoice,
    },
    SourceConfiguration {
        descriptor: SourceReviewDescriptor,
        selected_resources: Vec<Uuid>,
        successor: SourceConnection,
        operation_id: Uuid,
    },
    SourceMutation {
        source: SourceConnection,
        summary: SourceSummary,
    },
    Launch(ValidatedManagementLaunch),
    GatewayForgotten(GatewaySummary),
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionsRecord {
    pub record_ref: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub command_id: Uuid,
    pub intent_digest: [u8; 32],
    pub revision: u64,
    pub payload: ConnectionsPayload,
}
impl ConnectionsRecord {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.record_ref.is_nil()
            || self.command_id.is_nil()
            || !self.person_id.is_valid()
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.revision == 0
            || self.revision > i64::MAX as u64
            || self.intent_digest == [0; 32]
        {
            return Err(AgentFailure::InvalidInput);
        }
        match &self.payload {
            ConnectionsPayload::Integration(value) => {
                if value.integration_ref != self.record_ref
                    || value.revision != self.revision
                    || value.descriptor.display_name.is_empty()
                    || value.descriptor.display_name.len() > 256
                {
                    return Err(AgentFailure::InvalidInput);
                }
                validate_target(&value.target, value.descriptor.setup_kind, &self.device_id)?;
            }
            ConnectionsPayload::IntegrationReview(value) => {
                if value.summary.review_ref.id != self.record_ref
                    || value.summary.review_ref.revision != 1
                    || value.summary.target != value.integration.target.public_target()
                    || value.summary.setup_kind != value.integration.descriptor.setup_kind
                    || value.summary.catalog_revision
                        != value.integration.descriptor.catalog_revision
                {
                    return Err(AgentFailure::InvalidInput);
                }
                value.summary.review_ref.validate()?;
            }
            ConnectionsPayload::SourceReview(value) => {
                value
                    .source
                    .validate()
                    .map_err(|_| AgentFailure::InvalidInput)?;
                if value.source.person_id() != self.person_id
                    || value.summary.review_ref.id != self.record_ref
                    || value.summary.source_revision != value.source.revision()
                    || value.resources.len() > 256
                    || value
                        .resources
                        .iter()
                        .map(|(id, _)| id)
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        != value.resources.len()
                {
                    return Err(AgentFailure::InvalidInput);
                }
                value.summary.review_ref.validate()?;
                if let Some(expected) = &value.expected {
                    expected.validate()?;
                } else if value.source.state() != crate::SourceState::Pending {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            ConnectionsPayload::IntegrationOperation(value) => {
                validate_operation(&value.snapshot, self.record_ref, self.revision)?;
                if value.remote.operation_id != self.record_ref
                    || value.remote.connector_id
                        != value.reviewed.integration.descriptor.connector_id
                {
                    return Err(AgentFailure::InvalidInput);
                }
                value.remote.expected.validate()?;
            }
            ConnectionsPayload::NativeSetup {
                snapshot,
                reviewed,
                source,
                dispatched,
            } => {
                validate_operation(snapshot, self.record_ref, self.revision)?;
                if !matches!(&reviewed.integration.target,IntegrationBinding::Device{device_id} if device_id==&self.device_id)
                    || source.person_id() != self.person_id
                    || (!dispatched && snapshot.state != ConnectionOperationState::Pending)
                {
                    return Err(AgentFailure::InvalidInput);
                }
                source.validate().map_err(|_| AgentFailure::InvalidInput)?;
            }
            ConnectionsPayload::SourceConfiguration {
                descriptor,
                successor,
                ..
            } => {
                descriptor
                    .source
                    .validate_successor(successor)
                    .map_err(|_| AgentFailure::InvalidInput)?;
            }
            ConnectionsPayload::SourceMutation { source, summary } => {
                source.validate().map_err(|_| AgentFailure::InvalidInput)?;
                if source.person_id() != self.person_id || summary.revision != source.revision() {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            ConnectionsPayload::CancellationIntent {
                operation_ref,
                expected_revision,
            } => {
                if operation_ref.is_nil() || *expected_revision == 0 {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            ConnectionsPayload::CancellationReceipt(snapshot) => {
                if snapshot.operation_ref.is_nil() || snapshot.revision == 0 {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            ConnectionsPayload::ObserveReview {
                reference,
                source_ref,
                source_revision,
                ..
            } => {
                reference.validate()?;
                if source_ref.is_nil() || *source_revision == 0 {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            ConnectionsPayload::Launch(action) => {
                if action.action_ref != self.record_ref || action.validated_url.len() > 2048 {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            ConnectionsPayload::GatewayForgotten(summary) => {
                if summary.state != crate::GatewayState::Forgotten
                    || summary.gateway_ref.is_nil()
                    || summary.revision == 0
                {
                    return Err(AgentFailure::InvalidInput);
                }
            }
        }
        Ok(())
    }
    /// Pure cancellation transition; storage commits this successor with the
    /// exact cancellation admission in one transaction.
    pub fn with_cancellation(&self, receipt: &Self) -> Result<Self, AgentFailure> {
        self.validate()?;
        receipt.validate()?;
        let ConnectionsPayload::CancellationIntent {
            operation_ref,
            expected_revision,
        } = receipt.payload
        else {
            return Err(AgentFailure::InvalidInput);
        };
        if operation_ref != self.record_ref
            || receipt.person_id != self.person_id
            || receipt.device_id != self.device_id
            || expected_revision > self.revision
        {
            return Err(AgentFailure::Conflict);
        }
        let ConnectionsPayload::IntegrationOperation(ref operation) = self.payload else {
            return Err(AgentFailure::InvalidInput);
        };
        if !operation
            .snapshot
            .allowed_actions
            .contains(&ConnectionAction::Cancel)
            || operation.cancellation_command.is_some()
        {
            return Err(AgentFailure::Conflict);
        }
        let mut next = self.clone();
        next.revision = self.revision.checked_add(1).ok_or(AgentFailure::Conflict)?;
        let ConnectionsPayload::IntegrationOperation(ref mut operation) = next.payload else {
            unreachable!()
        };
        operation.cancellation_command = Some(receipt.command_id);
        operation.snapshot.revision = next.revision;
        operation.snapshot.allowed_actions = vec![ConnectionAction::Reobserve];
        operation.snapshot.launch_action = None;
        operation.snapshot.display_code = None;
        self.validate_successor(&next)?;
        Ok(next)
    }
    pub fn validate_successor(&self, next: &Self) -> Result<(), AgentFailure> {
        self.validate()?;
        next.validate()?;
        if self.record_ref != next.record_ref
            || self.person_id != next.person_id
            || self.device_id != next.device_id
            || self.command_id != next.command_id
            || self.intent_digest != next.intent_digest
            || self.revision.checked_add(1) != Some(next.revision)
        {
            return Err(AgentFailure::Conflict);
        }
        match (&self.payload, &next.payload) {
            (
                ConnectionsPayload::CancellationIntent {
                    operation_ref,
                    expected_revision,
                },
                ConnectionsPayload::CancellationReceipt(snapshot),
            ) if snapshot.operation_ref == *operation_ref
                && snapshot.revision >= *expected_revision => {}

            (
                ConnectionsPayload::NativeSetup {
                    reviewed: old,
                    source: old_source,
                    dispatched: was,
                    snapshot: before,
                },
                ConnectionsPayload::NativeSetup {
                    reviewed: new,
                    source: new_source,
                    dispatched: now,
                    snapshot: after,
                },
            ) if old == new
                && old_source == new_source
                && (!*was || *now)
                && !terminal_operation(before.state)
                && after.revision == next.revision => {}
            (
                ConnectionsPayload::SourceConfiguration { successor, .. },
                ConnectionsPayload::SourceMutation { source, .. },
            ) if successor == source => {}
            (ConnectionsPayload::Integration(old), ConnectionsPayload::Integration(new))
                if old.integration_ref == new.integration_ref
                    && old.target == new.target
                    && old.descriptor.connector_id == new.descriptor.connector_id => {}
            (
                ConnectionsPayload::IntegrationOperation(old),
                ConnectionsPayload::IntegrationOperation(new),
            ) if old.remote.operation_id == new.remote.operation_id
                && old.remote.expected == new.remote.expected
                && old.reviewed == new.reviewed
                && !terminal_operation(old.snapshot.state)
                && new.remote.remote_revision >= old.remote.remote_revision
                && old
                    .cancellation_command
                    .is_none_or(|id| new.cancellation_command == Some(id)) => {}
            _ => return Err(AgentFailure::Conflict),
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObserveDecision {
    Allow,
}

fn validate_target(
    target: &IntegrationBinding,
    kind: crate::IntegrationSetupKind,
    device: &str,
) -> Result<(), AgentFailure> {
    match (target, kind) {
        (
            IntegrationBinding::Device { device_id },
            crate::IntegrationSetupKind::NativePermission,
        ) if device_id == device => Ok(()),
        (
            IntegrationBinding::Gateway {
                gateway_ref,
                binding,
            },
            kind,
        ) if kind != crate::IntegrationSetupKind::NativePermission && !gateway_ref.is_nil() => {
            binding.validate()
        }
        _ => Err(AgentFailure::InvalidInput),
    }
}
fn terminal_operation(state: ConnectionOperationState) -> bool {
    matches!(
        state,
        ConnectionOperationState::Completed
            | ConnectionOperationState::Failed
            | ConnectionOperationState::Cancelled
            | ConnectionOperationState::RepairRequired
    )
}
fn validate_operation(
    snapshot: &ConnectionOperationSnapshot,
    id: Uuid,
    revision: u64,
) -> Result<(), AgentFailure> {
    if snapshot.operation_ref != id
        || snapshot.revision != revision
        || (snapshot.state == ConnectionOperationState::Completed && snapshot.source.is_none())
        || (matches!(
            snapshot.state,
            ConnectionOperationState::Failed | ConnectionOperationState::RepairRequired
        ) && snapshot.failure.is_none())
        || snapshot
            .next_observation_after_ms
            .is_some_and(|value| value > 60000)
    {
        Err(AgentFailure::InvalidInput)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionsCommandJournal {
    Product,
    SourceOperation,
}

/// Immutable command identity within its owning storage admission journal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionsCommandIdentity {
    pub journal: ConnectionsCommandJournal,
    pub record_ref: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub command_id: Uuid,
    pub intent_digest: [u8; 32],
}
impl ConnectionsCommandIdentity {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.record_ref.is_nil()
            || !self.person_id.is_valid()
            || self.command_id.is_nil()
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.intent_digest == [0; 32]
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
    pub fn matches(&self, record: &ConnectionsRecord) -> bool {
        self.record_ref == record.record_ref
            && self.person_id == record.person_id
            && self.device_id == record.device_id
            && self.command_id == record.command_id
            && self.intent_digest == record.intent_digest
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionsCommandRejection {
    pub identity: ConnectionsCommandIdentity,
    pub reason: AgentFailure,
}
pub enum ConnectionsCommandResolution {
    Admitted,
    NotApplied(AgentFailure),
}
