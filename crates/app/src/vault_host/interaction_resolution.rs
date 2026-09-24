//! Reviewed decision execution for durable interactions.
//!
//! This module is the App composition of 05-C section 4 (decision execution
//! protocol) and section 6 (query/cancellation semantics). Conversation owns
//! the lifecycle and decision CAS; the source/Access owners own the mutation;
//! this module binds them:
//!
//! - `resolve` records the person's decision, re-reads current owner truth,
//!   compares it with the immutable reviewed target, invokes the canonical
//!   owner operation with the exact reviewed expectation, and records the
//!   semantic resolution.
//! - `refresh` is the explicit reconciliation command. It never enables a
//!   grant: it settles an already satisfied requirement, replaces a stale
//!   review with a freshly captured card, or reports the card unchanged.
//!
//! Fresh `Approve` requires the live state to match the reviewed
//! precondition exactly. Anything else (drift, out-of-band enablement,
//! partial mutation) supersedes the review instead of widening or silently
//! adopting it. Only reconciliation (a rejoined decision command, `refresh`,
//! or crash reopen) may settle an already satisfied requirement without a
//! mutation, and only through an explicit person command.
//!
//! Navigation-only targets never offer inline mutation: `resolve(Approve)`
//! rejects them. They resolve through `refresh` when the requirement is
//! already satisfied, or are denied/dismissed like any other card.

use std::num::NonZeroU64;

use floe_access::{GrantAuthority, GrantId};
use floe_agent_contract::{AgentFailure, BoxFuture};
use floe_context_contract::{ConsumerPolicyAuthority, SourceAuthority};
use floe_kernel::PersonId;
use uuid::Uuid;

use crate::CallerContext;

/// One live non-revoked grant covering a reviewed member resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LiveGrant {
    pub id: GrantId,
    pub authority: GrantAuthority,
}

/// Current owner truth for one reviewed bundle member, re-read at decision
/// time. Values are typed owner values, comparable with the reviewed
/// descriptor field by field.
#[derive(Clone, Debug)]
pub(crate) struct LiveMember {
    pub member_id: String,
    pub resource: String,
    pub source_revision: Option<SourceAuthority>,
    pub live_grants: Vec<LiveGrant>,
    pub policy_authority: Option<ConsumerPolicyAuthority>,
}

/// Current owner truth for an inline Observe review: every canonical
/// member probed now (reviewed keys plus any new canonical member), and the
/// bound owner identity.
#[derive(Clone, Debug)]
pub(crate) struct LiveInlineState {
    pub members: Vec<LiveMember>,
    pub connection_revision: Option<u64>,
    pub producer_fingerprint: Option<String>,
    pub native_subject: Option<String>,
    pub connection_usable: bool,
}

/// Why the reviewed target no longer binds the live state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DriftReason {
    ConnectionUnusable,
    ConnectionRevision,
    ProducerFingerprint,
    NativeSubject,
    MemberSet,
    SourceRevision {
        member_id: String,
        resource: String,
    },
    PolicyAuthority {
        member_id: String,
        resource: String,
    },
    GrantState {
        member_id: String,
        resource: String,
    },
    PartialMutation,
    /// Live state already satisfies the requirement. Reconciliation settles
    /// this; a fresh Allow treats it as a conflict because the reviewed
    /// card no longer describes reality.
    ConcurrentEnablement,
}

/// The live state matches the reviewed precondition (mutation still needed)
/// or already satisfies the requirement (no mutation needed).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReviewMatch {
    Precondition,
    Satisfied,
}

/// Compare the immutable reviewed target with freshly read owner truth.
///
/// Identity (connection usability/revision, pinned producer, live native
/// subject, member set, per-member source/policy revisions) must match
/// exactly. Grant expectations classify each member: reviewed absence with
/// no live grant needs the mutation; reviewed absence with exactly one live
/// grant is out-of-band satisfaction; a reviewed live grant must be the
/// identical grant. A mix of still-missing and out-of-band-satisfied members
/// is a partial mutation, never a widened approval.
pub(crate) fn compare_reviewed_live(
    reviewed: &floe_conversation::InlineObserveTarget,
    live: &LiveInlineState,
) -> Result<ReviewMatch, DriftReason> {
    if !live.connection_usable {
        return Err(DriftReason::ConnectionUnusable);
    }
    if let Some(reviewed_revision) = reviewed.connection_revision
        && live.connection_revision != Some(reviewed_revision)
    {
        return Err(DriftReason::ConnectionRevision);
    }
    if let Some(reviewed_producer) = reviewed.reviewed_producer_fingerprint.as_deref()
        && live.producer_fingerprint.as_deref() != Some(reviewed_producer)
    {
        return Err(DriftReason::ProducerFingerprint);
    }
    if let Some(reviewed_subject) = reviewed.reviewed_native_subject.as_deref()
        && live.native_subject.as_deref() != Some(reviewed_subject)
    {
        return Err(DriftReason::NativeSubject);
    }
    if live.members.iter().any(|current| {
        !reviewed.members.iter().any(|member| {
            member.member_id == current.member_id && member.resource == current.resource
        })
    }) {
        return Err(DriftReason::MemberSet);
    }
    let mut needs_mutation = false;
    let mut out_of_band = false;
    for member in &reviewed.members {
        let Some(current) = live.members.iter().find(|candidate| {
            candidate.member_id == member.member_id && candidate.resource == member.resource
        }) else {
            return Err(DriftReason::MemberSet);
        };
        if member.source_revision.is_some()
            && source_key(current.source_revision) != source_key(reviewed_source(&member))
        {
            return Err(DriftReason::SourceRevision {
                member_id: member.member_id.clone(),
                resource: member.resource.clone(),
            });
        }
        if member.policy_authority.is_some()
            && policy_key(current.policy_authority) != policy_key(reviewed_policy(&member))
        {
            return Err(DriftReason::PolicyAuthority {
                member_id: member.member_id.clone(),
                resource: member.resource.clone(),
            });
        }
        match (&member.expected_grant, current.live_grants.as_slice()) {
            (floe_conversation::ExpectedGrantState::Absent, []) => {
                needs_mutation = true;
            }
            (floe_conversation::ExpectedGrantState::Absent, [_]) => {
                out_of_band = true;
            }
            (
                floe_conversation::ExpectedGrantState::Active {
                    grant_id,
                    authority_incarnation,
                    authority_epoch,
                },
                [grant],
            ) if grant.id.as_uuid() == *grant_id
                && grant.authority.incarnation() == *authority_incarnation
                && grant.authority.access_epoch().get() == *authority_epoch =>
            {
                // Reviewed live grant, unchanged: no mutation for this member.
            }
            _ => {
                return Err(DriftReason::GrantState {
                    member_id: member.member_id.clone(),
                    resource: member.resource.clone(),
                });
            }
        }
    }
    if needs_mutation && out_of_band {
        return Err(DriftReason::PartialMutation);
    }
    if needs_mutation {
        Ok(ReviewMatch::Precondition)
    } else {
        Ok(ReviewMatch::Satisfied)
    }
}

