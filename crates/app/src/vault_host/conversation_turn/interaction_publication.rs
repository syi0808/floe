//! Trusted publication of owner-produced requirements.
//!
//! Context and the delegated host report recoverable source blockers as typed
//! outcomes, and the delegated model host reports a blocked dispatch the same
//! way. This boundary converts each owner-produced requirement into the
//! reviewed descriptor the person's decision binds, publishes it under the
//! admitted origin through Conversation, and returns safe interaction artifacts
//! with safe references only. Model-produced JSON — even with a matching
//! media type — never enters this path: only outcomes the trusted inner port
//! returned are published.

use floe_agent_contract::{
    AgentFailure, Artifact, ArtifactPart, DependencyCoverage, USER_INTERACTION_MEDIA_TYPE,
    UserInteractionKind, UserInteractionRef, UserInteractionStatus,
};
use floe_context_contract::{
    SourceAccessBlockers, SourceAccessRequirement, SourceAccessRequirementKind,
};
use floe_kernel::{PersonId, RunId};
use uuid::Uuid;

/// The generic source label a blocked result may name to the model.
///
/// Fixed per source identity, bounded, and free of account, resource and
/// authority detail.
pub(crate) fn generic_source_label(source_id: &str) -> &'static str {
    match source_id {
        "floe.source.calendar" => "Calendar",
        "floe.source.contacts" => "Contacts",
        "floe.source.attention" => "Attention",
        "floe.source.wellbeing" => "Wellbeing",
        "floe.source.mail" => "Mail",
        "floe.source.work-context" => "Work context",
        "floe.source.logistics" => "Logistics",
        "floe.source.tasks" => "Tasks",
        "floe.source.confirmed-memory" => "Confirmed memory",
        _ => "Connected source",
    }
}

fn purpose_label(purpose: floe_context_contract::GrantPurpose) -> &'static str {
    match purpose {
        floe_context_contract::GrantPurpose::Assistant => "assistant",
        floe_context_contract::GrantPurpose::Scheduling => "scheduling",
        floe_context_contract::GrantPurpose::Summarization => "summarization",
    }
}

/// Convert one owner-produced requirement into the stored interaction
/// request. Recipient consent is Access-owned (05-D) and never becomes a
/// source card here.
fn interaction_requirement(
    requirement: &SourceAccessRequirement,
) -> Result<floe_conversation::InteractionRequirement, AgentFailure> {
    requirement
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let kind = match requirement.reason() {
        SourceAccessRequirementKind::EnableObserve => {
            floe_conversation::InteractionRequirementKind::EnableObserve
        }
        SourceAccessRequirementKind::ReviewChangedSource => {
            floe_conversation::InteractionRequirementKind::ReviewChangedSource
        }
        SourceAccessRequirementKind::RequestSystemPermission => {
            floe_conversation::InteractionRequirementKind::RequestSystemPermission
        }
        SourceAccessRequirementKind::Reconnect => {
            floe_conversation::InteractionRequirementKind::Reconnect
        }
        SourceAccessRequirementKind::SelectResource => {
            floe_conversation::InteractionRequirementKind::SelectResource
        }
        SourceAccessRequirementKind::ApproveProcessingRecipient => {
            return Err(AgentFailure::PolicyDenied);
        }
    };
    let interaction = floe_conversation::InteractionRequirement {
        kind,
        source_id: requirement.source_id().to_owned(),
        connection_id: requirement.connection_id().map(|id| id.as_str().to_owned()),
        consumer: requirement.consumer().identifier().to_owned(),
        purpose: purpose_label(requirement.purpose()).to_owned(),
        inline: requirement.inline_resolution(),
    };
    interaction
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    Ok(interaction)
}

/// Whether the requirement may offer inline Observe once its reviewed
/// snapshot captures. Anything else navigates without capturing.
fn inline_eligible(requirement: &SourceAccessRequirement) -> bool {
    requirement.inline_resolution()
        && matches!(
            requirement.reason(),
            SourceAccessRequirementKind::EnableObserve
                | SourceAccessRequirementKind::ReviewChangedSource
        )
        && requirement.connector_id().is_some()
        && requirement.connection_id().is_some()
        && !requirement.resources().is_empty()
        && requirement.resources().len() <= floe_conversation::MAX_TARGET_BUNDLE_MEMBERS
}

/// The navigation-only target: no inline-mutation fields exist on it, so a
/// card that never offered inline enable cannot resolve one.
fn navigation_target(
    requirement: &SourceAccessRequirement,
    consumer: String,
    purpose: String,
) -> Result<floe_conversation::ReviewedTarget, AgentFailure> {
    let connector = requirement
        .connector_id()
        .map(|id| id.as_str())
        .unwrap_or_default();
    let destination = match requirement.reason() {
        SourceAccessRequirementKind::RequestSystemPermission => {
            floe_conversation::NavigationDestination::SystemPermission
        }
        SourceAccessRequirementKind::SelectResource => {
            floe_conversation::NavigationDestination::ResourcePicker
        }
        _ if connector.starts_with("contacts.") => {
            floe_conversation::NavigationDestination::ResourcePicker
        }
        _ => floe_conversation::NavigationDestination::ConnectionSettings,
    };
    let target = floe_conversation::ReviewedTarget::NavigationOnly(
        floe_conversation::NavigationOnlyTarget {
            destination,
            source_id: requirement.source_id().to_owned(),
            connection_id: requirement.connection_id().map(|id| id.as_str().to_owned()),
            consumer,
            purpose,
        },
    );
    target.validate().map_err(|_| AgentFailure::InvalidInput)?;
    Ok(target)
}

