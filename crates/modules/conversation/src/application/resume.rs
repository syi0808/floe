//! The best-effort gate before a linked fresh Run admission.
//!
//! Whether an origin currently wants a child is a pure judgment over the
//! origin receipt and its interaction group: Completed, with chain depth
//! left, every card terminal and at least one resolved. Admission itself
//! re-verifies all of this atomically (plus the Session revision and the
//! unique resume slot), so this gate only saves wasted admission attempts
//! and names honest suppression reasons; it never authorizes a child.

use crate::{
    ConversationInteraction, InteractionResumeRef, InteractionState, RunReceipt, RunState,
};

/// Why no linked child is currently wanted for an origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResumeSuppression {
    /// The origin never finished successfully; Working, Failed, Cancelled,
    /// TimedOut and Interrupted origins have no resume linkage.
    OriginNotCompleted,
    /// The origin finished but its chain already reached the automatic
    /// depth cap; only a fresh explicit turn continues from here.
    LineageExhausted,
    /// A card is still Pending or Resolving; automatic resume waits for
    /// the whole group to settle.
    GroupOpen,
    /// Every card settled with none resolved: deny-all and its cousins
    /// never start a child on their own.
    NothingResolved,
    /// The Session moved on since the origin finished. Automatic restart
    /// stays suppressed; an explicit Continue may still claim the slot at
    /// the current revision.
    NewerTurn,
}

/// The child linkage one origin currently admits, if any.
///
/// Reads only: the atomic admission re-verifies the origin, the group,
/// the revision and the slot before anything is created.
pub fn resume_gate(
    origin: &RunReceipt,
    group: &[ConversationInteraction],
) -> Result<InteractionResumeRef, ResumeSuppression> {
    if origin.state != RunState::Completed {
        return Err(ResumeSuppression::OriginNotCompleted);
    }
    let Some(link) = origin.resume() else {
        return Err(ResumeSuppression::LineageExhausted);
    };
    if group.is_empty()
        || !group
            .iter()
            .any(|entry| matches!(entry.state, InteractionState::Resolved { .. }))
    {
        return Err(ResumeSuppression::NothingResolved);
    }
    if group.iter().any(|entry| !entry.state.is_terminal()) {
        return Err(ResumeSuppression::GroupOpen);
    }
    Ok(link)
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_kernel::RunId;

    fn origin(state: RunState, lineage: u8) -> RunReceipt {
        crate::RunReceipt {
            run_id: RunId::new(),
            command_id: floe_kernel::CommandId::new(),
            session_id: uuid::Uuid::new_v4(),
            principal: "person".into(),
            request_digest: [7; 32],
            state,
            output: Some("done".into()),
            coverage: floe_agent_contract::DependencyCoverage::Independent,
            issue: None,
            session_revision: 2,
            aggregate_revision: 2,
            executor_generation: 1,
            continuation_of: None,
            continuation_executor_generation: None,
            continuation_level: 0,
            retry_of: None,
            resume_of: None,
            resume_lineage: lineage,
            profile: crate::ProfileSelection::Auto,
            attempt_refs: vec![],
            task_refs: vec![],
        }
    }

    fn interaction(state: InteractionState) -> ConversationInteraction {
        crate::ConversationInteraction {
            id: uuid::Uuid::new_v4(),
            person_id: floe_kernel::PersonId::new(),
            session_id: uuid::Uuid::new_v4(),
            origin_run_id: RunId::new(),
            origin_turn_id: uuid::Uuid::new_v4(),
            origin: crate::InteractionOrigin::Model {
                attempt_id: uuid::Uuid::new_v4(),
            },
            kind: floe_agent_contract::UserInteractionKind::ProcessingRecipient,
            requirement: crate::InteractionRequirement {
                kind: crate::InteractionRequirementKind::ApproveProcessingRecipient,
                source_id: "recipient".into(),
                connection_id: None,
                consumer: "conversation.root".into(),
                purpose: "everyday-assistance".into(),
                inline: true,
            },
            requirement_digest: [3; 32],
            target: crate::ReviewedTarget::RecipientConsent(crate::RecipientConsentTarget {
                recipient: "recipient".into(),
                profile_id: "profile".into(),
                purpose: "everyday-assistance".into(),
                consumer: "conversation.root".into(),
                input_data_classes: vec![],
                source_scopes: vec![],
                lineage: floe_agent_contract::RecipientLineage::try_new(
                    uuid::Uuid::new_v4(),
                    uuid::Uuid::new_v4(),
                )
                .unwrap(),
                device_id: "device".into(),
                projection_ref: uuid::Uuid::new_v4(),
                projection_revision: 1,
            }),
            target_digest: [5; 32],
            state,
            revision: 1,
            created_at_unix_ms: 1,
            expires_at_unix_ms: 1 + crate::INTERACTION_PENDING_LIFETIME_MS,
        }
    }

    #[test]
    fn gate_names_each_suppression_before_admitting() {
        let completed = origin(RunState::Completed, 0);
        let resolved = interaction(InteractionState::Resolved {
            receipt: crate::InteractionResolutionReceipt {
                decision_id: uuid::Uuid::new_v4(),
                owner_operation_id: uuid::Uuid::new_v4(),
                resolved_at_unix_ms: 2,
            },
        });
        let link = resume_gate(&completed, &[resolved.clone()]).unwrap();
        assert_eq!(link.origin_run_id, completed.run_id);
        assert_eq!(link.lineage, 1);

        assert_eq!(
            resume_gate(&origin(RunState::Working, 0), &[resolved.clone()]),
            Err(ResumeSuppression::OriginNotCompleted)
        );
        assert_eq!(
            resume_gate(
                &origin(RunState::Completed, crate::MAX_RESUME_LINEAGE),
                &[resolved]
            ),
            Err(ResumeSuppression::LineageExhausted)
        );
        assert_eq!(
            resume_gate(&completed, &[]),
            Err(ResumeSuppression::NothingResolved)
        );
        assert_eq!(
            resume_gate(
                &completed,
                &[interaction(InteractionState::Denied {
                    decision_id: uuid::Uuid::new_v4(),
                })],
            ),
            Err(ResumeSuppression::NothingResolved)
        );
        assert_eq!(
            resume_gate(
                &completed,
                &[
                    interaction(InteractionState::Resolved {
                        receipt: crate::InteractionResolutionReceipt {
                            decision_id: uuid::Uuid::new_v4(),
                            owner_operation_id: uuid::Uuid::new_v4(),
                            resolved_at_unix_ms: 2,
                        },
                    }),
                    interaction(InteractionState::Pending),
                ],
            ),
            Err(ResumeSuppression::GroupOpen)
        );

        // A mixed terminal group (one resolved, one denied) admits one
        // child: the denial stays unavailable, it never re-prompts.
        let mixed = resume_gate(
            &completed,
            &[
                interaction(InteractionState::Resolved {
                    receipt: crate::InteractionResolutionReceipt {
                        decision_id: uuid::Uuid::new_v4(),
                        owner_operation_id: uuid::Uuid::new_v4(),
                        resolved_at_unix_ms: 2,
                    },
                }),
                interaction(InteractionState::Denied {
                    decision_id: uuid::Uuid::new_v4(),
                }),
            ],
        )
        .unwrap();
        assert_eq!(mixed.origin_run_id, completed.run_id);
        assert_eq!(mixed.lineage, 1);
    }
}