fn source_key(authority: Option<SourceAuthority>) -> Option<(Uuid, u64)> {
    authority.map(|value| (value.incarnation(), value.epoch().get()))
}

fn policy_key(authority: Option<ConsumerPolicyAuthority>) -> Option<(Uuid, u64)> {
    authority.map(|value| (value.incarnation(), value.epoch().get()))
}

fn reviewed_source(member: &floe_conversation::ReviewedBundleMember) -> Option<SourceAuthority> {
    member.source_revision.as_ref().and_then(|revision| {
        NonZeroU64::new(revision.epoch)
            .and_then(|epoch| SourceAuthority::from_parts(revision.incarnation, epoch))
    })
}

fn reviewed_policy(
    member: &floe_conversation::ReviewedBundleMember,
) -> Option<ConsumerPolicyAuthority> {
    member.policy_authority.as_ref().and_then(|revision| {
        NonZeroU64::new(revision.epoch)
            .and_then(|epoch| ConsumerPolicyAuthority::from_parts(revision.incarnation, epoch))
    })
}

/// Reads current owner truth for decision-time comparison.
///
/// Implementations query owners (never model output): vault grants and
/// policies, core connections, live native subjects, the pinned producer.
/// Failure fails the decision closed; it never substitutes fresh values for
/// the reviewed descriptor.
pub(crate) trait ObserveStateReader: Send + Sync {
    fn read_live_inline<'a>(
        &'a self,
        target: &'a floe_conversation::InlineObserveTarget,
        person_id: PersonId,
        device_id: &'a str,
        cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<LiveInlineState, AgentFailure>>;

    /// Whether the navigation target's owning connection still exists and is
    /// usable. A dead connection supersedes the card; otherwise the card
    /// stays actionable.
    fn navigation_connection_usable<'a>(
        &'a self,
        target: &'a floe_conversation::NavigationOnlyTarget,
        person_id: PersonId,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>>;

    /// Whether the navigation requirement is already fully satisfied: the
    /// original read would now be admitted without any mutation.
    fn navigation_satisfied<'a>(
        &'a self,
        target: &'a floe_conversation::NavigationOnlyTarget,
        person_id: PersonId,
        device_id: &'a str,
        cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>>;
}

/// Performs the canonical owner mutation for a reviewed inline target.
///
/// Implementations call the same owner operation the owning connection
/// screen uses, with the exact reviewed expectation: native
/// `CalendarAccessChange::Review`, personal `PersonalAccessChange::Review`,
/// or remote `ConnectionObserve` enable. The operation re-verifies live
/// before committing; this trait never widens or fabricates expectations.
pub(crate) trait InlineOwnerMutation: Send + Sync {
    fn enable_reviewed<'a>(
        &'a self,
        target: &'a floe_conversation::InlineObserveTarget,
        person_id: PersonId,
        device_id: &'a str,
        operation_id: Uuid,
        cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;
}

/// A person's decision on one interaction: stable command identity,
/// Session/revision CAS, and the reviewed digest the decision binds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolveInteractionCommand {
    pub interaction_id: Uuid,
    pub command_id: Uuid,
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub kind: floe_conversation::InteractionDecisionKind,
    pub target_digest: [u8; 32],
}

impl ResolveInteractionCommand {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.interaction_id.is_nil()
            || self.command_id.is_nil()
            || self.session_id.is_nil()
            || self.expected_revision == 0
            || self.target_digest == [0; 32]
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// The outcome of a decision command. Every variant carries the current
/// interaction so callers project authoritative snapshots, never cached
/// guesses.
#[derive(Clone, Debug)]
pub(crate) enum ResolveOutcome {
    Resolved {
        interaction: floe_conversation::ConversationInteraction,
    },
    /// Owner work is durably claimed but not yet proven complete. Reconcile
    /// explicitly; never retry blindly with a new command.
    Resolving {
        interaction: floe_conversation::ConversationInteraction,
    },
    Denied {
        interaction: floe_conversation::ConversationInteraction,
    },
    Cancelled {
        interaction: floe_conversation::ConversationInteraction,
    },
    Superseded {
        interaction: floe_conversation::ConversationInteraction,
        reason: DriftReason,
        replacement_id: Option<Uuid>,
    },
    Expired {
        interaction: floe_conversation::ConversationInteraction,
    },
    Stale {
        interaction: floe_conversation::ConversationInteraction,
    },
    Terminal {
        interaction: floe_conversation::ConversationInteraction,
    },
    WrongDevice {
        interaction: floe_conversation::ConversationInteraction,
    },
}

impl ResolveOutcome {
    pub fn interaction(&self) -> &floe_conversation::ConversationInteraction {
        match self {
            Self::Resolved { interaction }
            | Self::Resolving { interaction }
            | Self::Denied { interaction }
            | Self::Cancelled { interaction }
            | Self::Superseded { interaction, .. }
            | Self::Expired { interaction }
            | Self::Stale { interaction }
            | Self::Terminal { interaction }
            | Self::WrongDevice { interaction } => interaction,
        }
    }
}

/// An explicit reconciliation command: stable identity plus Session/revision
/// CAS. Refresh never carries authority values and never enables a grant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RefreshInteractionCommand {
    pub interaction_id: Uuid,
    pub command_id: Uuid,
    pub session_id: Uuid,
    pub expected_revision: u64,
}

