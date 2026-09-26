//! Conversation-owned durable user interactions.
//!
//! An interaction records that a running turn reached a recoverable owner
//! requirement: the admitted origin (a Tool call, Delegation Task or Model
//! attempt), the owner-produced semantic request, and the immutable reviewed
//! target the person's decision binds. Conversation owns interaction identity,
//! origin, lifecycle, decision intent and resume linkage; it never owns source,
//! grant or recipient authority. Those stay with Access, Connections and the
//! provider/native owners, which re-verify current authority when a decision
//! resolves (05-C/05-D).
//!
//! The reviewed target carries opaque bounded identifiers only: exact
//! connection/device/source, requesting consumer/purpose, reviewed owner
//! identity (connection revision, pinned producer, live native subject), and
//! the whole affected bundle with per-member source revision, grant
//! expectation (including expected absence) and policy authority. It carries
//! no credentials, tokens, source payloads, prompts, provider errors or
//! display labels. Model-safe artifacts carry only the opaque
//! [`UserInteractionRef`].

use floe_agent_contract::{
    AgentFailure, DataClass, PackageKind, PackageRef, ProcessingSourceScope, RecipientLineage,
    UserInteractionKind,
};
use floe_kernel::{CommandId, PersonId, RunId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Actionable (Pending or Resolving) interactions admitted per origin Run.
pub const MAX_ACTIVE_INTERACTIONS_PER_RUN: usize = 8;
/// Stored interaction rows per origin Run, terminal rows included.
pub const MAX_STORED_INTERACTIONS_PER_RUN: usize = 64;
/// Pending-review lifetime: a decision must arrive within 24 hours.
pub const INTERACTION_PENDING_LIFETIME_MS: i64 = 24 * 60 * 60 * 1000;
pub const MAX_REVIEWED_SOURCE_BYTES: usize = 128;
pub const MAX_REVIEWED_IDENTIFIER_BYTES: usize = 256;
pub const MAX_REVIEWED_PURPOSE_BYTES: usize = 64;
pub const MAX_TARGET_BUNDLE_MEMBERS: usize = 8;
pub const MAX_REVIEWED_TARGET_BYTES: usize = 8 * 1024;
pub const MAX_RECIPIENT_CONSENT_TARGET_BYTES: usize = 128 * 1024;

/// Fixed namespace for deterministic interaction publication identity.
pub const INTERACTION_ID_NAMESPACE: Uuid =
    Uuid::from_u128(0x9f2c_4b1e_8d3a_4f6b_9c1d_2e3f_4a5b_6c7d);
/// Fixed namespace for the stable owner-operation identity of a decision.
pub const INTERACTION_OPERATION_NAMESPACE: Uuid =
    Uuid::from_u128(0x3b7e_1a90_c4d2_4e5f_8a6b_0c1d_2e3f_4a5b);

/// The admitted invocation that produced the requirement.
///
/// Identity is verified against the origin Run's durable journal: a Tool
/// origin needs its ToolIntent, a Task origin its DelegationIntent, a Model
/// origin its ModelIntent. An origin that the journal never admitted is
/// rejected, however well-formed its ids look.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "origin", rename_all = "snake_case", deny_unknown_fields)]
pub enum InteractionOrigin {
    Tool {
        call_id: Uuid,
    },
    Task {
        task_id: Uuid,
        capability_call_id: Option<Uuid>,
    },
    Model {
        attempt_id: Uuid,
    },
}

impl InteractionOrigin {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        let valid = match self {
            Self::Tool { call_id } => !call_id.is_nil(),
            Self::Task {
                task_id,
                capability_call_id,
            } => !task_id.is_nil() && capability_call_id.is_none_or(|call_id| !call_id.is_nil()),
            Self::Model { attempt_id } => !attempt_id.is_nil(),
        };
        valid.then_some(()).ok_or(AgentFailure::StorageUnavailable)
    }
}

/// The owner-produced semantic request, converted at the App boundary.
///
/// Variants mirror the source-owner requirement reasons; Conversation stores
/// the converted record and binds decisions to the reviewed target, but never
/// interprets source authority itself.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionRequirementKind {
    EnableObserve,
    ReviewChangedSource,
    RequestSystemPermission,
    Reconnect,
    ApproveProcessingRecipient,
    SelectResource,
    ConfigureExpertBinding,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionRequirement {
    pub kind: InteractionRequirementKind,
    pub source_id: String,
    pub connection_id: Option<String>,
    pub consumer: String,
    pub purpose: String,
    pub inline: bool,
}

impl InteractionRequirement {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if validate_identifier(&self.source_id, MAX_REVIEWED_SOURCE_BYTES).is_err()
            || self.connection_id.as_ref().is_some_and(|value| {
                validate_identifier(value, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            })
            || validate_identifier(&self.consumer, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            || validate_identifier(&self.purpose, MAX_REVIEWED_PURPOSE_BYTES).is_err()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

/// One observed authority revision: source incarnation/epoch or the
/// consumer-policy authority where applicable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityRevision {
    pub incarnation: Uuid,
    pub epoch: u64,
}

impl AuthorityRevision {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.incarnation.is_nil() || self.epoch == 0 {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

/// The grant state the person reviewed, including expected absence.
///
/// A decision binds this expectation; resolution re-reads current authority
/// and never treats a changed grant as the reviewed one.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpectedGrantState {
    Absent,
    Active {
        grant_id: Uuid,
        authority_incarnation: Uuid,
        authority_epoch: u64,
    },
}

impl ExpectedGrantState {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        match self {
            Self::Absent => Ok(()),
            Self::Active {
                grant_id,
                authority_incarnation,
                authority_epoch,
            } => {
                if grant_id.is_nil() || authority_incarnation.is_nil() || *authority_epoch == 0 {
                    return Err(AgentFailure::StorageUnavailable);
                }
                Ok(())
            }
        }
    }
}

/// One reviewed grant of an inline Observe bundle: the exact member, its
/// resource, and the per-member source revision, grant expectation (including
/// expected absence) and policy authority the decision binds. Members are the
/// whole affected bundle, not just the view the blocked read named: a live
/// member outside this set, or a changed per-member expectation, invalidates
/// the review instead of widening it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedBundleMember {
    pub member_id: String,
    pub policy_fingerprint: String,
    pub resource: String,
    pub source_revision: Option<AuthorityRevision>,
    pub expected_grant: ExpectedGrantState,
    pub policy_authority: Option<AuthorityRevision>,
}

impl ReviewedBundleMember {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if validate_identifier(&self.member_id, MAX_REVIEWED_SOURCE_BYTES).is_err()
            || validate_identifier(&self.resource, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            || self.policy_fingerprint.len() != 64
            || !self
                .policy_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        if let Some(revision) = &self.source_revision {
            revision.validate()?;
        }
        self.expected_grant.validate()?;
        if let Some(authority) = &self.policy_authority {
            authority.validate()?;
        }
        Ok(())
    }
}

/// An inline-mutation target: connection-level Observe approval over the
/// reviewed bundle. The bundle-level fields bind the owner identity the
/// person reviewed (local connection revision, pinned remote producer,
/// live native subject); the members bind every affected grant. The card
/// discloses the whole bundle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InlineObserveTarget {
    pub connection_id: String,
    pub device_id: Option<String>,
    pub source_id: String,
    pub connector_id: Option<String>,
    pub consumer: String,
    pub purpose: String,
    pub connection_revision: Option<u64>,
    pub reviewed_producer_fingerprint: Option<String>,
    pub reviewed_native_subject: Option<String>,
    pub members: Vec<ReviewedBundleMember>,
}