/// The inline target bound to a captured owner snapshot: the whole reviewed
/// bundle with per-member expectations, in canonical member order.
fn inline_target_from_snapshot(
    requirement: &SourceAccessRequirement,
    device_id: &str,
    consumer: String,
    purpose: String,
    snapshot: crate::vault_host::review_snapshot::InlineReviewSnapshot,
) -> Result<floe_conversation::ReviewedTarget, AgentFailure> {
    let connector_id = requirement
        .connector_id()
        .map(|id| id.as_str().to_owned())
        .ok_or(AgentFailure::InvalidInput)?;
    let connection_id = requirement
        .connection_id()
        .map(|id| id.as_str().to_owned())
        .ok_or(AgentFailure::InvalidInput)?;
    let mut members: Vec<floe_conversation::ReviewedBundleMember> = snapshot
        .members
        .into_iter()
        .map(|member| floe_conversation::ReviewedBundleMember {
            member_id: member.member_id,
            policy_digest: member.policy_digest,
            resource: member.resource,
            expected_grant: match member.expected_grant {
                Some((grant_id, authority)) => floe_conversation::ExpectedGrantState::Active {
                    grant_id: grant_id.as_uuid(),
                    authority_incarnation: authority.incarnation(),
                    authority_epoch: authority.access_epoch().get(),
                },
                None => floe_conversation::ExpectedGrantState::Absent,
            },
        })
        .collect();
    members.sort_by(|left, right| {
        left.member_id
            .cmp(&right.member_id)
            .then_with(|| left.resource.cmp(&right.resource))
    });
    if members
        .windows(2)
        .any(|pair| pair[0].member_id == pair[1].member_id && pair[0].resource == pair[1].resource)
    {
        return Err(AgentFailure::InvalidInput);
    }
    let target =
        floe_conversation::ReviewedTarget::InlineObserve(floe_conversation::InlineObserveTarget {
            connection_id,
            device_id: Some(device_id.to_owned()),
            source_id: requirement.source_id().to_owned(),
            connector_id: Some(connector_id),
            consumer,
            purpose,
            source_revision: snapshot.source_revision.map(|authority| {
                floe_conversation::AuthorityRevision {
                    incarnation: authority.incarnation(),
                    epoch: authority.epoch().get(),
                }
            }),
            connection_revision: snapshot.connection_revision,
            reviewed_producer_fingerprint: snapshot.producer_fingerprint,
            reviewed_native_subject: snapshot.native_subject,
            members,
        });
    target.validate().map_err(|_| AgentFailure::InvalidInput)?;
    Ok(target)
}

fn interaction_status(state: &floe_conversation::InteractionState) -> UserInteractionStatus {
    match state {
        floe_conversation::InteractionState::Pending => UserInteractionStatus::Pending,
        floe_conversation::InteractionState::Resolving { .. } => UserInteractionStatus::Resolving,
        floe_conversation::InteractionState::Resolved { .. } => UserInteractionStatus::Resolved,
        floe_conversation::InteractionState::Denied { .. } => UserInteractionStatus::Denied,
        floe_conversation::InteractionState::Cancelled { .. } => UserInteractionStatus::Cancelled,
        floe_conversation::InteractionState::Superseded { .. } => UserInteractionStatus::Superseded,
        floe_conversation::InteractionState::Expired => UserInteractionStatus::Expired,
    }
}