impl RefreshInteractionCommand {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.interaction_id.is_nil()
            || self.command_id.is_nil()
            || self.session_id.is_nil()
            || self.expected_revision == 0
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) enum RefreshOutcome {
    Resolved {
        interaction: floe_conversation::ConversationInteraction,
    },
    StillPending {
        interaction: floe_conversation::ConversationInteraction,
    },
    Superseded {
        interaction: floe_conversation::ConversationInteraction,
        reason: DriftReason,
        replacement_id: Option<Uuid>,
    },
    Terminal {
        interaction: floe_conversation::ConversationInteraction,
    },
    Expired {
        interaction: floe_conversation::ConversationInteraction,
    },
    Stale {
        interaction: floe_conversation::ConversationInteraction,
    },
    WrongDevice {
        interaction: floe_conversation::ConversationInteraction,
    },
}

impl RefreshOutcome {
    pub fn interaction(&self) -> &floe_conversation::ConversationInteraction {
        match self {
            Self::Resolved { interaction }
            | Self::StillPending { interaction }
            | Self::Superseded { interaction, .. }
            | Self::Terminal { interaction }
            | Self::Expired { interaction }
            | Self::Stale { interaction }
            | Self::WrongDevice { interaction } => interaction,
        }
    }
}

/// Record the person's decision and drive an inline approval through the
/// canonical owner operation.
///
/// Deny mutates no owner state. Dismiss cancels a Pending or Resolving
/// card. Approve on an inline target claims Resolving durably, compares
/// fresh owner truth with the reviewed target, mutates only on an exact
/// precondition match, and records the semantic resolution. A rejoined
/// command reconciles the already claimed operation instead of mutating
/// again.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve_interaction<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    reader: &dyn ObserveStateReader,
    mutation: &dyn InlineOwnerMutation,
    caller: &CallerContext,
    command: ResolveInteractionCommand,
    cancellation: &floe_execution::Cancellation,
    now_unix_ms: i64,
) -> Result<ResolveOutcome, AgentFailure>
where
    Runs: floe_conversation::ConversationRepository + ?Sized,
    Interactions: floe_conversation::InteractionRepository,
{
    command.validate()?;
    if now_unix_ms < 0 {
        return Err(AgentFailure::InvalidInput);
    }
    let person_id = caller_person(caller)?;
    let principal = caller.person_id().to_string();
    let mut current =
        load_owned(interactions, &principal, person_id, command.interaction_id).await?;
    if current.session_id != command.session_id {
        return Err(AgentFailure::NotFound);
    }
    if device_mismatch(&current.target, caller.device_id()) {
        return Ok(ResolveOutcome::WrongDevice {
            interaction: current,
        });
    }
    if current.projects_expired_at(now_unix_ms) {
        let expired =
            expire_owned(interactions, person_id, command.interaction_id, now_unix_ms).await?;
        return Ok(ResolveOutcome::Expired {
            interaction: expired,
        });
    }
    if command.target_digest != current.target_digest {
        return Err(AgentFailure::InvalidInput);
    }
    if matches!(
        command.kind,
        floe_conversation::InteractionDecisionKind::Approve
    ) && matches!(
        current.target,
        floe_conversation::ReviewedTarget::NavigationOnly(_)
    ) {
        return Err(AgentFailure::InvalidInput);
    }
    let admission = floe_conversation::decide_interaction(
        interactions,
        floe_conversation::DecideInteractionCommand {
            command_id: command.command_id,
            interaction_id: command.interaction_id,
            principal: principal.clone(),
            expected_revision: command.expected_revision,
            kind: command.kind,
            target_digest: command.target_digest,
        },
        now_unix_ms,
    )
    .await;
    let (admitted, rejoined) = match admission {
        Ok(floe_conversation::DecisionAdmission::Applied(current)) => (current, false),
        Ok(floe_conversation::DecisionAdmission::Rejoined(current)) => (current, true),
        Err(AgentFailure::Conflict) => {
            return stale_or_terminal(
                interactions,
                &principal,
                command.interaction_id,
                command.expected_revision,
            )
            .await
            .map(|interaction| match interaction {
                StaleOrTerminal::Stale(inner) => ResolveOutcome::Stale { interaction: inner },
                StaleOrTerminal::Terminal(inner) => ResolveOutcome::Terminal { interaction: inner },
            });
        }
        Err(failure) => return Err(failure),
    };
    current = admitted;
    match current.state.clone() {
        floe_conversation::InteractionState::Denied { .. } => Ok(ResolveOutcome::Denied {
            interaction: current,
        }),
        floe_conversation::InteractionState::Cancelled { .. } => Ok(ResolveOutcome::Cancelled {
            interaction: current,
        }),
        // An identical retry rejoins the recorded decision on its settled
        // state: resolving again would repeat owner work.
        floe_conversation::InteractionState::Resolved { .. } => Ok(ResolveOutcome::Resolved {
            interaction: current,
        }),
        floe_conversation::InteractionState::Superseded { .. }
        | floe_conversation::InteractionState::Expired => Ok(ResolveOutcome::Terminal {
            interaction: current,
        }),
        floe_conversation::InteractionState::Resolving {
            decision_id,
            owner_operation_id,
        } => {
            let floe_conversation::ReviewedTarget::InlineObserve(target) = current.target.clone()
            else {
                return Err(AgentFailure::StorageUnavailable);
            };
            if rejoined {
                reconcile_resolving(
                    runs,
                    interactions,
                    reader,
                    mutation,
                    caller,
                    person_id,
                    &principal,
                    &current,
                    &target,
                    decision_id,
                    owner_operation_id,
                    cancellation,
                    now_unix_ms,
                )
                .await
            } else {
                drive_fresh_approval(
                    runs,
                    interactions,
                    reader,
                    mutation,
                    caller,
                    person_id,
                    &principal,
                    &current,
                    &target,
                    decision_id,
                    owner_operation_id,
                    cancellation,
                    now_unix_ms,
                )
                .await
            }
        }
        _ => Err(AgentFailure::StorageUnavailable),
    }
}