impl InlineObserveTarget {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if validate_identifier(&self.connection_id, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            || self.device_id.as_ref().is_some_and(|value| {
                validate_identifier(value, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            })
            || validate_identifier(&self.source_id, MAX_REVIEWED_SOURCE_BYTES).is_err()
            || self
                .connector_id
                .as_ref()
                .is_some_and(|value| validate_identifier(value, MAX_REVIEWED_SOURCE_BYTES).is_err())
            || validate_identifier(&self.consumer, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            || validate_identifier(&self.purpose, MAX_REVIEWED_PURPOSE_BYTES).is_err()
            || self
                .connection_revision
                .is_some_and(|revision| revision == 0)
            || self
                .reviewed_producer_fingerprint
                .as_ref()
                .is_some_and(|value| {
                    validate_identifier(value, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
                })
            || self.reviewed_native_subject.as_ref().is_some_and(|value| {
                validate_identifier(value, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            })
            || self.members.is_empty()
            || self.members.len() > MAX_TARGET_BUNDLE_MEMBERS
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let mut previous: Option<(&str, &str)> = None;
        for member in &self.members {
            member.validate()?;
            let key = (member.member_id.as_str(), member.resource.as_str());
            if previous.is_some_and(|previous| previous >= key) {
                return Err(AgentFailure::StorageUnavailable);
            }
            previous = Some(key);
        }
        if serde_json::to_vec(self)
            .map(|encoded| encoded.len() > MAX_REVIEWED_TARGET_BYTES)
            .unwrap_or(true)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationDestination {
    ConnectionSettings,
    SystemPermission,
    ResourcePicker,
}

/// A navigation/review-only target: no inline-mutation fields exist on this
/// variant, so resolution code cannot accidentally read grant or resource
/// mutation state off a card that never offered inline enable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NavigationOnlyTarget {
    pub destination: NavigationDestination,
    pub source_id: String,
    pub connection_id: Option<String>,
    pub consumer: String,
    pub purpose: String,
}

impl NavigationOnlyTarget {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if validate_identifier(&self.source_id, MAX_REVIEWED_SOURCE_BYTES).is_err()
            || self.connection_id.as_ref().is_some_and(|value| {
                validate_identifier(value, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            })
            || validate_identifier(&self.consumer, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            || validate_identifier(&self.purpose, MAX_REVIEWED_PURPOSE_BYTES).is_err()
            || serde_json::to_vec(self)
                .map(|encoded| encoded.len() > MAX_REVIEWED_TARGET_BYTES)
                .unwrap_or(true)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

/// An exact-recipient consent target: the reviewed dispatch the person's
/// decision binds.
///
/// Stores the canonical requirement values verbatim: exact recipient,
/// route/profile identity, purpose/consumer, data classes, source scopes,
/// lineage, and device, plus the audit-only original projection identity.
/// The decision grants an Access consent for exactly this review; a
/// different recipient, profile, scope, lineage, or device requires a new
/// review. The paired-connection (client) binding is ambient: resolution
/// binds the live pairing, and dispatch re-checks it, so a re-pairing never
/// reuses an old review.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipientConsentTarget {
    pub recipient: String,
    pub profile_id: String,
    pub purpose: String,
    pub consumer: String,
    pub input_data_classes: Vec<DataClass>,
    pub source_scopes: Vec<ProcessingSourceScope>,
    pub lineage: RecipientLineage,
    pub device_id: String,
    pub projection_ref: Uuid,
    pub projection_revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertBindingTarget {
    pub registry_instance_id: Uuid,
    pub assignment_id: Uuid,
    pub package: PackageRef,
    pub definition_revision: u64,
    pub requirement_key: String,
    pub capability: String,
    pub contract_version: u32,
    pub minimum_sources: u8,
    pub maximum_sources: u8,
    pub expected_binding_revision: u64,
    pub admitted_selection_digest: [u8; 32],
}

impl ExpertBindingTarget {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.registry_instance_id.is_nil()
            || self.assignment_id.is_nil()
            || self.package.kind != PackageKind::Expert
            || validate_identifier(&self.package.id, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            || validate_identifier(&self.package.version, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            || self.definition_revision == 0
            || validate_identifier(&self.requirement_key, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            || validate_identifier(&self.capability, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            || self.contract_version == 0
            || self.minimum_sources > self.maximum_sources
            || self.maximum_sources > 16
            || self.expected_binding_revision == 0
            || self.admitted_selection_digest == [0; 32]
            || serde_json::to_vec(self)
                .map(|encoded| encoded.len() > MAX_REVIEWED_TARGET_BYTES)
                .unwrap_or(true)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

impl RecipientConsentTarget {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        floe_agent_contract::ProcessingRequirement::try_new(
            self.recipient.clone(),
            self.profile_id.clone(),
            self.purpose.clone(),
            self.consumer.clone(),
            self.input_data_classes.clone(),
            self.source_scopes.clone(),
            self.projection_ref,
            self.projection_revision,
            self.lineage,
        )
        .map_err(|_| AgentFailure::StorageUnavailable)?;
        // The stored order is canonical: the digest serializes members in
        // place, so unsorted storage would fork card identity.
        let mut sorted_classes = self.input_data_classes.clone();
        sorted_classes.sort();
        let mut sorted_scopes = self.source_scopes.clone();
        sorted_scopes.sort_by_cached_key(|scope| serde_json::to_vec(scope).unwrap_or_default());
        if sorted_classes != self.input_data_classes
            || sorted_scopes != self.source_scopes
            || validate_identifier(&self.device_id, MAX_REVIEWED_IDENTIFIER_BYTES).is_err()
            || serde_json::to_vec(self)
                .map(|encoded| encoded.len() > MAX_RECIPIENT_CONSENT_TARGET_BYTES)
                .unwrap_or(true)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

/// The immutable reviewed descriptor a decision binds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ReviewedTarget {
    InlineObserve(InlineObserveTarget),
    NavigationOnly(NavigationOnlyTarget),
    RecipientConsent(RecipientConsentTarget),
    ExpertBinding(ExpertBindingTarget),
}

impl ReviewedTarget {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        match self {
            Self::InlineObserve(target) => target.validate(),
            Self::NavigationOnly(target) => target.validate(),
            Self::RecipientConsent(target) => target.validate(),
            Self::ExpertBinding(target) => target.validate(),
        }
    }
}

/// The semantic receipt of a completed resolution: which decision and which
/// stable owner operation produced it. Grant/authority facts stay with their
/// owners; this receipt is coordination evidence only.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionResolutionReceipt {
    pub decision_id: Uuid,
    pub owner_operation_id: Uuid,
    pub resolved_at_unix_ms: i64,
}

impl InteractionResolutionReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.decision_id.is_nil()
            || self.owner_operation_id.is_nil()
            || self.resolved_at_unix_ms < 0
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

/// Canonical lifecycle: Pending -> Resolving -> Resolved, with Pending also
/// reaching Denied/Cancelled/Superseded/Expired. Resolving is durable
/// owner-operation recovery, never a Run waiting on the person.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum InteractionState {
    Pending,
    Resolving {
        decision_id: Uuid,
        owner_operation_id: Uuid,
    },
    Resolved {
        receipt: InteractionResolutionReceipt,
    },
    Denied {
        decision_id: Uuid,
    },
    Cancelled {
        decision_id: Uuid,
    },
    Superseded {
        superseded_by: Option<Uuid>,
    },
    Expired,
}

impl InteractionState {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        match self {
            Self::Pending | Self::Expired => Ok(()),
            Self::Resolving {
                decision_id,
                owner_operation_id,
            } => {
                if decision_id.is_nil() || owner_operation_id.is_nil() {
                    return Err(AgentFailure::StorageUnavailable);
                }
                Ok(())
            }
            Self::Resolved { receipt } => receipt.validate(),
            Self::Denied { decision_id } | Self::Cancelled { decision_id } => {
                if decision_id.is_nil() {
                    return Err(AgentFailure::StorageUnavailable);
                }
                Ok(())
            }
            Self::Superseded { superseded_by } => {
                if superseded_by.is_some_and(|id| id.is_nil()) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                Ok(())
            }
        }
    }

    pub fn is_terminal(&self) -> bool {
        match self {
            Self::Pending | Self::Resolving { .. } => false,
            Self::Resolved { .. }
            | Self::Denied { .. }
            | Self::Cancelled { .. }
            | Self::Superseded { .. }
            | Self::Expired => true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationInteraction {
    pub id: Uuid,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub origin_run_id: RunId,
    pub origin_turn_id: Uuid,
    pub origin: InteractionOrigin,
    pub kind: UserInteractionKind,
    pub requirement: InteractionRequirement,
    pub requirement_digest: [u8; 32],
    pub target: ReviewedTarget,
    pub target_digest: [u8; 32],
    pub state: InteractionState,
    pub revision: u64,
    pub created_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
}

impl ConversationInteraction {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.origin.validate()?;
        self.requirement.validate()?;
        self.target.validate()?;
        self.state.validate()?;
        if self.id.is_nil()
            || !self.person_id.is_valid()
            || self.session_id.is_nil()
            || !self.origin_run_id.is_valid()
            || self.origin_turn_id != self.origin_run_id.as_uuid()
            || self.revision == 0
            || self.created_at_unix_ms < 0
            || self.expires_at_unix_ms != self.created_at_unix_ms + INTERACTION_PENDING_LIFETIME_MS
            || (self.kind == UserInteractionKind::ProcessingRecipient)
                != (self.requirement.kind == InteractionRequirementKind::ApproveProcessingRecipient)
            || (self.kind == UserInteractionKind::ProcessingRecipient)
                != matches!(self.target, ReviewedTarget::RecipientConsent(_))
            || (self.kind == UserInteractionKind::ExpertBinding)
                != (self.requirement.kind == InteractionRequirementKind::ConfigureExpertBinding)
            || (self.kind == UserInteractionKind::ExpertBinding)
                != matches!(self.target, ReviewedTarget::ExpertBinding(_))
            || self.requirement_digest == [0; 32]
            || self.target_digest == [0; 32]
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        if canonical_requirement_digest(&self.requirement)? != self.requirement_digest
            || canonical_target_digest(&self.target)? != self.target_digest
            || interaction_publication_id(
                self.origin_run_id,
                &self.origin,
                &self.requirement_digest,
                &self.target_digest,
            )? != self.id
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        if let InteractionState::Resolved { receipt } = &self.state
            && receipt.resolved_at_unix_ms < self.created_at_unix_ms
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }

    /// Whether the interaction still awaits user or owner work at `now`.
    pub fn is_actionable_at(&self, now_unix_ms: i64) -> bool {
        !self.state.is_terminal() && now_unix_ms < self.expires_at_unix_ms
    }

    /// Whether a read at `now` must project the expired state. Read-only get
    /// projects without writing; an explicit command persists Expired.
    pub fn projects_expired_at(&self, now_unix_ms: i64) -> bool {
        !self.state.is_terminal() && now_unix_ms >= self.expires_at_unix_ms
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionDecisionKind {
    Approve,
    Deny,
    Dismiss,
}

/// Durable decision intent, persisted before any owner mutation.
///
/// The stable command id is the idempotency key: the same command id with a
/// different digest conflicts, while an identical retry rejoins the recorded
/// decision before any revision check.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionDecision {
    pub command_id: Uuid,
    pub interaction_id: Uuid,
    pub interaction_revision: u64,
    pub kind: InteractionDecisionKind,
    pub target_digest: [u8; 32],
    pub principal: String,
    pub decided_at_unix_ms: i64,
}

impl InteractionDecision {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.command_id.is_nil()
            || self.interaction_id.is_nil()
            || self.interaction_revision == 0
            || self.target_digest == [0; 32]
            || self.principal.trim() != self.principal
            || self.principal.is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.decided_at_unix_ms < 0
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    /// Whether a retry carries the identical recorded decision.
    pub fn matches_recorded(&self, recorded: &Self) -> bool {
        self.command_id == recorded.command_id
            && self.interaction_id == recorded.interaction_id
            && self.kind == recorded.kind
            && self.target_digest == recorded.target_digest
            && self.principal == recorded.principal
    }
}

/// How many linked generations one resume chain may admit automatically.
///
/// A resume child whose own origin blocks again resumes at the next depth;
/// past this cap the person starts a fresh explicit turn instead of growing
/// an automatic chain.
pub const MAX_RESUME_LINEAGE: u8 = 3;

/// Which origin Run a linked fresh Run resumes, and at which chain depth.
///
/// The origin is the immediate parent whose interaction group the admission
/// re-verifies; the lineage is that parent's lineage plus one. It is not a
/// budget continuation: the child takes no batch, cursor, attempt identity
/// or transport from the origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractionResumeRef {
    pub origin_run_id: RunId,
    pub lineage: u8,
}

impl InteractionResumeRef {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.origin_run_id.is_valid() || self.lineage == 0 || self.lineage > MAX_RESUME_LINEAGE
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// The stable command identity for one origin's automatic resume slot.
///
/// Automatic and explicit-Continue paths derive the same command from the
/// same origin, so concurrent resolutions and repeated clicks rejoin one
/// canonical command instead of admitting siblings.
pub fn resume_command_id(origin_run_id: RunId) -> Result<CommandId, AgentFailure> {
    if !origin_run_id.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"floe.conversation.resume-command\0");
    bytes.extend_from_slice(origin_run_id.as_uuid().as_bytes());
    CommandId::from_uuid(Uuid::new_v5(&INTERACTION_ID_NAMESPACE, &bytes))
        .ok_or(AgentFailure::StorageUnavailable)
}

/// The stable owner-operation identity claimed by one decision command.
/// 05-C owner operations claim exactly this identity, so a retried decision
/// rejoins the same operation instead of performing the mutation again.
pub fn decision_operation_id(command_id: Uuid) -> Uuid {
    Uuid::new_v5(
        &INTERACTION_OPERATION_NAMESPACE,
        format!("floe.conversation.decision-operation\0{command_id}").as_bytes(),
    )
}

/// The state a decision moves the interaction to. Anything else conflicts:
/// terminal states never reopen through a decision, and a Resolving
/// interaction only accepts a cancelling Dismiss (which cannot pretend an
/// already committed owner mutation did not occur).
pub fn next_state_after_decision(
    state: &InteractionState,
    decision: &InteractionDecision,
) -> Result<InteractionState, AgentFailure> {
    match (state, decision.kind) {
        (InteractionState::Pending, InteractionDecisionKind::Approve) => {
            Ok(InteractionState::Resolving {
                decision_id: decision.command_id,
                owner_operation_id: decision_operation_id(decision.command_id),
            })
        }
        (InteractionState::Pending, InteractionDecisionKind::Deny) => {
            Ok(InteractionState::Denied {
                decision_id: decision.command_id,
            })
        }
        (
            InteractionState::Pending | InteractionState::Resolving { .. },
            InteractionDecisionKind::Dismiss,
        ) => Ok(InteractionState::Cancelled {
            decision_id: decision.command_id,
        }),
        _ => Err(AgentFailure::Conflict),
    }
}

pub fn state_after_resolution(
    state: &InteractionState,
    decision_id: Uuid,
    owner_operation_id: Uuid,
    resolved_at_unix_ms: i64,
) -> Result<InteractionState, AgentFailure> {
    match state {
        InteractionState::Resolving {
            decision_id: recorded_decision,
            owner_operation_id: recorded_operation,
        } if *recorded_decision == decision_id
            && *recorded_operation == owner_operation_id
            && !decision_id.is_nil()
            && !owner_operation_id.is_nil()
            && resolved_at_unix_ms >= 0 =>
        {
            Ok(InteractionState::Resolved {
                receipt: InteractionResolutionReceipt {
                    decision_id,
                    owner_operation_id,
                    resolved_at_unix_ms,
                },
            })
        }
        _ => Err(AgentFailure::Conflict),
    }
}

/// Canonical requirement digest: every semantic field, no display text.
/// Resource-like sets are canonical sorted order by validation.
pub fn canonical_requirement_digest(
    requirement: &InteractionRequirement,
) -> Result<[u8; 32], AgentFailure> {
    requirement
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"floe.conversation.interaction-requirement\0");
    bytes.push(match requirement.kind {
        InteractionRequirementKind::EnableObserve => 1,
        InteractionRequirementKind::ReviewChangedSource => 2,
        InteractionRequirementKind::RequestSystemPermission => 3,
        InteractionRequirementKind::Reconnect => 4,
        InteractionRequirementKind::ApproveProcessingRecipient => 5,
        InteractionRequirementKind::SelectResource => 6,
        InteractionRequirementKind::ConfigureExpertBinding => 7,
    });
    append_str(&mut bytes, &requirement.source_id);
    match &requirement.connection_id {
        Some(connection_id) => {
            bytes.push(1);
            append_str(&mut bytes, connection_id);
        }
        None => bytes.push(0),
    }
    append_str(&mut bytes, &requirement.consumer);
    append_str(&mut bytes, &requirement.purpose);
    bytes.push(u8::from(requirement.inline));
    Ok(Sha256::digest(bytes).into())
}

/// Canonical reviewed-target digest: exact authority and expected absence,
/// sorted set fields, never display text (the target carries none).
pub fn canonical_target_digest(target: &ReviewedTarget) -> Result<[u8; 32], AgentFailure> {
    target.validate().map_err(|_| AgentFailure::InvalidInput)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"floe.conversation.reviewed-target\0");
    match target {
        ReviewedTarget::InlineObserve(target) => {
            bytes.push(1);
            append_str(&mut bytes, &target.connection_id);
            match &target.device_id {
                Some(device_id) => {
                    bytes.push(1);
                    append_str(&mut bytes, device_id);
                }
                None => bytes.push(0),
            }
            append_str(&mut bytes, &target.source_id);
            match &target.connector_id {
                Some(connector_id) => {
                    bytes.push(1);
                    append_str(&mut bytes, connector_id);
                }
                None => bytes.push(0),
            }
            append_str(&mut bytes, &target.consumer);
            append_str(&mut bytes, &target.purpose);
            match &target.connection_revision {
                Some(revision) => {
                    bytes.push(1);
                    bytes.extend_from_slice(&revision.to_be_bytes());
                }
                None => bytes.push(0),
            }
            match &target.reviewed_producer_fingerprint {
                Some(fingerprint) => {
                    bytes.push(1);
                    append_str(&mut bytes, fingerprint);
                }
                None => bytes.push(0),
            }
            match &target.reviewed_native_subject {
                Some(subject) => {
                    bytes.push(1);
                    append_str(&mut bytes, subject);
                }
                None => bytes.push(0),
            }
            let member_count = u64::try_from(target.members.len()).unwrap_or(u64::MAX);
            bytes.extend_from_slice(&member_count.to_be_bytes());
            for member in &target.members {
                append_str(&mut bytes, &member.member_id);
                append_str(&mut bytes, &member.policy_fingerprint);
                append_str(&mut bytes, &member.resource);
                match &member.source_revision {
                    Some(revision) => {
                        bytes.push(1);
                        append_authority(&mut bytes, revision);
                    }
                    None => bytes.push(0),
                }
                match &member.expected_grant {
                    ExpectedGrantState::Absent => bytes.push(0),
                    ExpectedGrantState::Active {
                        grant_id,
                        authority_incarnation,
                        authority_epoch,
                    } => {
                        bytes.push(1);
                        bytes.extend_from_slice(grant_id.as_bytes());
                        bytes.extend_from_slice(authority_incarnation.as_bytes());
                        bytes.extend_from_slice(&authority_epoch.to_be_bytes());
                    }
                }
                match &member.policy_authority {
                    Some(authority) => {
                        bytes.push(1);
                        append_authority(&mut bytes, authority);
                    }
                    None => bytes.push(0),
                }
            }
        }
        ReviewedTarget::NavigationOnly(target) => {
            bytes.push(2);
            bytes.push(match target.destination {
                NavigationDestination::ConnectionSettings => 1,
                NavigationDestination::SystemPermission => 2,
                NavigationDestination::ResourcePicker => 3,
            });
            append_str(&mut bytes, &target.source_id);
            match &target.connection_id {
                Some(connection_id) => {
                    bytes.push(1);
                    append_str(&mut bytes, connection_id);
                }
                None => bytes.push(0),
            }
            append_str(&mut bytes, &target.consumer);
            append_str(&mut bytes, &target.purpose);
        }
        ReviewedTarget::RecipientConsent(target) => {
            bytes.push(3);
            append_str(&mut bytes, &target.recipient);
            append_str(&mut bytes, &target.profile_id);
            append_str(&mut bytes, &target.purpose);
            append_str(&mut bytes, &target.consumer);
            let class_count = u64::try_from(target.input_data_classes.len()).unwrap_or(u64::MAX);
            bytes.extend_from_slice(&class_count.to_be_bytes());
            for class in &target.input_data_classes {
                bytes.push(data_class_byte(class));
            }
            let scope_count = u64::try_from(target.source_scopes.len()).unwrap_or(u64::MAX);
            bytes.extend_from_slice(&scope_count.to_be_bytes());
            for scope in &target.source_scopes {
                let encoded = serde_json::to_vec(scope).map_err(|_| AgentFailure::InvalidInput)?;
                append_bytes(&mut bytes, &encoded);
            }
            bytes.extend_from_slice(target.lineage.session_id().as_bytes());
            bytes.extend_from_slice(target.lineage.origin_run_id().as_bytes());
            append_str(&mut bytes, &target.device_id);
            bytes.extend_from_slice(target.projection_ref.as_bytes());
            bytes.extend_from_slice(&target.projection_revision.to_be_bytes());
        }
        ReviewedTarget::ExpertBinding(target) => {
            bytes.push(4);
            bytes.extend_from_slice(target.registry_instance_id.as_bytes());
            bytes.extend_from_slice(target.assignment_id.as_bytes());
            append_str(&mut bytes, &target.package.id);
            append_str(&mut bytes, &target.package.version);
            bytes.extend_from_slice(&target.definition_revision.to_be_bytes());
            append_str(&mut bytes, &target.requirement_key);
            append_str(&mut bytes, &target.capability);
            bytes.extend_from_slice(&target.contract_version.to_be_bytes());
            bytes.push(target.minimum_sources);
            bytes.push(target.maximum_sources);
            bytes.extend_from_slice(&target.expected_binding_revision.to_be_bytes());
            bytes.extend_from_slice(&target.admitted_selection_digest);
        }
    }
    Ok(Sha256::digest(bytes).into())
}

/// Deterministic publication identity: the admitted origin plus the canonical
/// requirement/target digests. Replaying the same publication derives the
/// same id, so crash replay settles instead of duplicating.
pub fn interaction_publication_id(
    origin_run_id: RunId,
    origin: &InteractionOrigin,
    requirement_digest: &[u8; 32],
    target_digest: &[u8; 32],
) -> Result<Uuid, AgentFailure> {
    if !origin_run_id.is_valid()
        || origin.validate().is_err()
        || requirement_digest == &[0; 32]
        || target_digest == &[0; 32]
    {
        return Err(AgentFailure::InvalidInput);
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"floe.conversation.interaction-publication\0");
    bytes.extend_from_slice(origin_run_id.as_uuid().as_bytes());
    match origin {
        InteractionOrigin::Tool { call_id } => {
            bytes.push(1);
            bytes.extend_from_slice(call_id.as_bytes());
        }
        InteractionOrigin::Task {
            task_id,
            capability_call_id,
        } => {
            bytes.push(2);
            bytes.extend_from_slice(task_id.as_bytes());
            match capability_call_id {
                Some(call_id) => {
                    bytes.push(1);
                    bytes.extend_from_slice(call_id.as_bytes());
                }
                None => bytes.push(0),
            }
        }
        InteractionOrigin::Model { attempt_id } => {
            bytes.push(3);
            bytes.extend_from_slice(attempt_id.as_bytes());
        }
    }
    bytes.extend_from_slice(requirement_digest);
    bytes.extend_from_slice(target_digest);
    Ok(Uuid::new_v5(&INTERACTION_ID_NAMESPACE, &bytes))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublishAdmission {
    Created(ConversationInteraction),
    Existing(ConversationInteraction),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecisionAdmission {
    Applied(ConversationInteraction),
    Rejoined(ConversationInteraction),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpireOutcome {
    Expired(ConversationInteraction),
    AlreadyTerminal(ConversationInteraction),
    NotExpired(ConversationInteraction),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteractionResolution {
    pub interaction_id: Uuid,
    pub person_id: PersonId,
    pub expected_revision: u64,
    pub decision_id: Uuid,
    pub owner_operation_id: Uuid,
    pub resolved_at_unix_ms: i64,
}

impl InteractionResolution {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.interaction_id.is_nil()
            || !self.person_id.is_valid()
            || self.expected_revision == 0
            || self.decision_id.is_nil()
            || self.owner_operation_id.is_nil()
            || self.resolved_at_unix_ms < 0
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupersedeInteraction {
    pub interaction_id: Uuid,
    pub person_id: PersonId,
    pub expected_revision: u64,
    pub superseded_by: Option<Uuid>,
}

impl SupersedeInteraction {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.interaction_id.is_nil()
            || !self.person_id.is_valid()
            || self.expected_revision == 0
            || self.superseded_by.is_some_and(|id| id.is_nil())
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpireInteraction {
    pub interaction_id: Uuid,
    pub person_id: PersonId,
    pub now_unix_ms: i64,
}

impl ExpireInteraction {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.interaction_id.is_nil() || !self.person_id.is_valid() || self.now_unix_ms < 0 {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

fn append_authority(bytes: &mut Vec<u8>, revision: &AuthorityRevision) {
    bytes.extend_from_slice(revision.incarnation.as_bytes());
    bytes.extend_from_slice(&revision.epoch.to_be_bytes());
}

fn data_class_byte(class: &DataClass) -> u8 {
    match class {
        DataClass::Synthetic => 1,
        DataClass::Personal => 2,
        DataClass::TemporaryAiContext => 3,
        DataClass::HighlySensitive => 4,
        DataClass::DeviceOnlyRaw => 5,
        DataClass::Credential => 6,
    }
}

fn append_str(bytes: &mut Vec<u8>, value: &str) {
    append_bytes(bytes, value.as_bytes());
}

fn append_bytes(bytes: &mut Vec<u8>, value: &[u8]) {
    let length = u64::try_from(value.len()).unwrap_or(u64::MAX);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(value);
}

fn validate_identifier(value: &str, limit: usize) -> Result<(), AgentFailure> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > limit
        || value.chars().any(char::is_control)
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn requirement() -> InteractionRequirement {
        InteractionRequirement {
            kind: InteractionRequirementKind::EnableObserve,
            source_id: "floe.source.calendar".into(),
            connection_id: Some("calendar-connection".into()),
            consumer: "floe.builtin.schedule".into(),
            purpose: "scheduling".into(),
            inline: true,
        }
    }

    pub(crate) fn member(member_id: &str, resource: &str) -> ReviewedBundleMember {
        ReviewedBundleMember {
            member_id: member_id.into(),
            policy_fingerprint: "a".repeat(64),
            resource: resource.into(),
            source_revision: None,
            expected_grant: ExpectedGrantState::Absent,
            policy_authority: None,
        }
    }

    pub(crate) fn target() -> ReviewedTarget {
        ReviewedTarget::InlineObserve(InlineObserveTarget {
            connection_id: "calendar-connection".into(),
            device_id: None,
            source_id: "floe.source.calendar".into(),
            connector_id: Some("floe.connector.calendar".into()),
            consumer: "floe.builtin.schedule".into(),
            purpose: "scheduling".into(),
            connection_revision: None,
            reviewed_producer_fingerprint: None,
            reviewed_native_subject: None,
            members: vec![member("calendar.timeline", "personal")],
        })
    }

    pub(crate) fn record() -> ConversationInteraction {
        let requirement = requirement();
        let target = target();
        let requirement_digest = canonical_requirement_digest(&requirement).unwrap();
        let target_digest = canonical_target_digest(&target).unwrap();
        let origin_run_id = RunId::new();
        let origin = InteractionOrigin::Tool {
            call_id: Uuid::new_v4(),
        };
        let id =
            interaction_publication_id(origin_run_id, &origin, &requirement_digest, &target_digest)
                .unwrap();
        ConversationInteraction {
            id,
            person_id: PersonId::new(),
            session_id: Uuid::new_v4(),
            origin_run_id,
            origin_turn_id: origin_run_id.as_uuid(),
            origin,
            kind: UserInteractionKind::SourceAccess,
            requirement,
            requirement_digest,
            target,
            target_digest,
            state: InteractionState::Pending,
            revision: 1,
            created_at_unix_ms: 1_700_000_000_000,
            expires_at_unix_ms: 1_700_000_000_000 + INTERACTION_PENDING_LIFETIME_MS,
        }
    }

    #[test]
    fn valid_record_round_trips() {
        let record = record();
        assert!(record.validate().is_ok());
        let decoded: ConversationInteraction =
            serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
        assert_eq!(decoded, record);
    }

    #[test]
    fn identity_is_deterministic_and_binds_origin_and_digests() {
        let first = record();
        let mut second = first.clone();
        second.session_id = Uuid::new_v4();
        second.person_id = PersonId::new();
        let rebuilt = interaction_publication_id(
            second.origin_run_id,
            &second.origin,
            &second.requirement_digest,
            &second.target_digest,
        )
        .unwrap();
        assert_eq!(rebuilt, first.id);

        let mut changed = first.clone();
        changed.origin = InteractionOrigin::Tool {
            call_id: Uuid::new_v4(),
        };
        let changed_id = interaction_publication_id(
            changed.origin_run_id,
            &changed.origin,
            &changed.requirement_digest,
            &changed.target_digest,
        )
        .unwrap();
        assert_ne!(changed_id, first.id);
    }

    #[test]
    fn digests_distinguish_targets_and_expected_absence() {
        let base = canonical_target_digest(&target()).unwrap();
        let mut changed_policy = target();
        let ReviewedTarget::InlineObserve(inline) = &mut changed_policy else {
            panic!("test target is inline");
        };
        inline.members[0].policy_fingerprint = "b".repeat(64);
        assert_ne!(canonical_target_digest(&changed_policy).unwrap(), base);
        let ReviewedTarget::InlineObserve(inline) = &mut changed_policy else {
            panic!("test target is inline");
        };
        inline.members[0].policy_fingerprint = "B".repeat(64);
        assert!(canonical_target_digest(&changed_policy).is_err());
        let mut absent = target();
        let ReviewedTarget::InlineObserve(inline) = &mut absent else {
            panic!("test target is inline");
        };
        inline.members[0].expected_grant = ExpectedGrantState::Active {
            grant_id: Uuid::new_v4(),
            authority_incarnation: Uuid::new_v4(),
            authority_epoch: 3,
        };
        assert_ne!(canonical_target_digest(&absent).unwrap(), base);

        let mut reordered = target();
        let ReviewedTarget::InlineObserve(inline) = &mut reordered else {
            panic!("test target is inline");
        };
        inline.members = vec![
            member("calendar.timeline", "work"),
            member("calendar.timeline", "personal"),
        ];
        assert!(canonical_target_digest(&reordered).is_err());
    }

    #[test]
    fn kind_and_requirement_coherence_is_enforced() {
        let mut processing = record();
        processing.kind = UserInteractionKind::ProcessingRecipient;
        assert_eq!(processing.validate(), Err(AgentFailure::StorageUnavailable));
        processing.requirement.kind = InteractionRequirementKind::ApproveProcessingRecipient;
        processing.requirement_digest =
            canonical_requirement_digest(&processing.requirement).unwrap();
        // A processing interaction with a source target is still incoherent.
        assert_eq!(processing.validate(), Err(AgentFailure::StorageUnavailable));
        processing.target = ReviewedTarget::RecipientConsent(consent_target());
        processing.target_digest = canonical_target_digest(&processing.target).unwrap();
        processing.id = interaction_publication_id(
            processing.origin_run_id,
            &processing.origin,
            &processing.requirement_digest,
            &processing.target_digest,
        )
        .unwrap();
        assert!(processing.validate().is_ok());
        // A source interaction with a consent target is incoherent.
        let mut mismatched = record();
        mismatched.target = ReviewedTarget::RecipientConsent(consent_target());
        mismatched.target_digest = canonical_target_digest(&mismatched.target).unwrap();
        mismatched.id = interaction_publication_id(
            mismatched.origin_run_id,
            &mismatched.origin,
            &mismatched.requirement_digest,
            &mismatched.target_digest,
        )
        .unwrap();
        assert_eq!(mismatched.validate(), Err(AgentFailure::StorageUnavailable));
    }

    pub(crate) fn consent_target() -> RecipientConsentTarget {
        RecipientConsentTarget {
            recipient: "model.example".into(),
            profile_id: "server-model".into(),
            purpose: "everyday_assistance".into(),
            consumer: "conversation.root".into(),
            input_data_classes: vec![DataClass::Personal],
            source_scopes: vec![],
            lineage: RecipientLineage::try_new(Uuid::new_v4(), Uuid::new_v4()).unwrap(),
            device_id: "device".into(),
            projection_ref: Uuid::new_v4(),
            projection_revision: 1,
        }
    }

    #[test]
    fn consent_target_validates_digest_binds_and_round_trips() {
        let target = consent_target();
        assert!(target.validate().is_ok());
        let digest =
            canonical_target_digest(&ReviewedTarget::RecipientConsent(target.clone())).unwrap();
        assert_ne!(digest, [0; 32]);
        let decoded: RecipientConsentTarget =
            serde_json::from_str(&serde_json::to_string(&target).unwrap()).unwrap();
        assert_eq!(decoded, target);
        // Every reviewed field enters the digest.
        let mut changed = target.clone();
        changed.recipient = "other.example".into();
        assert_ne!(
            canonical_target_digest(&ReviewedTarget::RecipientConsent(changed)).unwrap(),
            digest
        );
        let mut changed = target.clone();
        changed.profile_id = "other-model".into();
        assert_ne!(
            canonical_target_digest(&ReviewedTarget::RecipientConsent(changed)).unwrap(),
            digest
        );
        let mut changed = target.clone();
        changed.device_id = "other-device".into();
        assert_ne!(
            canonical_target_digest(&ReviewedTarget::RecipientConsent(changed)).unwrap(),
            digest
        );
        let mut changed = target.clone();
        changed.lineage = RecipientLineage::try_new(Uuid::new_v4(), Uuid::new_v4()).unwrap();
        assert_ne!(
            canonical_target_digest(&ReviewedTarget::RecipientConsent(changed)).unwrap(),
            digest
        );
        // Blank, wildcard, unsorted, and oversized reviews are rejected.
        let mut blank = target.clone();
        blank.recipient = String::new();
        assert!(blank.validate().is_err());
        let mut wildcard = target.clone();
        wildcard.recipient = "*".into();
        assert!(wildcard.validate().is_err());
        let mut blank_device = target.clone();
        blank_device.device_id = String::new();
        assert!(blank_device.validate().is_err());
        let mut unsorted = target.clone();
        unsorted.input_data_classes = vec![DataClass::Personal, DataClass::Synthetic];
        assert!(unsorted.validate().is_err());
        let mut oversized = target.clone();
        oversized.device_id = "x".repeat(MAX_REVIEWED_IDENTIFIER_BYTES + 1);
        assert!(oversized.validate().is_err());
    }

    #[test]
    fn expert_binding_target_binds_assignment_revision_and_selection_without_source() {
        let target = ExpertBindingTarget {
            registry_instance_id: Uuid::new_v4(),
            assignment_id: Uuid::new_v4(),
            package: PackageRef {
                kind: PackageKind::Expert,
                id: "example.test.expert".into(),
                version: "1.0.0".into(),
            },
            definition_revision: 1,
            requirement_key: "floe.source.calendar".into(),
            capability: "calendar.timeline".into(),
            contract_version: 1,
            minimum_sources: 1,
            maximum_sources: 16,
            expected_binding_revision: 2,
            admitted_selection_digest: [7; 32],
        };
        target.validate().unwrap();
        let reviewed = ReviewedTarget::ExpertBinding(target.clone());
        let digest = canonical_target_digest(&reviewed).unwrap();
        let mut changed = target.clone();
        changed.expected_binding_revision += 1;
        assert_ne!(
            canonical_target_digest(&ReviewedTarget::ExpertBinding(changed)).unwrap(),
            digest
        );
        let mut changed = target.clone();
        changed.admitted_selection_digest = [8; 32];
        assert_ne!(
            canonical_target_digest(&ReviewedTarget::ExpertBinding(changed)).unwrap(),
            digest
        );
        let encoded = serde_json::to_string(&target).unwrap();
        assert!(!encoded.contains("connector_id"));
        assert!(!encoded.contains("grant_id"));
        assert_eq!(
            serde_json::from_str::<ExpertBindingTarget>(&encoded).unwrap(),
            target
        );
        let mut interaction = record();
        interaction.kind = UserInteractionKind::ExpertBinding;
        interaction.requirement.kind = InteractionRequirementKind::ConfigureExpertBinding;
        interaction.requirement_digest =
            canonical_requirement_digest(&interaction.requirement).unwrap();
        interaction.target = reviewed;
        interaction.target_digest = digest;
        interaction.id = interaction_publication_id(
            interaction.origin_run_id,
            &interaction.origin,
            &interaction.requirement_digest,
            &interaction.target_digest,
        )
        .unwrap();
        interaction.validate().unwrap();
    }

    #[test]
    fn tampered_digests_and_identity_fail_validation() {
        let mut tampered = record();
        tampered.target_digest = [7; 32];
        assert_eq!(tampered.validate(), Err(AgentFailure::StorageUnavailable));
        let mut reidentified = record();
        reidentified.id = Uuid::new_v4();
        assert_eq!(
            reidentified.validate(),
            Err(AgentFailure::StorageUnavailable)
        );
    }

    #[test]
    fn descriptor_bounds_reject_oversized_and_unsorted_targets() {
        let mut unsorted = target();
        let ReviewedTarget::InlineObserve(inline) = &mut unsorted else {
            panic!("test target is inline");
        };
        inline.members = vec![
            member("calendar.timeline", "b"),
            member("calendar.timeline", "a"),
        ];
        assert!(unsorted.validate().is_err());

        let mut oversized = target();
        let ReviewedTarget::InlineObserve(inline) = &mut oversized else {
            panic!("test target is inline");
        };
        inline.members = (0..=MAX_TARGET_BUNDLE_MEMBERS)
            .map(|index| member("calendar.timeline", &format!("resource-{index:04}")))
            .collect();
        assert!(oversized.validate().is_err());

        let mut empty = target();
        let ReviewedTarget::InlineObserve(inline) = &mut empty else {
            panic!("test target is inline");
        };
        inline.members.clear();
        assert!(empty.validate().is_err());

        let mut requirement = requirement();
        requirement.source_id = "x".repeat(MAX_REVIEWED_SOURCE_BYTES + 1);
        assert!(requirement.validate().is_err());
    }

    #[test]
    fn expiry_projects_without_writing() {
        let record = record();
        assert!(record.is_actionable_at(record.created_at_unix_ms));
        assert!(!record.projects_expired_at(record.created_at_unix_ms));
        assert!(!record.is_actionable_at(record.expires_at_unix_ms));
        assert!(record.projects_expired_at(record.expires_at_unix_ms));
        let mut terminal = record.clone();
        terminal.state = InteractionState::Denied {
            decision_id: Uuid::new_v4(),
        };
        assert!(!terminal.projects_expired_at(i64::MAX));
    }

    #[test]
    fn decision_transitions_follow_the_lifecycle() {
        let pending = InteractionState::Pending;
        let approve = InteractionDecision {
            command_id: Uuid::new_v4(),
            interaction_id: Uuid::new_v4(),
            interaction_revision: 1,
            kind: InteractionDecisionKind::Approve,
            target_digest: [1; 32],
            principal: PersonId::new().to_string(),
            decided_at_unix_ms: 1,
        };
        let resolving = next_state_after_decision(&pending, &approve).unwrap();
        let InteractionState::Resolving {
            decision_id,
            owner_operation_id,
        } = resolving
        else {
            panic!("approve must resolve");
        };
        assert_eq!(decision_id, approve.command_id);
        assert_eq!(
            owner_operation_id,
            decision_operation_id(approve.command_id)
        );

        let deny = InteractionDecision {
            kind: InteractionDecisionKind::Deny,
            ..approve.clone()
        };
        assert!(matches!(
            next_state_after_decision(&pending, &deny).unwrap(),
            InteractionState::Denied { .. }
        ));
        assert!(next_state_after_decision(&resolving, &deny).is_err());

        let dismiss = InteractionDecision {
            kind: InteractionDecisionKind::Dismiss,
            ..approve.clone()
        };
        assert!(matches!(
            next_state_after_decision(&pending, &dismiss).unwrap(),
            InteractionState::Cancelled { .. }
        ));
        assert!(matches!(
            next_state_after_decision(&resolving, &dismiss).unwrap(),
            InteractionState::Cancelled { .. }
        ));
        assert!(next_state_after_decision(&resolving, &approve).is_err());

        let resolved = state_after_resolution(
            &resolving,
            decision_id,
            owner_operation_id,
            approve.decided_at_unix_ms,
        )
        .unwrap();
        assert!(matches!(resolved, InteractionState::Resolved { .. }));
        assert!(state_after_resolution(&pending, decision_id, owner_operation_id, 1).is_err());
        assert!(state_after_resolution(&resolving, Uuid::new_v4(), owner_operation_id, 1).is_err());
    }

    #[test]
    fn identical_decision_matches_recorded_command() {
        let decision = InteractionDecision {
            command_id: Uuid::new_v4(),
            interaction_id: Uuid::new_v4(),
            interaction_revision: 1,
            kind: InteractionDecisionKind::Approve,
            target_digest: [1; 32],
            principal: "person".into(),
            decided_at_unix_ms: 1,
        };
        assert!(decision.validate().is_ok());
        let mut changed = decision.clone();
        changed.target_digest = [2; 32];
        assert!(!decision.matches_recorded(&changed));
        assert!(decision.matches_recorded(&decision.clone()));
    }
}