/// Publish the durable card for one expert model blockage under the Task
/// origin and return the trusted ref the endpoint attaches.
///
/// The requirement is the trusted host-captured dispatch the model owner
/// blocked on, never expert-authored content. Publication replays
/// deterministically: an identical blockage rejoins the same card.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn publish_model_blocker<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    principal: &str,
    session_id: Uuid,
    origin_run_id: RunId,
    task_id: Uuid,
    device_id: &str,
    requirement: floe_context_contract::ProcessingRequirement,
    now_unix_ms: i64,
) -> Result<UserInteractionRef, AgentFailure>
where
    Runs: floe_conversation::ConversationRepository + ?Sized,
    Interactions: floe_conversation::InteractionRepository + ?Sized,
{
    let admission = floe_conversation::publish_model_requirement(
        runs,
        interactions,
        floe_conversation::PublishModelRequirement {
            principal: principal.to_owned(),
            session_id,
            origin_run_id,
            origin: floe_conversation::InteractionOrigin::Task {
                task_id,
                capability_call_id: None,
            },
            requirement,
            device_id: device_id.to_owned(),
        },
        now_unix_ms,
    )
    .await?;
    let record = match admission {
        floe_conversation::PublishAdmission::Created(record)
        | floe_conversation::PublishAdmission::Existing(record) => record,
    };
    let reference = UserInteractionRef {
        interaction_id: record.id,
        kind: record.kind,
        status: interaction_status(&record.state),
    };
    reference.validate()?;
    Ok(reference)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn publish_expert_binding_blockers<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    principal: &str,
    session_id: Uuid,
    origin_run_id: RunId,
    task_id: Uuid,
    admission: &floe_experts::ExpertAdmissionIdentity,
    selection: &floe_experts::ExpertExecutionSelection,
    now_unix_ms: i64,
) -> Result<Vec<UserInteractionRef>, AgentFailure>
where
    Runs: floe_conversation::ConversationRepository + ?Sized,
    Interactions: floe_conversation::InteractionRepository + ?Sized,
{
    selection.validate()?;
    if selection
        .requirements
        .iter()
        .filter(|requirement| requirement.selected.len() < usize::from(requirement.minimum_sources))
        .count()
        > floe_conversation::MAX_ACTIVE_INTERACTIONS_PER_RUN
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    let mut refs = Vec::new();
    for requirement in &selection.requirements {
        if requirement.selected.len() >= usize::from(requirement.minimum_sources) {
            continue;
        }
        let admission = floe_conversation::publish_interaction(
            runs,
            interactions,
            floe_conversation::PublishInteractionRequest {
                principal: principal.to_owned(),
                session_id,
                origin_run_id,
                origin: floe_conversation::InteractionOrigin::Task {
                    task_id,
                    capability_call_id: None,
                },
                kind: UserInteractionKind::ExpertBinding,
                requirement: floe_conversation::InteractionRequirement {
                    kind: floe_conversation::InteractionRequirementKind::ConfigureExpertBinding,
                    source_id: "floe.expert.binding".into(),
                    connection_id: None,
                    consumer: admission.package.id.clone(),
                    purpose: "configuration".into(),
                    inline: false,
                },
                target: floe_conversation::ReviewedTarget::ExpertBinding(
                    floe_conversation::ExpertBindingTarget {
                        registry_instance_id: admission.registry_instance_id,
                        assignment_id: admission.assignment_id,
                        package: admission.package.clone(),
                        definition_revision: admission.definition_revision,
                        requirement_key: requirement.key.clone(),
                        capability: requirement.capability.clone(),
                        contract_version: requirement.contract_version,
                        minimum_sources: requirement.minimum_sources,
                        maximum_sources: requirement.maximum_sources,
                        expected_binding_revision: selection.binding_revision,
                        admitted_selection_digest: selection.digest,
                    },
                ),
            },
            now_unix_ms,
        )
        .await?;
        let record = match admission {
            floe_conversation::PublishAdmission::Created(record)
            | floe_conversation::PublishAdmission::Existing(record) => record,
        };
        refs.push(UserInteractionRef {
            interaction_id: record.id,
            kind: UserInteractionKind::ExpertBinding,
            status: interaction_status(&record.state),
        });
    }
    Ok(refs)
}

/// Publish every blocker under the admitted origin and return the safe refs.
///
/// Publication is deterministic in origin plus canonical requirement digest:
/// replaying the same requirement replays the same interaction, while a
/// different account binds a separate id. An inline-eligible requirement
/// captures its reviewed owner snapshot; a capture failure navigates to a
/// fresh review instead of offering an unbound inline mutation.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn publish_requirements<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    snapshots: &dyn crate::vault_host::review_snapshot::ReviewSnapshotSource,
    principal: &str,
    session_id: Uuid,
    origin_run_id: RunId,
    origin: floe_conversation::InteractionOrigin,
    person_id: PersonId,
    device_id: &str,
    blockers: &SourceAccessBlockers,
    cancellation: &floe_execution::Cancellation,
    now_unix_ms: i64,
) -> Result<Vec<UserInteractionRef>, AgentFailure>
where
    Runs: floe_conversation::ConversationRepository + ?Sized,
    Interactions: floe_conversation::InteractionRepository + ?Sized,
{
    blockers
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    if device_id.trim().is_empty() || device_id.len() > 256 {
        return Err(AgentFailure::InvalidInput);
    }
    let mut refs = Vec::with_capacity(blockers.blockers().len());
    for requirement in blockers.blockers() {
        let interaction = interaction_requirement(requirement)?;
        let target = if inline_eligible(requirement) {
            match snapshots
                .capture_inline(requirement, person_id, device_id, cancellation)
                .await
            {
                Ok(snapshot) => inline_target_from_snapshot(
                    requirement,
                    device_id,
                    interaction.consumer.clone(),
                    interaction.purpose.clone(),
                    snapshot,
                )?,
                Err(_) => navigation_target(
                    requirement,
                    interaction.consumer.clone(),
                    interaction.purpose.clone(),
                )?,
            }
        } else {
            navigation_target(
                requirement,
                interaction.consumer.clone(),
                interaction.purpose.clone(),
            )?
        };
        let admission = floe_conversation::publish_interaction(
            runs,
            interactions,
            floe_conversation::PublishInteractionRequest {
                principal: principal.to_owned(),
                session_id,
                origin_run_id,
                origin: origin.clone(),
                kind: UserInteractionKind::SourceAccess,
                requirement: interaction,
                target,
            },
            now_unix_ms,
        )
        .await?;
        let record = match admission {
            floe_conversation::PublishAdmission::Created(record)
            | floe_conversation::PublishAdmission::Existing(record) => record,
        };
        let reference = UserInteractionRef {
            interaction_id: record.id,
            kind: UserInteractionKind::SourceAccess,
            status: interaction_status(&record.state),
        };
        reference.validate()?;
        refs.push(reference);
    }
    Ok(refs)
}