/// Reconcile one interaction without enabling any grant.
///
/// A Pending card whose requirement is already satisfied resolves through
/// the person's explicit refresh. A stale review is replaced with a freshly
/// captured card. A Resolving card reconciles its claimed owner operation
/// by stable identity and current owner truth.
pub(crate) async fn refresh_interaction<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    reader: &dyn ObserveStateReader,
    mutation: &dyn InlineOwnerMutation,
    caller: &CallerContext,
    command: RefreshInteractionCommand,
    cancellation: &floe_execution::Cancellation,
    now_unix_ms: i64,
) -> Result<RefreshOutcome, AgentFailure>
where
    Runs: floe_conversation::ConversationRepository + ?Sized,
    Interactions: floe_conversation::InteractionRepository,
{
    command.validate()?;
    if now_unix_ms < 0 {
        return Err(AgentFailure::InvalidInput);
    }
    let person_id = caller_person(caller)?;
    let principal = caller.person_id().to_string();
    let current = load_owned(interactions, &principal, person_id, command.interaction_id).await?;
    if current.session_id != command.session_id {
        return Err(AgentFailure::NotFound);
    }
    if device_mismatch(&current.target, caller.device_id()) {
        return Ok(RefreshOutcome::WrongDevice {
            interaction: current,
        });
    }
    if current.projects_expired_at(now_unix_ms) {
        let expired =
            expire_owned(interactions, person_id, command.interaction_id, now_unix_ms).await?;
        return Ok(RefreshOutcome::Expired {
            interaction: expired,
        });
    }
    if current.revision != command.expected_revision {
        return Ok(RefreshOutcome::Stale {
            interaction: current,
        });
    }
    match current.state.clone() {
        floe_conversation::InteractionState::Pending => match current.target.clone() {
            floe_conversation::ReviewedTarget::InlineObserve(target) => {
                let live = reader
                    .read_live_inline(&target, person_id, caller.device_id(), cancellation)
                    .await?;
                match compare_reviewed_live(&target, &live) {
                    Ok(ReviewMatch::Satisfied) => {
                        let resolved = settle_satisfied(
                            interactions,
                            person_id,
                            &principal,
                            &current,
                            command.command_id,
                            now_unix_ms,
                        )
                        .await?;
                        Ok(RefreshOutcome::Resolved {
                            interaction: resolved,
                        })
                    }
                    Ok(ReviewMatch::Precondition) => Ok(RefreshOutcome::StillPending {
                        interaction: current,
                    }),
                    Err(reason) => {
                        let (superseded, replacement) = supersede_with_replacement(
                            runs,
                            interactions,
                            person_id,
                            &principal,
                            &current,
                            Some(&live),
                            caller.device_id(),
                            now_unix_ms,
                        )
                        .await?;
                        Ok(RefreshOutcome::Superseded {
                            interaction: superseded,
                            reason,
                            replacement_id: replacement,
                        })
                    }
                }
            }
            floe_conversation::ReviewedTarget::NavigationOnly(target) => {
                if !reader
                    .navigation_connection_usable(&target, person_id)
                    .await?
                {
                    let (superseded, _) = supersede_with_replacement(
                        runs,
                        interactions,
                        person_id,
                        &principal,
                        &current,
                        None,
                        caller.device_id(),
                        now_unix_ms,
                    )
                    .await?;
                    return Ok(RefreshOutcome::Superseded {
                        interaction: superseded,
                        reason: DriftReason::ConnectionUnusable,
                        replacement_id: None,
                    });
                }
                if reader
                    .navigation_satisfied(&target, person_id, caller.device_id(), cancellation)
                    .await?
                {
                    let resolved = settle_satisfied(
                        interactions,
                        person_id,
                        &principal,
                        &current,
                        command.command_id,
                        now_unix_ms,
                    )
                    .await?;
                    Ok(RefreshOutcome::Resolved {
                        interaction: resolved,
                    })
                } else {
                    Ok(RefreshOutcome::StillPending {
                        interaction: current,
                    })
                }
            }
        },
        floe_conversation::InteractionState::Resolving {
            decision_id,
            owner_operation_id,
        } => {
            let floe_conversation::ReviewedTarget::InlineObserve(target) = current.target.clone()
            else {
                return Ok(RefreshOutcome::Terminal {
                    interaction: current,
                });
            };
            let outcome = reconcile_resolving(
                runs,
                interactions,
                reader,
                mutation,
                caller,
                person_id,
                &principal,
                &current,
                &target,
                decision_id,
                owner_operation_id,
                cancellation,
                now_unix_ms,
            )
            .await?;
            Ok(match outcome {
                ResolveOutcome::Resolved { interaction } => {
                    RefreshOutcome::Resolved { interaction }
                }
                ResolveOutcome::Resolving { interaction } => {
                    RefreshOutcome::Terminal { interaction }
                }
                ResolveOutcome::Superseded {
                    interaction,
                    reason,
                    replacement_id,
                } => RefreshOutcome::Superseded {
                    interaction,
                    reason,
                    replacement_id,
                },
                other => RefreshOutcome::Terminal {
                    interaction: other.interaction().clone(),
                },
            })
        }
        _ => Ok(RefreshOutcome::Terminal {
            interaction: current,
        }),
    }
}

