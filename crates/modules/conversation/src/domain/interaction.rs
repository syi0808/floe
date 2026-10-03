//! Conversation-owned durable user interactions.
//!
//! An interaction records that a running turn reached a recoverable owner
//! requirement: the admitted origin (a Tool call, Delegation Task or pre-model
//! projection), the owner-produced semantic request, and the immutable reviewed
//! target the person's decision binds. Conversation owns interaction identity,
//! origin, lifecycle, decision intent and resume linkage; it never owns source,
//! grant or processing authority. Those stay with Access, Connections and the
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

use floe_agent_contract::{AgentFailure, TaskExecutionReceiptRef, UserInteractionKind};
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

/// Fixed namespace for deterministic interaction publication identity.
pub const INTERACTION_ID_NAMESPACE: Uuid =
    Uuid::from_u128(0x9f2c_4b1e_8d3a_4f6b_9c1d_2e3f_4a5b_6c7d);
/// Fixed namespace for the stable owner-operation identity of a decision.
pub const INTERACTION_OPERATION_NAMESPACE: Uuid =
    Uuid::from_u128(0x3b7e_1a90_c4d2_4e5f_8a6b_0c1d_2e3f_4a5b);

/// The admitted invocation that produced the requirement.
///
/// Identity is verified against the origin Run's durable journal: a Tool
/// origin needs its ToolIntent, a Task origin its DelegationIntent, a Projection
/// origin its atomic blocked-run publication. An origin that the journal never admitted is
/// rejected, however well-formed its ids look.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "origin", rename_all = "snake_case", deny_unknown_fields)]
pub enum InteractionOrigin {
    Task {
        execution: TaskExecutionReceiptRef,
        capability_call_id: Option<Uuid>,
    },
    Projection {
        run_id: RunId,
        projection_operation_id: Uuid,
        target_digest: [u8; 32],
    },
}

impl InteractionOrigin {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        let valid = match self {
            Self::Task {
                execution,
                capability_call_id,
            } => {
                execution.validate().is_ok()
                    && capability_call_id.is_none_or(|call_id| !call_id.is_nil())
            }
            Self::Projection {
                run_id,
                projection_operation_id,
                target_digest,
            } => {
                run_id.is_valid() && !projection_operation_id.is_nil() && *target_digest != [0; 32]
            }
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
    ReviewProcessing,
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
    pub fn from_source(
        blocker: &floe_context_contract::SourceAccessRequirement,
        inline: bool,
    ) -> Self {
        use floe_context_contract::SourceAccessRequirementKind as Reason;
        InteractionRequirement {
            kind: match blocker.reason() {
                Reason::EnableObserve => InteractionRequirementKind::EnableObserve,
                Reason::ReviewChangedSource => InteractionRequirementKind::ReviewChangedSource,
                Reason::RequestSystemPermission => {
                    InteractionRequirementKind::RequestSystemPermission
                }
                Reason::Reconnect => InteractionRequirementKind::Reconnect,
                Reason::ReviewProcessing => InteractionRequirementKind::ReviewProcessing,
                Reason::SelectResource => InteractionRequirementKind::SelectResource,
            },
            source_id: blocker.source_id().to_owned(),
            connection_id: blocker.connection_id().map(|id| id.as_str().to_owned()),
            consumer: blocker.consumer().identifier().to_owned(),
            purpose: match blocker.purpose() {
                floe_context_contract::GrantPurpose::Assistant => "assistant",
                floe_context_contract::GrantPurpose::Scheduling => "scheduling",
                floe_context_contract::GrantPurpose::Summarization => "summarization",
            }
            .into(),
            inline,
        }
    }

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

/// The immutable reviewed descriptor a decision binds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ReviewedTarget {
    NavigationOnly(NavigationOnlyTarget),
    SourceReview(floe_access::ReviewRef),
    ExpertBinding(floe_experts::BindingReviewRef),
}

impl ReviewedTarget {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        match self {
            Self::NavigationOnly(target) => target.validate(),
            Self::SourceReview(reference) => reference.validate(),
            Self::ExpertBinding(target) => target.validate(),
        }
    }
}

/// Why Conversation reconciled an independently authenticated owner receipt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InteractionResolutionCause {
    Decision { command_id: Uuid },
    Refresh { command_id: Uuid },
}
impl InteractionResolutionCause {
    pub fn command_id(&self) -> Uuid {
        match self {
            Self::Decision { command_id } | Self::Refresh { command_id } => *command_id,
        }
    }
}