/// The deterministic model-safe text for a blocked read: generic source
/// labels only, never account, resource or authority detail.
pub(crate) fn blocked_text(blockers: &SourceAccessBlockers) -> Result<String, AgentFailure> {
    blockers
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let mut labels: Vec<&str> = blockers
        .blockers()
        .iter()
        .map(|blocker| generic_source_label(blocker.source_id()))
        .collect();
    labels.sort();
    labels.dedup();
    if labels.len() == 1 {
        return Ok(format!(
            "{} access needs your review before this read can complete.",
            labels[0]
        ));
    }
    Ok(format!(
        "{} sources need your review before this read can complete: {}.",
        labels.len(),
        labels.join(", ")
    ))
}

/// One safe-ref artifact per durable interaction: opaque id plus kind
/// label only, source-independent.
pub(crate) fn interaction_ref_artifacts(
    refs: &[UserInteractionRef],
) -> Result<Vec<Artifact>, AgentFailure> {
    if refs.is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    let mut artifacts = Vec::with_capacity(refs.len());
    for reference in refs {
        reference.validate()?;
        artifacts.push(Artifact {
            artifact_id: Uuid::new_v4(),
            name: "user_interaction".into(),
            parts: vec![ArtifactPart::Data {
                media_type: USER_INTERACTION_MEDIA_TYPE.into(),
                data: serde_json::to_string(reference)
                    .map_err(|_| AgentFailure::InvalidModelOutput)?,
            }],
            coverage: DependencyCoverage::Independent,
        });
    }
    Ok(artifacts)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use floe_agent_contract::{
        BoxFuture, DependencyCoverage, InvocationKey, JournalEvent, ToolCall,
    };
    use floe_context_contract::{
        ConnectionId, ConnectorId, GrantConsumer, GrantOperation, GrantPurpose, ResourceHandle,
    };
    use floe_conversation::{
        ConversationInteraction, DecisionAdmission, ExpireInteraction, ExpireOutcome,
        InteractionDecision, InteractionResolution, JournalEntry, PublishAdmission, RunReceipt,
        RunState, SupersedeInteraction,
    };

    use super::*;

    fn requirement(
        source_id: &str,
        reason: SourceAccessRequirementKind,
        inline: bool,
    ) -> SourceAccessRequirement {
        let (connector, connection, resources) = if inline {
            (
                Some(ConnectorId::try_new("attention.macos").unwrap()),
                Some(ConnectionId::try_new("attention.macos.local").unwrap()),
                vec![ResourceHandle::try_new("attention.coarse").unwrap()],
            )
        } else {
            (None, None, vec![])
        };
        SourceAccessRequirement::try_new(
            source_id,
            connector,
            connection,
            GrantOperation::Read,
            GrantConsumer::builtin("floe.builtin.focus-attention").unwrap(),
            GrantPurpose::Assistant,
            resources,
            None,
            reason,
            None,
            None,
            inline,
        )
        .unwrap()
    }

    struct ScriptedSnapshots {
        snapshot:
            Mutex<Result<crate::vault_host::review_snapshot::InlineReviewSnapshot, AgentFailure>>,
    }

    impl crate::vault_host::review_snapshot::ReviewSnapshotSource for ScriptedSnapshots {
        fn capture_inline<'a>(
            &'a self,
            _requirement: &'a SourceAccessRequirement,
            _person_id: PersonId,
            _device_id: &'a str,
            _cancellation: &'a floe_execution::Cancellation,
        ) -> BoxFuture<
            'a,
            Result<crate::vault_host::review_snapshot::InlineReviewSnapshot, AgentFailure>,
        > {
            let snapshot = self.snapshot.lock().unwrap();
            let cloned = match &*snapshot {
                Ok(snapshot) => Ok(crate::vault_host::review_snapshot::InlineReviewSnapshot {
                    source_revision: snapshot.source_revision,
                    members: snapshot.members.clone(),
                    connection_revision: snapshot.connection_revision,
                    producer_fingerprint: snapshot.producer_fingerprint.clone(),
                    native_subject: snapshot.native_subject.clone(),
                }),
                Err(failure) => Err(*failure),
            };
            Box::pin(async move { cloned })
        }
    }

    fn captured_snapshot() -> crate::vault_host::review_snapshot::InlineReviewSnapshot {
        crate::vault_host::review_snapshot::InlineReviewSnapshot {
            source_revision: None,
            members: vec![crate::vault_host::review_snapshot::SnapshotMember {
                member_id: "attention.macos".into(),
                policy_digest: "a".repeat(64),
                resource: "attention.coarse".into(),
                expected_grant: None,
            }],
            connection_revision: None,
            producer_fingerprint: None,
            native_subject: Some("b".repeat(64)),
        }
    }

    #[test]
    fn inline_observe_needs_complete_identity_and_reviewable_reason() {
        assert!(inline_eligible(&requirement(
            "floe.source.attention",
            SourceAccessRequirementKind::EnableObserve,
            true,
        )));
        // Incomplete identity never captures even when the owner hinted inline.
        assert!(!inline_eligible(&requirement(
            "floe.source.contacts",
            SourceAccessRequirementKind::EnableObserve,
            false,
        )));
        // Reconnect never offers inline mutation.
        assert!(!inline_eligible(&requirement(
            "floe.source.mail",
            SourceAccessRequirementKind::Reconnect,
            true,
        )));

        let target = navigation_target(
            &requirement(
                "floe.source.contacts",
                SourceAccessRequirementKind::EnableObserve,
                false,
            ),
            "assistant".into(),
            "assistant".into(),
        )
        .unwrap();
        let floe_conversation::ReviewedTarget::NavigationOnly(nav) = target else {
            panic!("unknown identity must navigate");
        };
        assert_eq!(
            nav.destination,
            floe_conversation::NavigationDestination::ConnectionSettings
        );
    }

    #[test]
    fn recipient_consent_never_becomes_a_source_card() {
        let requirement = SourceAccessRequirement::try_new(
            "floe.source.mail",
            None,
            None,
            GrantOperation::Read,
            GrantConsumer::builtin("floe.builtin.focus-attention").unwrap(),
            GrantPurpose::Assistant,
            vec![],
            Some("model.example".into()),
            SourceAccessRequirementKind::ApproveProcessingRecipient,
            None,
            None,
            false,
        )
        .unwrap();
        assert_eq!(
            interaction_requirement(&requirement),
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn captured_snapshot_binds_inline_members_and_failure_navigates() {
        let person_id = PersonId::new();
        let session_id = Uuid::new_v4();
        let run_id = RunId::new();
        let call = tool_call("test.source-blocker");
        let runs = FakeRuns {
            receipt: receipt_fixture(person_id, session_id, run_id),
            journal: vec![JournalEntry {
                revision: 1,
                event: JournalEvent::ToolIntent { call: call.clone() },
            }],
        };
        let interactions = FakeInteractions::default();
        let cancellation = floe_execution::Cancellation::default();
        let blockers = SourceAccessBlockers::try_new(vec![requirement(
            "floe.source.attention",
            SourceAccessRequirementKind::EnableObserve,
            true,
        )])
        .unwrap();

        // A captured snapshot binds the inline members, the owner identity
        // and the reviewed absence exactly as captured.
        let snapshots = ScriptedSnapshots {
            snapshot: Mutex::new(Ok(captured_snapshot())),
        };
        let refs = publish_requirements(
            &runs,
            &interactions,
            &snapshots,
            &person_id.to_string(),
            session_id,
            run_id,
            floe_conversation::InteractionOrigin::Tool {
                call_id: call.call_id,
            },
            person_id,
            "device",
            &blockers,
            &cancellation,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(refs.len(), 1);
        let stored = interactions
            .records
            .lock()
            .unwrap()
            .get(&refs[0].interaction_id)
            .cloned()
            .unwrap();
        let floe_conversation::ReviewedTarget::InlineObserve(inline) = &stored.target else {
            panic!("captured snapshot must bind inline observe");
        };
        assert_eq!(inline.connection_id, "attention.macos.local");
        assert_eq!(inline.device_id.as_deref(), Some("device"));
        assert_eq!(inline.members.len(), 1);
        assert_eq!(inline.members[0].member_id, "attention.macos");
        assert_eq!(inline.members[0].resource, "attention.coarse");
        assert_eq!(
            inline.members[0].expected_grant,
            floe_conversation::ExpectedGrantState::Absent
        );
        assert_eq!(
            inline.reviewed_native_subject.as_deref(),
            Some("b".repeat(64).as_str())
        );

        // A capture failure still publishes, but navigates: no inline
        // mutation without a bound snapshot.
        let failing = ScriptedSnapshots {
            snapshot: Mutex::new(Err(AgentFailure::CapabilityUnavailable)),
        };
        let declined = publish_requirements(
            &runs,
            &interactions,
            &failing,
            &person_id.to_string(),
            session_id,
            run_id,
            floe_conversation::InteractionOrigin::Tool {
                call_id: call.call_id,
            },
            person_id,
            "device",
            &blockers,
            &cancellation,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(declined.len(), 1);
        assert_ne!(declined[0].interaction_id, refs[0].interaction_id);
        let navigated = interactions
            .records
            .lock()
            .unwrap()
            .get(&declined[0].interaction_id)
            .cloned()
            .unwrap();
        assert!(matches!(
            navigated.target,
            floe_conversation::ReviewedTarget::NavigationOnly(_)
        ));
    }

    #[test]
    fn blocked_text_names_generic_labels_only() {
        let single = SourceAccessBlockers::try_new(vec![requirement(
            "floe.source.mail",
            SourceAccessRequirementKind::SelectResource,
            false,
        )])
        .unwrap();
        assert_eq!(
            blocked_text(&single).unwrap(),
            "Mail access needs your review before this read can complete."
        );
        let multi = SourceAccessBlockers::try_new(vec![
            requirement(
                "floe.source.mail",
                SourceAccessRequirementKind::SelectResource,
                false,
            ),
            requirement(
                "floe.source.calendar",
                SourceAccessRequirementKind::EnableObserve,
                true,
            ),
        ])
        .unwrap();
        let text = blocked_text(&multi).unwrap();
        assert!(text.starts_with("2 sources need your review"), "{text}");
        assert!(text.contains("Calendar") && text.contains("Mail"), "{text}");
        for secret in [
            "attention.macos.local",
            "b-connection",
            "fingerprint",
            "token",
            "credential",
        ] {
            assert!(!text.contains(secret), "{text}");
        }
    }

    struct FakeRuns {
        receipt: RunReceipt,
        journal: Vec<JournalEntry>,
    }

    impl floe_conversation::ConversationRepository for FakeRuns {
        fn find_command<'a>(
            &'a self,
            _: floe_conversation::CommandQuery,
        ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>> {
            Box::pin(async { unimplemented!("publication needs no command lookup") })
        }

        fn admit_turn<'a>(
            &'a self,
            _: floe_conversation::TurnAdmissionRequest,
        ) -> BoxFuture<'a, Result<floe_conversation::TurnAdmission, AgentFailure>> {
            Box::pin(async { unimplemented!("publication needs no admission") })
        }

        fn admit_cancel<'a>(
            &'a self,
            _: floe_conversation::CancelRunCommand,
        ) -> BoxFuture<'a, Result<floe_conversation::CancelRunAdmission, AgentFailure>> {
            Box::pin(async { unimplemented!("publication needs no cancellation") })
        }

        fn journal(
            &self,
            _: RunId,
        ) -> Result<Arc<dyn floe_agent_contract::ExecutionJournal>, AgentFailure> {
            unimplemented!("publication needs no journal handle")
        }

        fn finish_run<'a>(
            &'a self,
            _: RunId,
            _: u64,
            _: floe_conversation::RunTerminal,
        ) -> BoxFuture<'a, Result<RunReceipt, AgentFailure>> {
            Box::pin(async { unimplemented!("publication never finishes runs") })
        }

        fn load_run<'a>(
            &'a self,
            _: RunId,
        ) -> BoxFuture<'a, Result<Option<floe_conversation::AdmittedTurn>, AgentFailure>> {
            Box::pin(async { unimplemented!("publication needs no turn load") })
        }

        fn load_receipt<'a>(
            &'a self,
            run_id: RunId,
        ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>> {
            let receipt = (run_id == self.receipt.run_id).then(|| self.receipt.clone());
            Box::pin(async move { Ok(receipt) })
        }

        fn recover_session<'a>(
            &'a self,
            _: floe_conversation::RecoveryRequest,
        ) -> BoxFuture<'a, Result<floe_conversation::RecoveryReceipt, AgentFailure>> {
            Box::pin(async { unimplemented!("publication needs no recovery") })
        }

        fn load_journal<'a>(
            &'a self,
            run_id: RunId,
        ) -> BoxFuture<'a, Result<Vec<JournalEntry>, AgentFailure>> {
            let journal = if run_id == self.receipt.run_id {
                self.journal.clone()
            } else {
                vec![]
            };
            Box::pin(async move { Ok(journal) })
        }
    }

    #[derive(Default)]
    struct FakeInteractions {
        records: Mutex<HashMap<Uuid, ConversationInteraction>>,
    }

    impl floe_conversation::InteractionRepository for FakeInteractions {
        fn publish_interaction<'a>(
            &'a self,
            record: ConversationInteraction,
        ) -> BoxFuture<'a, Result<PublishAdmission, AgentFailure>> {
            let mut records = self.records.lock().unwrap();
            if let Some(existing) = records.get(&record.id) {
                let existing = existing.clone();
                return Box::pin(async move { Ok(PublishAdmission::Existing(existing)) });
            }
            records.insert(record.id, record.clone());
            Box::pin(async move { Ok(PublishAdmission::Created(record)) })
        }

        fn get_interaction<'a>(
            &'a self,
            person_id: PersonId,
            interaction_id: Uuid,
        ) -> BoxFuture<'a, Result<Option<ConversationInteraction>, AgentFailure>> {
            let found = self
                .records
                .lock()
                .unwrap()
                .get(&interaction_id)
                .filter(|record| record.person_id == person_id)
                .cloned();
            Box::pin(async move { Ok(found) })
        }

        fn list_run_interactions<'a>(
            &'a self,
            _: PersonId,
            _: RunId,
        ) -> BoxFuture<'a, Result<Vec<ConversationInteraction>, AgentFailure>> {
            Box::pin(async { Ok(vec![]) })
        }

        fn record_decision<'a>(
            &'a self,
            _: InteractionDecision,
        ) -> BoxFuture<'a, Result<DecisionAdmission, AgentFailure>> {
            Box::pin(async { unimplemented!("publication tests decide nothing") })
        }

        fn record_resolution<'a>(
            &'a self,
            _: InteractionResolution,
        ) -> BoxFuture<'a, Result<ConversationInteraction, AgentFailure>> {
            Box::pin(async { unimplemented!("publication tests resolve nothing") })
        }

        fn mark_superseded<'a>(
            &'a self,
            _: SupersedeInteraction,
        ) -> BoxFuture<'a, Result<ConversationInteraction, AgentFailure>> {
            Box::pin(async { unimplemented!("publication tests supersede nothing") })
        }

        fn mark_expired<'a>(
            &'a self,
            _: ExpireInteraction,
        ) -> BoxFuture<'a, Result<ExpireOutcome, AgentFailure>> {
            Box::pin(async { unimplemented!("publication tests expire nothing") })
        }
    }

    fn receipt_fixture(person_id: PersonId, session_id: Uuid, run_id: RunId) -> RunReceipt {
        RunReceipt {
            expert_environment: floe_experts::RunExpertEnvironmentIdentity {
                revision: 1,
                digest: [1; 32],
            },
            run_id,
            command_id: floe_agent_contract::CommandId::new(),
            session_id,
            principal: person_id.to_string(),
            request_digest: [7; 32],
            state: RunState::Working,
            output: None,
            coverage: DependencyCoverage::Independent,
            issue: None,
            session_revision: 1,
            aggregate_revision: 1,
            executor_generation: 1,
            continuation_of: None,
            continuation_executor_generation: None,
            continuation_level: 0,
            retry_of: None,
            resume_of: None,
            resume_lineage: 0,
            profile: floe_conversation::ProfileSelection::Auto,
            attempt_refs: vec![],
            task_refs: vec![],
        }
    }

    fn tool_call(tool_id: &str) -> ToolCall {
        ToolCall {
            call_id: Uuid::new_v4(),
            invocation_key: InvocationKey::new(),
            tool_id: tool_id.into(),
            definition_revision: 1,
            input: "{}".into(),
        }
    }

    #[tokio::test]
    async fn shared_publication_replays_the_same_safe_ref() {
        let person_id = PersonId::new();
        let session_id = Uuid::new_v4();
        let run_id = RunId::new();
        let task_id = floe_agent_contract::TaskId::new();
        let request = floe_agent_contract::DelegationRequest {
            task_id,
            parent_run_id: Some(run_id.as_uuid()),
            principal: person_id.to_string(),
            invocation_key: InvocationKey::new(),
            selected_agent_id: "floe.builtin.focus-attention".into(),
            selected_definition_revision: 1,
            message: "Assess current attention".into(),
            context_refs: vec![],
            execution_context: floe_agent_contract::DelegationExecutionContext {
                session_id,
                device_id: "device".into(),
                agent_context: floe_agent_contract::AgentContext {
                    projection_version: 1,
                    persona: None,
                    memories: vec![],
                    optional_context_issues: vec![],
                    evidence: vec![],
                },
                max_output_bytes: floe_agent_contract::MAX_OUTPUT_BYTES,
            },
        };
        let blockers = SourceAccessBlockers::try_new(vec![requirement(
            "floe.source.attention",
            SourceAccessRequirementKind::EnableObserve,
            true,
        )])
        .unwrap();
        let runs = FakeRuns {
            receipt: receipt_fixture(person_id, session_id, run_id),
            journal: vec![JournalEntry {
                revision: 1,
                event: JournalEvent::DelegationIntent { request },
            }],
        };
        let interactions = FakeInteractions::default();
        let snapshots = crate::vault_host::review_snapshot::NoCaptureSnapshots;
        let cancellation = floe_execution::Cancellation::default();
        let origin = floe_conversation::InteractionOrigin::Task {
            task_id: task_id.as_uuid(),
            capability_call_id: None,
        };
        let first = publish_requirements(
            &runs,
            &interactions,
            &snapshots,
            &person_id.to_string(),
            session_id,
            run_id,
            origin.clone(),
            person_id,
            "device",
            &blockers,
            &cancellation,
            1_000,
        )
        .await
        .unwrap();
        let second = publish_requirements(
            &runs,
            &interactions,
            &snapshots,
            &person_id.to_string(),
            session_id,
            run_id,
            origin,
            person_id,
            "device",
            &blockers,
            &cancellation,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(first, second);
        let records = interactions.records.lock().unwrap();
        let stored = records.get(&first[0].interaction_id).unwrap();
        assert_eq!(
            stored.origin,
            floe_conversation::InteractionOrigin::Task {
                task_id: task_id.as_uuid(),
                capability_call_id: None,
            }
        );
        assert_eq!(stored.requirement.consumer, "floe.builtin.focus-attention");
        drop(records);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].kind, UserInteractionKind::SourceAccess);
        assert_eq!(first[0].status, UserInteractionStatus::Pending);
        let artifacts = interaction_ref_artifacts(&first).unwrap();
        assert_eq!(artifacts[0].coverage, DependencyCoverage::Independent);
        let ArtifactPart::Data { media_type, data } = &artifacts[0].parts[0] else {
            panic!("safe reference");
        };
        assert_eq!(media_type, USER_INTERACTION_MEDIA_TYPE);
        assert_eq!(
            serde_json::from_str::<UserInteractionRef>(data).unwrap(),
            first[0]
        );
        assert!(!data.contains("fingerprint"));
        assert_eq!(interactions.records.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn different_accounts_bind_separate_interactions() {
        let person_id = PersonId::new();
        let session_id = Uuid::new_v4();
        let run_id = RunId::new();
        let call = tool_call("test.source-blocker");
        let blocker_for = |connection: &str| {
            SourceAccessRequirement::try_new(
                "floe.source.mail",
                Some(ConnectorId::try_new("gmail").unwrap()),
                Some(ConnectionId::try_new(connection).unwrap()),
                GrantOperation::Read,
                GrantConsumer::builtin("floe.builtin.focus-attention").unwrap(),
                GrantPurpose::Assistant,
                vec![ResourceHandle::try_new(format!("mail.communication:{connection}")).unwrap()],
                None,
                SourceAccessRequirementKind::EnableObserve,
                None,
                None,
                true,
            )
            .unwrap()
        };
        let runs = FakeRuns {
            receipt: receipt_fixture(person_id, session_id, run_id),
            journal: vec![JournalEntry {
                revision: 1,
                event: JournalEvent::ToolIntent { call: call.clone() },
            }],
        };
        let interactions = FakeInteractions::default();
        let snapshots = crate::vault_host::review_snapshot::NoCaptureSnapshots;
        let cancellation = floe_execution::Cancellation::default();
        let first = publish_requirements(
            &runs,
            &interactions,
            &snapshots,
            &person_id.to_string(),
            session_id,
            run_id,
            floe_conversation::InteractionOrigin::Tool {
                call_id: call.call_id,
            },
            person_id,
            "device",
            &SourceAccessBlockers::try_new(vec![blocker_for("a-connection")]).unwrap(),
            &cancellation,
            1_000,
        )
        .await
        .unwrap();
        let second = publish_requirements(
            &runs,
            &interactions,
            &snapshots,
            &person_id.to_string(),
            session_id,
            run_id,
            floe_conversation::InteractionOrigin::Tool {
                call_id: call.call_id,
            },
            person_id,
            "device",
            &SourceAccessBlockers::try_new(vec![blocker_for("b-connection")]).unwrap(),
            &cancellation,
            1_000,
        )
        .await
        .unwrap();
        assert_ne!(
            first[0].interaction_id, second[0].interaction_id,
            "different accounts must never share a review"
        );
    }

    #[tokio::test]
    async fn unadmitted_origin_and_missing_run_stay_hard_failures() {
        let person_id = PersonId::new();
        let session_id = Uuid::new_v4();
        let run_id = RunId::new();
        let runs = FakeRuns {
            receipt: receipt_fixture(person_id, session_id, run_id),
            journal: vec![],
        };
        let interactions = FakeInteractions::default();
        let snapshots = crate::vault_host::review_snapshot::NoCaptureSnapshots;
        let blockers = SourceAccessBlockers::try_new(vec![requirement(
            "floe.source.attention",
            SourceAccessRequirementKind::EnableObserve,
            true,
        )])
        .unwrap();
        for (requested_run, expected) in [
            (run_id, AgentFailure::Conflict),
            (RunId::new(), AgentFailure::NotFound),
        ] {
            assert_eq!(
                publish_requirements(
                    &runs,
                    &interactions,
                    &snapshots,
                    &person_id.to_string(),
                    session_id,
                    requested_run,
                    floe_conversation::InteractionOrigin::Task {
                        task_id: Uuid::new_v4(),
                        capability_call_id: None
                    },
                    person_id,
                    "device",
                    &blockers,
                    &floe_execution::Cancellation::default(),
                    1_000
                )
                .await
                .unwrap_err(),
                expected
            );
        }
        assert!(interactions.records.lock().unwrap().is_empty());
    }
}