/// Fresh approval: the live state must match the reviewed precondition
/// exactly, including still-absent grants. Anything else supersedes the
/// review; a concurrent out-of-band enable is a conflict for a fresh
/// Allow, settled only through explicit reconciliation.
#[allow(clippy::too_many_arguments)]
async fn drive_fresh_approval<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    reader: &dyn ObserveStateReader,
    mutation: &dyn InlineOwnerMutation,
    caller: &CallerContext,
    person_id: PersonId,
    principal: &str,
    current: &floe_conversation::ConversationInteraction,
    target: &floe_conversation::InlineObserveTarget,
    decision_id: Uuid,
    owner_operation_id: Uuid,
    cancellation: &floe_execution::Cancellation,
    now_unix_ms: i64,
) -> Result<ResolveOutcome, AgentFailure>
where
    Runs: floe_conversation::ConversationRepository + ?Sized,
    Interactions: floe_conversation::InteractionRepository,
{
    let live = reader
        .read_live_inline(target, person_id, caller.device_id(), cancellation)
        .await?;
    match compare_reviewed_live(target, &live) {
        Ok(ReviewMatch::Precondition) => {
            let mutation_result = mutation
                .enable_reviewed(
                    target,
                    person_id,
                    caller.device_id(),
                    owner_operation_id,
                    cancellation,
                )
                .await;
            match mutation_result {
                Ok(()) => {
                    let post = reader
                        .read_live_inline(target, person_id, caller.device_id(), cancellation)
                        .await?;
                    match compare_reviewed_live(target, &post) {
                        Ok(ReviewMatch::Satisfied) => {
                            let resolved = record_resolution_owned(
                                interactions,
                                person_id,
                                current,
                                decision_id,
                                owner_operation_id,
                                now_unix_ms,
                            )
                            .await?;
                            Ok(ResolveOutcome::Resolved {
                                interaction: resolved,
                            })
                        }
                        _ => Ok(ResolveOutcome::Resolving {
                            interaction: current.clone(),
                        }),
                    }
                }
                Err(failure) => {
                    reconcile_after_mutation_outcome(
                        runs,
                        interactions,
                        reader,
                        principal,
                        person_id,
                        current,
                        target,
                        decision_id,
                        owner_operation_id,
                        caller.device_id(),
                        failure,
                        cancellation,
                        now_unix_ms,
                    )
                    .await
                }
            }
        }
        // A fresh Allow never adopts an out-of-band enable: the card no
        // longer describes reality, so it is replaced, not resolved.
        Ok(ReviewMatch::Satisfied) => {
            let (superseded, replacement) = supersede_with_replacement(
                runs,
                interactions,
                person_id,
                principal,
                current,
                Some(&live),
                caller.device_id(),
                now_unix_ms,
            )
            .await?;
            Ok(ResolveOutcome::Superseded {
                interaction: superseded,
                reason: DriftReason::ConcurrentEnablement,
                replacement_id: replacement,
            })
        }
        Err(reason) => {
            let (superseded, replacement) = supersede_with_replacement(
                runs,
                interactions,
                person_id,
                principal,
                current,
                Some(&live),
                caller.device_id(),
                now_unix_ms,
            )
            .await?;
            Ok(ResolveOutcome::Superseded {
                interaction: superseded,
                reason,
                replacement_id: replacement,
            })
        }
    }
}

/// Reconciliation of a claimed owner operation: settle satisfaction, run the
/// still-pending mutation, or supersede drift. Every branch re-reads live
/// first, so a retried command can neither repeat a committed mutation nor
/// widen a changed review.
#[allow(clippy::too_many_arguments)]
async fn reconcile_resolving<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    reader: &dyn ObserveStateReader,
    mutation: &dyn InlineOwnerMutation,
    caller: &CallerContext,
    person_id: PersonId,
    principal: &str,
    current: &floe_conversation::ConversationInteraction,
    target: &floe_conversation::InlineObserveTarget,
    decision_id: Uuid,
    owner_operation_id: Uuid,
    cancellation: &floe_execution::Cancellation,
    now_unix_ms: i64,
) -> Result<ResolveOutcome, AgentFailure>
where
    Runs: floe_conversation::ConversationRepository + ?Sized,
    Interactions: floe_conversation::InteractionRepository,
{
    let live = reader
        .read_live_inline(target, person_id, caller.device_id(), cancellation)
        .await?;
    match compare_reviewed_live(target, &live) {
        Ok(ReviewMatch::Satisfied) => {
            let resolved = record_resolution_owned(
                interactions,
                person_id,
                current,
                decision_id,
                owner_operation_id,
                now_unix_ms,
            )
            .await?;
            Ok(ResolveOutcome::Resolved {
                interaction: resolved,
            })
        }
        Ok(ReviewMatch::Precondition) => {
            match mutation
                .enable_reviewed(
                    target,
                    person_id,
                    caller.device_id(),
                    owner_operation_id,
                    cancellation,
                )
                .await
            {
                Ok(()) => {
                    let post = reader
                        .read_live_inline(target, person_id, caller.device_id(), cancellation)
                        .await?;
                    match compare_reviewed_live(target, &post) {
                        Ok(ReviewMatch::Satisfied) => {
                            let resolved = record_resolution_owned(
                                interactions,
                                person_id,
                                current,
                                decision_id,
                                owner_operation_id,
                                now_unix_ms,
                            )
                            .await?;
                            Ok(ResolveOutcome::Resolved {
                                interaction: resolved,
                            })
                        }
                        _ => Ok(ResolveOutcome::Resolving {
                            interaction: current.clone(),
                        }),
                    }
                }
                Err(failure) => {
                    reconcile_after_mutation_outcome(
                        runs,
                        interactions,
                        reader,
                        principal,
                        person_id,
                        current,
                        target,
                        decision_id,
                        owner_operation_id,
                        caller.device_id(),
                        failure,
                        cancellation,
                        now_unix_ms,
                    )
                    .await
                }
            }
        }
        Err(reason) => {
            let (superseded, replacement) = supersede_with_replacement(
                runs,
                interactions,
                person_id,
                principal,
                current,
                Some(&live),
                caller.device_id(),
                now_unix_ms,
            )
            .await?;
            Ok(ResolveOutcome::Superseded {
                interaction: superseded,
                reason,
                replacement_id: replacement,
            })
        }
    }
}

/// Settle a mutation outcome that did not prove success: response loss may
/// hide a commit, and the owner op may have refused on fresher evidence.
/// Re-read once and classify; never retry blindly here.
#[allow(clippy::too_many_arguments)]
async fn reconcile_after_mutation_outcome<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    reader: &dyn ObserveStateReader,
    principal: &str,
    person_id: PersonId,
    current: &floe_conversation::ConversationInteraction,
    target: &floe_conversation::InlineObserveTarget,
    decision_id: Uuid,
    owner_operation_id: Uuid,
    device_id: &str,
    failure: AgentFailure,
    cancellation: &floe_execution::Cancellation,
    now_unix_ms: i64,
) -> Result<ResolveOutcome, AgentFailure>
where
    Runs: floe_conversation::ConversationRepository + ?Sized,
    Interactions: floe_conversation::InteractionRepository,
{
    match failure {
        AgentFailure::AccessReviewRequired | AgentFailure::StaleContext => {
            let live = reader
                .read_live_inline(target, person_id, device_id, cancellation)
                .await?;
            match compare_reviewed_live(target, &live) {
                Ok(ReviewMatch::Satisfied) => {
                    let resolved = record_resolution_owned(
                        interactions,
                        person_id,
                        current,
                        decision_id,
                        owner_operation_id,
                        now_unix_ms,
                    )
                    .await?;
                    Ok(ResolveOutcome::Resolved {
                        interaction: resolved,
                    })
                }
                Ok(ReviewMatch::Precondition) => Ok(ResolveOutcome::Resolving {
                    interaction: current.clone(),
                }),
                Err(reason) => {
                    let (superseded, replacement) = supersede_with_replacement(
                        runs,
                        interactions,
                        person_id,
                        principal,
                        current,
                        Some(&live),
                        device_id,
                        now_unix_ms,
                    )
                    .await?;
                    Ok(ResolveOutcome::Superseded {
                        interaction: superseded,
                        reason,
                        replacement_id: replacement,
                    })
                }
            }
        }
        _ => Ok(ResolveOutcome::Resolving {
            interaction: current.clone(),
        }),
    }
}