/// The semantic receipt of a completed resolution: which decision and which
/// stable owner operation produced it. Grant/authority facts stay with their
/// owners; this receipt is coordination evidence only.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionResolutionReceipt {
    pub cause: InteractionResolutionCause,
    pub owner_command_id: Uuid,
    pub owner_receipt: super::OwnerResolutionReceipt,
    pub owner_operation_id: Uuid,
    pub resolved_at_unix_ms: i64,
}

impl InteractionResolutionReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.owner_receipt.validate()?;
        if !matches!(
            (&self.cause, &self.owner_receipt),
            (
                InteractionResolutionCause::Decision { .. },
                super::OwnerResolutionReceipt::SourceProcessing { .. }
            ) | (
                InteractionResolutionCause::Refresh { .. },
                super::OwnerResolutionReceipt::ExpertBinding { .. }
            )
        ) {
            return Err(AgentFailure::StorageUnavailable);
        }
        if self.cause.command_id().is_nil()
            || self.owner_command_id.is_nil()
            || self.owner_receipt.command_id() != self.owner_command_id
            || self.owner_receipt.operation_id() != self.owner_operation_id
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
        owner_command_id: Uuid,
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
                owner_command_id,
            } => {
                if decision_id.is_nil() || owner_command_id.is_nil() {
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
    pub audit: super::ReviewAuditRecord,
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
        self.audit.validate()?;
        if self.audit.person_id != self.person_id
            || self.audit.session_id != self.session_id
            || self.audit.run_id != self.origin_run_id
            || !self.audit.targets().contains(&self.target)
            || !self
                .audit
                .matches_requirement(&self.requirement, &self.target)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        match (&self.origin, &self.audit.evidence) {
            (
                InteractionOrigin::Projection {
                    run_id,
                    projection_operation_id,
                    target_digest,
                },
                super::BlockedReviewEvidence::ModelProjection { review, .. },
            ) if *run_id == self.origin_run_id
                && *projection_operation_id == review.projection_operation_id
                && *target_digest == review.target_digest => {}
            (
                InteractionOrigin::Task {
                    execution,
                    capability_call_id,
                },
                super::BlockedReviewEvidence::SourceRead {
                    execution: evidence,
                    tool_call_id,
                    ..
                },
            ) if execution == evidence && *capability_call_id == Some(*tool_call_id) => {}
            (
                InteractionOrigin::Task {
                    execution,
                    capability_call_id: None,
                },
                super::BlockedReviewEvidence::ExpertBinding {
                    execution: evidence,
                    ..
                },
            ) if execution == evidence => {}
            (
                InteractionOrigin::Task { execution, .. },
                super::BlockedReviewEvidence::Navigation {
                    execution: evidence,
                    ..
                },
            ) if execution == evidence => {}
            (
                InteractionOrigin::Task {
                    execution,
                    capability_call_id: None,
                },
                super::BlockedReviewEvidence::TaskModelProjection {
                    execution: evidence,
                    ..
                },
            ) if execution == evidence => {}
            _ => return Err(AgentFailure::StorageUnavailable),
        }
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
            || matches!(self.target, ReviewedTarget::SourceReview(_))
                && self.kind != UserInteractionKind::SourceAccess
            || self.requirement.kind == InteractionRequirementKind::ReviewProcessing
                && !matches!(self.target, ReviewedTarget::SourceReview(_))
            || matches!(&self.origin, InteractionOrigin::Projection { run_id, .. } if *run_id != self.origin_run_id)
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
        if let InteractionState::Resolved { receipt } = &self.state {
            if receipt.resolved_at_unix_ms < self.created_at_unix_ms {
                return Err(AgentFailure::StorageUnavailable);
            }
            let matches_owner = match (&self.target, &receipt.owner_receipt) {
                (
                    ReviewedTarget::SourceReview(reference),
                    super::OwnerResolutionReceipt::SourceProcessing { receipt },
                ) => {
                    matches!(&receipt.kind, floe_access::GrantCommitKind::Reviewed { review } if review == reference)
                }
                (
                    ReviewedTarget::ExpertBinding(reference),
                    super::OwnerResolutionReceipt::ExpertBinding { receipt },
                ) => &receipt.review_ref == reference,
                _ => false,
            };
            if !matches_owner {
                return Err(AgentFailure::StorageUnavailable);
            }
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
            && self.interaction_revision == recorded.interaction_revision
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
pub fn decision_owner_command_id(command_id: Uuid) -> Uuid {
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
                owner_command_id: decision_owner_command_id(decision.command_id),
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
    resolution: &InteractionResolution,
    owner_receipt: &super::OwnerResolutionReceipt,
) -> Result<InteractionState, AgentFailure> {
    resolution.validate()?;
    owner_receipt.validate()?;
    match state {
        InteractionState::Resolving {
            decision_id,
            owner_command_id,
        } if resolution.cause
            == (InteractionResolutionCause::Decision {
                command_id: *decision_id,
            })
            && *owner_command_id == resolution.owner_command_id
            && owner_receipt.command_id() == *owner_command_id
            && owner_receipt.operation_id() == resolution.owner_operation_id =>
        {
            Ok(InteractionState::Resolved {
                receipt: InteractionResolutionReceipt {
                    cause: resolution.cause.clone(),
                    owner_command_id: *owner_command_id,
                    owner_operation_id: resolution.owner_operation_id,
                    owner_receipt: owner_receipt.clone(),
                    resolved_at_unix_ms: resolution.resolved_at_unix_ms,
                },
            })
        }
        InteractionState::Pending
            if matches!(resolution.cause, InteractionResolutionCause::Refresh { .. })
                && matches!(
                    owner_receipt,
                    super::OwnerResolutionReceipt::ExpertBinding { .. }
                )
                && owner_receipt.command_id() == resolution.owner_command_id
                && owner_receipt.operation_id() == resolution.owner_operation_id =>
        {
            Ok(InteractionState::Resolved {
                receipt: InteractionResolutionReceipt {
                    cause: resolution.cause.clone(),
                    owner_command_id: resolution.owner_command_id,
                    owner_operation_id: resolution.owner_operation_id,
                    owner_receipt: owner_receipt.clone(),
                    resolved_at_unix_ms: resolution.resolved_at_unix_ms,
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
        InteractionRequirementKind::ReviewProcessing => 5,
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
        ReviewedTarget::SourceReview(reference) => {
            bytes.push(3);
            bytes.extend_from_slice(reference.id.as_bytes());
            bytes.extend_from_slice(&reference.revision.to_be_bytes());
            bytes.extend_from_slice(&reference.digest);
        }
        ReviewedTarget::ExpertBinding(target) => {
            bytes.push(4);
            bytes.extend_from_slice(target.id.as_bytes());
            bytes.extend_from_slice(&target.digest);
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
        InteractionOrigin::Task {
            execution,
            capability_call_id,
        } => {
            bytes.push(2);
            bytes.extend_from_slice(execution.execution.task_id.as_uuid().as_bytes());
            bytes.extend_from_slice(execution.execution.execution_id.as_bytes());
            bytes.extend_from_slice(&execution.execution.executor_generation.to_be_bytes());
            bytes.extend_from_slice(&execution.task_revision.to_be_bytes());
            bytes.extend_from_slice(&execution.journal_revision.to_be_bytes());
            bytes.extend_from_slice(&execution.digest);
            match capability_call_id {
                Some(call_id) => {
                    bytes.push(1);
                    bytes.extend_from_slice(call_id.as_bytes());
                }
                None => bytes.push(0),
            }
        }
        InteractionOrigin::Projection {
            run_id,
            projection_operation_id,
            target_digest,
        } => {
            if *run_id != origin_run_id {
                return Err(AgentFailure::InvalidInput);
            }
            bytes.push(3);
            bytes.extend_from_slice(run_id.as_uuid().as_bytes());
            bytes.extend_from_slice(projection_operation_id.as_bytes());
            bytes.extend_from_slice(target_digest);
        }
    }
    bytes.extend_from_slice(requirement_digest);
    bytes.extend_from_slice(target_digest);
    Ok(Uuid::new_v5(&INTERACTION_ID_NAMESPACE, &bytes))
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
    pub owner_command_id: Uuid,
    pub target_digest: [u8; 32],
    pub interaction_id: Uuid,
    pub person_id: PersonId,
    pub expected_revision: u64,
    pub cause: InteractionResolutionCause,
    pub owner_operation_id: Uuid,
    pub resolved_at_unix_ms: i64,
}

impl InteractionResolution {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.owner_command_id.is_nil()
            || self.target_digest == [0; 32]
            || self.interaction_id.is_nil()
            || !self.person_id.is_valid()
            || self.expected_revision == 0
            || self.cause.command_id().is_nil()
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteractionRefresh {
    pub command_id: Uuid,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub interaction_id: Uuid,
    pub expected_revision: u64,
}
impl InteractionRefresh {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.command_id.is_nil()
            || !self.person_id.is_valid()
            || self.session_id.is_nil()
            || self.interaction_id.is_nil()
            || self.expected_revision == 0
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}