/// Settle an already satisfied requirement through explicit refresh: record
/// the person's acceptance of the reviewed outcome, then resolve without
/// any owner mutation.
async fn settle_satisfied<Interactions>(
    interactions: &Interactions,
    person_id: PersonId,
    principal: &str,
    current: &floe_conversation::ConversationInteraction,
    command_id: Uuid,
    now_unix_ms: i64,
) -> Result<floe_conversation::ConversationInteraction, AgentFailure>
where
    Interactions: floe_conversation::InteractionRepository,
{
    let admitted = floe_conversation::decide_interaction(
        interactions,
        floe_conversation::DecideInteractionCommand {
            command_id,
            interaction_id: current.id,
            principal: principal.to_owned(),
            expected_revision: current.revision,
            kind: floe_conversation::InteractionDecisionKind::Approve,
            target_digest: current.target_digest,
        },
        now_unix_ms,
    )
    .await?;
    let admitted = match admitted {
        floe_conversation::DecisionAdmission::Applied(current) => current,
        floe_conversation::DecisionAdmission::Rejoined(current) => current,
    };
    let floe_conversation::InteractionState::Resolving {
        decision_id,
        owner_operation_id,
    } = admitted.state.clone()
    else {
        return Err(AgentFailure::Conflict);
    };
    let resolution = floe_conversation::InteractionResolution {
        interaction_id: admitted.id,
        person_id,
        expected_revision: admitted.revision,
        decision_id,
        owner_operation_id,
        resolved_at_unix_ms: now_unix_ms,
    };
    floe_conversation::resolve_interaction(interactions, resolution).await
}

/// Supersede a stale review and, when the live read supports it, publish
/// the replacement card under the same admitted origin. The replacement
/// binds live-at-read owner truth; its own decision re-verifies.
#[allow(clippy::too_many_arguments)]
async fn supersede_with_replacement<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    person_id: PersonId,
    principal: &str,
    current: &floe_conversation::ConversationInteraction,
    live: Option<&LiveInlineState>,
    device_id: &str,
    now_unix_ms: i64,
) -> Result<(floe_conversation::ConversationInteraction, Option<Uuid>), AgentFailure>
where
    Runs: floe_conversation::ConversationRepository + ?Sized,
    Interactions: floe_conversation::InteractionRepository,
{
    let replacement = match (&current.target, live) {
        (floe_conversation::ReviewedTarget::InlineObserve(target), Some(live)) => {
            publish_replacement(
                runs,
                interactions,
                principal,
                current,
                target,
                live,
                device_id,
                now_unix_ms,
            )
            .await
            .ok()
        }
        _ => None,
    };
    let superseded = floe_conversation::supersede_interaction(
        interactions,
        floe_conversation::SupersedeInteraction {
            interaction_id: current.id,
            person_id,
            expected_revision: current.revision,
            superseded_by: replacement,
        },
    )
    .await?;
    Ok((superseded, replacement))
}

/// Publish the fresh review for drifted live state: same requirement, a
/// target rebuilt from live-at-read owner truth.
#[allow(clippy::too_many_arguments)]
async fn publish_replacement<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    principal: &str,
    current: &floe_conversation::ConversationInteraction,
    target: &floe_conversation::InlineObserveTarget,
    live: &LiveInlineState,
    device_id: &str,
    now_unix_ms: i64,
) -> Result<Uuid, AgentFailure>
where
    Runs: floe_conversation::ConversationRepository + ?Sized,
    Interactions: floe_conversation::InteractionRepository,
{
    let mut members: Vec<floe_conversation::ReviewedBundleMember> =
        Vec::with_capacity(live.members.len());
    for current_member in &live.members {
        let [grant] = current_member.live_grants.as_slice() else {
            if current_member.live_grants.is_empty() {
                members.push(reviewed_member(
                    current_member,
                    floe_conversation::ExpectedGrantState::Absent,
                ));
                continue;
            }
            return Err(AgentFailure::Conflict);
        };
        members.push(reviewed_member(
            current_member,
            floe_conversation::ExpectedGrantState::Active {
                grant_id: grant.id.as_uuid(),
                authority_incarnation: grant.authority.incarnation(),
                authority_epoch: grant.authority.access_epoch().get(),
            },
        ));
    }
    members.sort_by(|left, right| {
        left.member_id
            .cmp(&right.member_id)
            .then_with(|| left.resource.cmp(&right.resource))
    });
    if members.is_empty()
        || members.len() > floe_conversation::MAX_TARGET_BUNDLE_MEMBERS
        || members.windows(2).any(|pair| {
            pair[0].member_id == pair[1].member_id && pair[0].resource == pair[1].resource
        })
    {
        return Err(AgentFailure::InvalidInput);
    }
    let replacement =
        floe_conversation::ReviewedTarget::InlineObserve(floe_conversation::InlineObserveTarget {
            connection_id: target.connection_id.clone(),
            device_id: Some(device_id.to_owned()),
            source_id: target.source_id.clone(),
            connector_id: target.connector_id.clone(),
            consumer: target.consumer.clone(),
            purpose: target.purpose.clone(),
            connection_revision: live.connection_revision,
            reviewed_producer_fingerprint: live.producer_fingerprint.clone(),
            reviewed_native_subject: live.native_subject.clone(),
            members,
        });
    replacement
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    if floe_conversation::canonical_target_digest(&replacement)? == current.target_digest {
        return Err(AgentFailure::Conflict);
    }
    let admission = floe_conversation::publish_interaction(
        runs,
        interactions,
        floe_conversation::PublishInteractionRequest {
            principal: principal.to_owned(),
            session_id: current.session_id,
            origin_run_id: current.origin_run_id,
            origin: current.origin.clone(),
            kind: current.kind,
            requirement: current.requirement.clone(),
            target: replacement,
        },
        now_unix_ms,
    )
    .await?;
    Ok(match admission {
        floe_conversation::PublishAdmission::Created(record)
        | floe_conversation::PublishAdmission::Existing(record) => record.id,
    })
}

fn reviewed_member(
    current: &LiveMember,
    expected_grant: floe_conversation::ExpectedGrantState,
) -> floe_conversation::ReviewedBundleMember {
    floe_conversation::ReviewedBundleMember {
        member_id: current.member_id.clone(),
        resource: current.resource.clone(),
        source_revision: current.source_revision.map(|authority| {
            floe_conversation::AuthorityRevision {
                incarnation: authority.incarnation(),
                epoch: authority.epoch().get(),
            }
        }),
        expected_grant,
        policy_authority: current.policy_authority.map(|authority| {
            floe_conversation::AuthorityRevision {
                incarnation: authority.incarnation(),
                epoch: authority.epoch().get(),
            }
        }),
    }
}

fn caller_person(caller: &CallerContext) -> Result<PersonId, AgentFailure> {
    PersonId::from_uuid(caller.person_id()).ok_or(AgentFailure::InvalidInput)
}

fn device_mismatch(target: &floe_conversation::ReviewedTarget, device_id: &str) -> bool {
    match target {
        floe_conversation::ReviewedTarget::InlineObserve(target) => {
            target.device_id.as_deref() != Some(device_id)
        }
        floe_conversation::ReviewedTarget::NavigationOnly(_) => false,
    }
}

async fn load_owned<Interactions>(
    interactions: &Interactions,
    principal: &str,
    person_id: PersonId,
    interaction_id: Uuid,
) -> Result<floe_conversation::ConversationInteraction, AgentFailure>
where
    Interactions: floe_conversation::InteractionRepository,
{
    let _ = person_id;
    floe_conversation::load_interaction(interactions, principal, interaction_id).await
}

async fn expire_owned<Interactions>(
    interactions: &Interactions,
    person_id: PersonId,
    interaction_id: Uuid,
    now_unix_ms: i64,
) -> Result<floe_conversation::ConversationInteraction, AgentFailure>
where
    Interactions: floe_conversation::InteractionRepository,
{
    let outcome = floe_conversation::expire_interaction(
        interactions,
        floe_conversation::ExpireInteraction {
            interaction_id,
            person_id,
            now_unix_ms,
        },
    )
    .await?;
    Ok(match outcome {
        floe_conversation::ExpireOutcome::Expired(current)
        | floe_conversation::ExpireOutcome::AlreadyTerminal(current)
        | floe_conversation::ExpireOutcome::NotExpired(current) => current,
    })
}

enum StaleOrTerminal {
    Stale(floe_conversation::ConversationInteraction),
    Terminal(floe_conversation::ConversationInteraction),
}

async fn stale_or_terminal<Interactions>(
    interactions: &Interactions,
    principal: &str,
    interaction_id: Uuid,
    expected_revision: u64,
) -> Result<StaleOrTerminal, AgentFailure>
where
    Interactions: floe_conversation::InteractionRepository,
{
    let current =
        floe_conversation::load_interaction(interactions, principal, interaction_id).await?;
    if current.revision != expected_revision {
        return Ok(StaleOrTerminal::Stale(current));
    }
    if current.state.is_terminal() {
        return Ok(StaleOrTerminal::Terminal(current));
    }
    Err(AgentFailure::Conflict)
}

async fn record_resolution_owned<Interactions>(
    interactions: &Interactions,
    person_id: PersonId,
    current: &floe_conversation::ConversationInteraction,
    decision_id: Uuid,
    owner_operation_id: Uuid,
    now_unix_ms: i64,
) -> Result<floe_conversation::ConversationInteraction, AgentFailure>
where
    Interactions: floe_conversation::InteractionRepository,
{
    let resolution = floe_conversation::InteractionResolution {
        interaction_id: current.id,
        person_id,
        expected_revision: current.revision,
        decision_id,
        owner_operation_id,
        resolved_at_unix_ms: now_unix_ms,
    };
    match floe_conversation::resolve_interaction(interactions, resolution).await {
        Ok(resolved) => Ok(resolved),
        Err(AgentFailure::Conflict) => Err(AgentFailure::Conflict),
        Err(failure) => Err(failure),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authority() -> SourceAuthority {
        SourceAuthority::from_parts(Uuid::new_v4(), NonZeroU64::new(3).unwrap()).unwrap()
    }

    fn policy() -> ConsumerPolicyAuthority {
        ConsumerPolicyAuthority::from_parts(Uuid::new_v4(), NonZeroU64::new(5).unwrap()).unwrap()
    }

    fn grant() -> (GrantId, GrantAuthority) {
        (
            GrantId::from_uuid(Uuid::new_v4()).unwrap(),
            GrantAuthority::from_parts(Uuid::new_v4(), NonZeroU64::new(7).unwrap()).unwrap(),
        )
    }

    fn reviewed_member(
        expected: floe_conversation::ExpectedGrantState,
        source: SourceAuthority,
        policy: Option<ConsumerPolicyAuthority>,
    ) -> floe_conversation::ReviewedBundleMember {
        floe_conversation::ReviewedBundleMember {
            member_id: "calendar.timeline".into(),
            resource: "personal".into(),
            source_revision: Some(floe_conversation::AuthorityRevision {
                incarnation: source.incarnation(),
                epoch: source.epoch().get(),
            }),
            expected_grant: expected,
            policy_authority: policy.map(|authority| floe_conversation::AuthorityRevision {
                incarnation: authority.incarnation(),
                epoch: authority.epoch().get(),
            }),
        }
    }

    fn reviewed_target(
        members: Vec<floe_conversation::ReviewedBundleMember>,
    ) -> floe_conversation::InlineObserveTarget {
        floe_conversation::InlineObserveTarget {
            connection_id: "connection".into(),
            device_id: Some("device".into()),
            source_id: "floe.source.calendar".into(),
            connector_id: Some("calendar.macos".into()),
            consumer: "floe.builtin.schedule".into(),
            purpose: "scheduling".into(),
            connection_revision: Some(9),
            reviewed_producer_fingerprint: None,
            reviewed_native_subject: Some("subject".into()),
            members,
        }
    }

    fn live_member(
        grants: Vec<(GrantId, GrantAuthority)>,
        source: SourceAuthority,
        policy: Option<ConsumerPolicyAuthority>,
    ) -> LiveMember {
        LiveMember {
            member_id: "calendar.timeline".into(),
            resource: "personal".into(),
            source_revision: Some(source),
            live_grants: grants
                .into_iter()
                .map(|(id, authority)| LiveGrant { id, authority })
                .collect(),
            policy_authority: policy,
        }
    }

    fn live_state(members: Vec<LiveMember>) -> LiveInlineState {
        LiveInlineState {
            members,
            connection_revision: Some(9),
            producer_fingerprint: None,
            native_subject: Some("subject".into()),
            connection_usable: true,
        }
    }

    #[test]
    fn precondition_requires_exact_identity_and_still_absent_grants() {
        let source = authority();
        let target = reviewed_target(vec![reviewed_member(
            floe_conversation::ExpectedGrantState::Absent,
            source,
            None,
        )]);
        let live = live_state(vec![live_member(vec![], source, None)]);
        assert_eq!(
            compare_reviewed_live(&target, &live),
            Ok(ReviewMatch::Precondition)
        );
    }

    #[test]
    fn satisfied_needs_no_mutation_but_is_not_a_fresh_approval() {
        let source = authority();
        let (id, grant_authority) = grant();
        let target = reviewed_target(vec![reviewed_member(
            floe_conversation::ExpectedGrantState::Absent,
            source,
            None,
        )]);
        let live = live_state(vec![live_member(vec![(id, grant_authority)], source, None)]);
        assert_eq!(
            compare_reviewed_live(&target, &live),
            Ok(ReviewMatch::Satisfied)
        );
    }

    #[test]
    fn reviewed_live_grant_must_be_identical() {
        let source = authority();
        let (id, grant_authority) = grant();
        let target = reviewed_target(vec![reviewed_member(
            floe_conversation::ExpectedGrantState::Active {
                grant_id: id.as_uuid(),
                authority_incarnation: grant_authority.incarnation(),
                authority_epoch: grant_authority.access_epoch().get(),
            },
            source,
            None,
        )]);
        let same = live_state(vec![live_member(vec![(id, grant_authority)], source, None)]);
        assert_eq!(
            compare_reviewed_live(&target, &same),
            Ok(ReviewMatch::Satisfied)
        );
        let advanced = grant_authority.advance().unwrap();
        let rotated = live_state(vec![live_member(vec![(id, advanced)], source, None)]);
        assert!(matches!(
            compare_reviewed_live(&target, &rotated),
            Err(DriftReason::GrantState { .. })
        ));
        let vanished = live_state(vec![live_member(vec![], source, None)]);
        assert!(matches!(
            compare_reviewed_live(&target, &vanished),
            Err(DriftReason::GrantState { .. })
        ));
        let (other_id, other_authority) = grant();
        let duplicated = live_state(vec![live_member(
            vec![(id, grant_authority), (other_id, other_authority)],
            source,
            None,
        )]);
        assert!(matches!(
            compare_reviewed_live(&target, &duplicated),
            Err(DriftReason::GrantState { .. })
        ));
    }

    #[test]
    fn partial_out_of_band_enablement_is_drift() {
        let source = authority();
        let (id, grant_authority) = grant();
        let target = reviewed_target(vec![
            reviewed_member(floe_conversation::ExpectedGrantState::Absent, source, None),
            floe_conversation::ReviewedBundleMember {
                member_id: "calendar.timeline".into(),
                resource: "work".into(),
                source_revision: Some(floe_conversation::AuthorityRevision {
                    incarnation: source.incarnation(),
                    epoch: source.epoch().get(),
                }),
                expected_grant: floe_conversation::ExpectedGrantState::Absent,
                policy_authority: None,
            },
        ]);
        let mut live = live_state(vec![
            live_member(vec![(id, grant_authority)], source, None),
            LiveMember {
                member_id: "calendar.timeline".into(),
                resource: "work".into(),
                source_revision: Some(source),
                live_grants: vec![],
                policy_authority: None,
            },
        ]);
        assert_eq!(
            compare_reviewed_live(&target, &live),
            Err(DriftReason::PartialMutation)
        );
        // A new canonical member outside the reviewed set invalidates too.
        live.members.push(LiveMember {
            member_id: "calendar.timeline".into(),
            resource: "family".into(),
            source_revision: Some(source),
            live_grants: vec![],
            policy_authority: None,
        });
        assert_eq!(
            compare_reviewed_live(&target, &live),
            Err(DriftReason::MemberSet)
        );
    }

    #[test]
    fn identity_drift_invalidates_before_grants() {
        let source = authority();
        let policy_value = policy();
        let target = reviewed_target(vec![reviewed_member(
            floe_conversation::ExpectedGrantState::Absent,
            source,
            Some(policy_value),
        )]);
        let base = live_state(vec![live_member(vec![], source, Some(policy_value))]);
        let mut unusable = base.clone();
        unusable.connection_usable = false;
        assert_eq!(
            compare_reviewed_live(&target, &unusable),
            Err(DriftReason::ConnectionUnusable)
        );
        let mut revision = base.clone();
        revision.connection_revision = Some(10);
        assert_eq!(
            compare_reviewed_live(&target, &revision),
            Err(DriftReason::ConnectionRevision)
        );
        let mut subject = base.clone();
        subject.native_subject = Some("other".into());
        assert_eq!(
            compare_reviewed_live(&target, &subject),
            Err(DriftReason::NativeSubject)
        );
        let mut source_live = base.clone();
        source_live.members[0].source_revision = Some(authority());
        assert!(matches!(
            compare_reviewed_live(&target, &source_live),
            Err(DriftReason::SourceRevision { .. })
        ));
        let mut policy_live = base;
        policy_live.members[0].policy_authority = Some(policy());
        assert!(matches!(
            compare_reviewed_live(&target, &policy_live),
            Err(DriftReason::PolicyAuthority { .. })
        ));
    }
}
