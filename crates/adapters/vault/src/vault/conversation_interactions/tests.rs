use std::{
    collections::HashMap,
    fs,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

use floe_conversation::{
    INTERACTION_PENDING_LIFETIME_MS, InteractionDecisionKind, InteractionOrigin,
    InteractionRequirement, InteractionRequirementKind, InteractionResolution, InteractionState,
    MAX_ACTIVE_INTERACTIONS_PER_RUN, MAX_STORED_INTERACTIONS_PER_RUN, ReviewedTarget,
    canonical_requirement_digest, canonical_target_digest, interaction_publication_id,
};

use super::*;

#[derive(Clone, Default)]
struct Keys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

impl VaultKeyProvider for Keys {
    fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<VaultKey, AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .get(&(person_id, vault_id))
            .copied()
            .map(VaultKey::from_bytes)
            .ok_or(AgentFailure::VaultUnavailable)
    }

    fn insert(
        &self,
        person_id: PersonId,
        vault_id: Uuid,
        key: &VaultKey,
    ) -> Result<(), AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .insert((person_id, vault_id), *key.as_bytes());
        Ok(())
    }
}

const NOW: i64 = 1_700_000_000_000;

fn requirement() -> InteractionRequirement {
    InteractionRequirement {
        kind: InteractionRequirementKind::EnableObserve,
        source_id: "floe.source.calendar".into(),
        connection_id: Some("calendar-connection".into()),
        consumer: "floe.builtin.schedule".into(),
        purpose: "scheduling".into(),
        inline: true,
    }
}

fn target() -> ReviewedTarget {
    ReviewedTarget::InlineObserve(floe_conversation::InlineObserveTarget {
        connection_id: "calendar-connection".into(),
        device_id: None,
        source_id: "floe.source.calendar".into(),
        connector_id: Some("floe.connector.calendar".into()),
        consumer: "floe.builtin.schedule".into(),
        purpose: "scheduling".into(),
        connection_revision: None,
        reviewed_producer_fingerprint: None,
        reviewed_native_subject: None,
        members: vec![floe_conversation::ReviewedBundleMember {
            member_id: "calendar.timeline".into(),
            resource: "personal".into(),
            source_revision: None,
            expected_grant: floe_conversation::ExpectedGrantState::Absent,
            policy_authority: None,
        }],
    })
}

fn record(
    person_id: PersonId,
    session_id: Uuid,
    run_id: RunId,
    call_id: Uuid,
    created_at: i64,
) -> ConversationInteraction {
    let requirement = requirement();
    let target = target();
    let requirement_digest = canonical_requirement_digest(&requirement).unwrap();
    let target_digest = canonical_target_digest(&target).unwrap();
    let origin = InteractionOrigin::Tool { call_id };
    let id =
        interaction_publication_id(run_id, &origin, &requirement_digest, &target_digest).unwrap();
    ConversationInteraction {
        id,
        person_id,
        session_id,
        origin_run_id: run_id,
        origin_turn_id: run_id.as_uuid(),
        origin,
        kind: floe_agent_contract::UserInteractionKind::SourceAccess,
        requirement,
        requirement_digest,
        target,
        target_digest,
        state: InteractionState::Pending,
        revision: 1,
        created_at_unix_ms: created_at,
        expires_at_unix_ms: created_at + INTERACTION_PENDING_LIFETIME_MS,
    }
}

fn decision_for(
    record: &ConversationInteraction,
    kind: InteractionDecisionKind,
    decided_at: i64,
) -> InteractionDecision {
    InteractionDecision {
        command_id: Uuid::new_v4(),
        interaction_id: record.id,
        interaction_revision: record.revision,
        kind,
        target_digest: record.target_digest,
        principal: record.person_id.to_string(),
        decided_at_unix_ms: decided_at,
    }
}

async fn setup() -> (
    tempfile::TempDir,
    EncryptedAgentVault<Keys>,
    PersonId,
    Keys,
    Uuid,
) {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
        .await
        .unwrap();
    vault.activate_conversation_executor().await.unwrap();
    let session = vault.create_session().await.unwrap();
    (root, vault, person_id, keys, session.id)
}

async fn reopened(vault: &EncryptedAgentVault<Keys>, id: Uuid) -> ConversationInteraction {
    vault.conversation_interaction(id).await.unwrap().unwrap()
}

async fn admit_run(
    vault: &EncryptedAgentVault<Keys>,
    person_id: PersonId,
    session_id: Uuid,
    expected_revision: u64,
) -> RunId {
    let run_id = RunId::new();
    let admission = vault
        .admit_conversation_turn(VaultConversationAdmissionRequest {
            run_id,
            command_id: floe_agent_contract::CommandId::new(),
            session_id,
            person_id,
            expected_session_revision: expected_revision,
            request_digest: [7; 32],
            text: "hello".into(),
            continuation: None,
            retry_of: None,
            profile: floe_conversation::ProfileSelection::Auto,
        })
        .await
        .unwrap();
    assert!(matches!(
        admission,
        VaultConversationAdmission::Created { .. }
    ));
    run_id
}

#[tokio::test]
async fn publish_replays_identically_after_reopen() {
    let (root, vault, person_id, keys, session_id) = setup().await;
    let run_id = admit_run(&vault, person_id, session_id, 0).await;
    let published = record(person_id, session_id, run_id, Uuid::new_v4(), NOW);
    let PublishAdmission::Created(first) = vault
        .publish_conversation_interaction(published.clone())
        .await
        .unwrap()
    else {
        panic!("first publish must create");
    };
    assert_eq!(first, published);
    drop(vault);

    let vault = EncryptedAgentVault::open(root.path(), person_id, keys)
        .await
        .unwrap();
    let PublishAdmission::Existing(second) = vault
        .publish_conversation_interaction(published.clone())
        .await
        .unwrap()
    else {
        panic!("replay must rejoin");
    };
    assert_eq!(second, published);
    let listed = vault.run_conversation_interactions(run_id).await.unwrap();
    assert_eq!(listed, vec![published]);
}

#[tokio::test]
async fn publish_rejects_foreign_person_unknown_and_cancelled_run() {
    let (_root, vault, person_id, _keys, session_id) = setup().await;
    let run_id = admit_run(&vault, person_id, session_id, 0).await;

    let mut foreign = record(person_id, session_id, run_id, Uuid::new_v4(), NOW);
    foreign.person_id = PersonId::new();
    assert_eq!(
        vault.publish_conversation_interaction(foreign).await,
        Err(AgentFailure::CapabilityDenied)
    );

    let unknown_run = record(person_id, session_id, RunId::new(), Uuid::new_v4(), NOW);
    assert_eq!(
        vault.publish_conversation_interaction(unknown_run).await,
        Err(AgentFailure::NotFound)
    );

    vault
        .finish_conversation_run(
            run_id,
            1,
            VaultConversationTerminal {
                state: VaultConversationRunState::Completed,
                output: Some("done".into()),
                coverage: floe_agent_contract::DependencyCoverage::Independent,
                issue: None,
                appended_messages: vec![floe_conversation::AgentMessage::Assistant {
                    turn_id: run_id.as_uuid(),
                    text: "done".into(),
                }],
            },
        )
        .await
        .unwrap();
    let cancelled_run = admit_run(&vault, person_id, session_id, 2).await;
    vault
        .finish_conversation_run(
            cancelled_run,
            1,
            VaultConversationTerminal {
                state: VaultConversationRunState::Cancelled,
                output: None,
                coverage: floe_agent_contract::DependencyCoverage::Unknown,
                issue: Some(AgentFailure::Cancelled),
                appended_messages: vec![],
            },
        )
        .await
        .unwrap();
    let orphan = record(person_id, session_id, cancelled_run, Uuid::new_v4(), NOW);
    assert_eq!(
        vault.publish_conversation_interaction(orphan).await,
        Err(AgentFailure::Conflict)
    );
}

#[tokio::test]
async fn publish_enforces_active_limit_and_frees_decided_slots() {
    let (_root, vault, person_id, _keys, session_id) = setup().await;
    let run_id = admit_run(&vault, person_id, session_id, 0).await;

    let mut published = Vec::new();
    for _ in 0..MAX_ACTIVE_INTERACTIONS_PER_RUN {
        let candidate = record(person_id, session_id, run_id, Uuid::new_v4(), NOW);
        let PublishAdmission::Created(created) = vault
            .publish_conversation_interaction(candidate)
            .await
            .unwrap()
        else {
            panic!("publish must create");
        };
        published.push(created);
    }
    let overflow = record(person_id, session_id, run_id, Uuid::new_v4(), NOW);
    assert_eq!(
        vault.publish_conversation_interaction(overflow).await,
        Err(AgentFailure::BudgetExceeded)
    );

    for record in &published {
        let decision = decision_for(record, InteractionDecisionKind::Deny, NOW + 1);
        assert!(matches!(
            vault
                .record_conversation_interaction_decision(decision)
                .await
                .unwrap(),
            DecisionAdmission::Applied(_)
        ));
    }
    let freed = record(person_id, session_id, run_id, Uuid::new_v4(), NOW + 2);
    assert!(matches!(
        vault.publish_conversation_interaction(freed).await.unwrap(),
        PublishAdmission::Created(_)
    ));
}

#[tokio::test]
async fn publish_enforces_total_stored_limit() {
    let (_root, vault, person_id, _keys, session_id) = setup().await;
    let run_id = admit_run(&vault, person_id, session_id, 0).await;

    for round in 0..(MAX_STORED_INTERACTIONS_PER_RUN / MAX_ACTIVE_INTERACTIONS_PER_RUN) {
        for index in 0..MAX_ACTIVE_INTERACTIONS_PER_RUN {
            let created_at = NOW + (round * MAX_ACTIVE_INTERACTIONS_PER_RUN + index) as i64;
            let candidate = record(person_id, session_id, run_id, Uuid::new_v4(), created_at);
            let PublishAdmission::Created(created) = vault
                .publish_conversation_interaction(candidate)
                .await
                .unwrap()
            else {
                panic!("publish must create");
            };
            let decision = decision_for(&created, InteractionDecisionKind::Deny, created_at + 1);
            assert!(matches!(
                vault
                    .record_conversation_interaction_decision(decision)
                    .await
                    .unwrap(),
                DecisionAdmission::Applied(_)
            ));
        }
    }
    assert_eq!(
        MAX_STORED_INTERACTIONS_PER_RUN % MAX_ACTIVE_INTERACTIONS_PER_RUN,
        0
    );
    let overflow = record(person_id, session_id, run_id, Uuid::new_v4(), NOW);
    assert_eq!(
        vault.publish_conversation_interaction(overflow).await,
        Err(AgentFailure::BudgetExceeded)
    );
    assert_eq!(
        vault
            .run_conversation_interactions(run_id)
            .await
            .unwrap()
            .len(),
        MAX_STORED_INTERACTIONS_PER_RUN
    );
}

#[tokio::test]
async fn decision_applies_rejoins_and_conflicts() {
    let (_root, vault, person_id, _keys, session_id) = setup().await;
    let run_id = admit_run(&vault, person_id, session_id, 0).await;
    let published = record(person_id, session_id, run_id, Uuid::new_v4(), NOW);
    let PublishAdmission::Created(created) = vault
        .publish_conversation_interaction(published)
        .await
        .unwrap()
    else {
        panic!("publish must create");
    };

    let approve = decision_for(&created, InteractionDecisionKind::Approve, NOW + 1);
    let DecisionAdmission::Applied(applied) = vault
        .record_conversation_interaction_decision(approve.clone())
        .await
        .unwrap()
    else {
        panic!("decision must apply");
    };
    assert_eq!(applied.revision, 2);
    let InteractionState::Resolving {
        decision_id,
        owner_operation_id,
    } = applied.state
    else {
        panic!("approve must move to resolving");
    };
    assert_eq!(decision_id, approve.command_id);
    assert_eq!(
        owner_operation_id,
        floe_conversation::decision_operation_id(approve.command_id)
    );

    // An identical retry rejoins before any revision check.
    let mut rejoin = approve.clone();
    rejoin.interaction_revision = 1;
    let DecisionAdmission::Rejoined(rejoined) = vault
        .record_conversation_interaction_decision(rejoin)
        .await
        .unwrap()
    else {
        panic!("identical retry must rejoin");
    };
    assert_eq!(rejoined, applied);

    // The same command with a different digest conflicts.
    let mut conflicted = approve.clone();
    conflicted.target_digest = [9; 32];
    assert_eq!(
        vault
            .record_conversation_interaction_decision(conflicted)
            .await,
        Err(AgentFailure::Conflict)
    );

    // A different decision races through CAS and loses on the old revision.
    let mut stale = decision_for(&created, InteractionDecisionKind::Deny, NOW + 2);
    stale.interaction_revision = 1;
    assert_eq!(
        vault.record_conversation_interaction_decision(stale).await,
        Err(AgentFailure::Conflict)
    );

    // Resolving only accepts a cancelling dismiss.
    let mut second_approve = decision_for(&applied, InteractionDecisionKind::Approve, NOW + 2);
    second_approve.interaction_revision = applied.revision;
    assert_eq!(
        vault
            .record_conversation_interaction_decision(second_approve)
            .await,
        Err(AgentFailure::Conflict)
    );
    let mut dismiss = decision_for(&applied, InteractionDecisionKind::Dismiss, NOW + 3);
    dismiss.interaction_revision = applied.revision;
    let DecisionAdmission::Applied(cancelled) = vault
        .record_conversation_interaction_decision(dismiss)
        .await
        .unwrap()
    else {
        panic!("dismiss during resolving must apply");
    };
    assert!(matches!(
        cancelled.state,
        InteractionState::Cancelled { .. }
    ));
}

#[tokio::test]
async fn decision_rejects_expired_review() {
    let (_root, vault, person_id, _keys, session_id) = setup().await;
    let run_id = admit_run(&vault, person_id, session_id, 0).await;
    let published = record(person_id, session_id, run_id, Uuid::new_v4(), NOW);
    let PublishAdmission::Created(created) = vault
        .publish_conversation_interaction(published)
        .await
        .unwrap()
    else {
        panic!("publish must create");
    };

    let lapsed = decision_for(
        &created,
        InteractionDecisionKind::Approve,
        created.expires_at_unix_ms,
    );
    assert_eq!(
        vault.record_conversation_interaction_decision(lapsed).await,
        Err(AgentFailure::Conflict)
    );
    let stored = vault
        .conversation_interaction(created.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.state, InteractionState::Pending);
}

#[tokio::test]
async fn resolution_binds_the_recorded_decision() {
    let (_root, vault, person_id, _keys, session_id) = setup().await;
    let run_id = admit_run(&vault, person_id, session_id, 0).await;
    let published = record(person_id, session_id, run_id, Uuid::new_v4(), NOW);
    let PublishAdmission::Created(created) = vault
        .publish_conversation_interaction(published)
        .await
        .unwrap()
    else {
        panic!("publish must create");
    };
    let approve = decision_for(&created, InteractionDecisionKind::Approve, NOW + 1);
    let DecisionAdmission::Applied(applied) = vault
        .record_conversation_interaction_decision(approve.clone())
        .await
        .unwrap()
    else {
        panic!("decision must apply");
    };
    let InteractionState::Resolving {
        decision_id,
        owner_operation_id,
    } = applied.state
    else {
        panic!("approve must move to resolving");
    };

    let wrong_operation = InteractionResolution {
        interaction_id: created.id,
        person_id,
        expected_revision: applied.revision,
        decision_id,
        owner_operation_id: Uuid::new_v4(),
        resolved_at_unix_ms: NOW + 2,
    };
    assert_eq!(
        vault
            .resolve_conversation_interaction(wrong_operation)
            .await,
        Err(AgentFailure::Conflict)
    );

    let stale_revision = InteractionResolution {
        interaction_id: created.id,
        person_id,
        expected_revision: 1,
        decision_id,
        owner_operation_id,
        resolved_at_unix_ms: NOW + 2,
    };
    assert_eq!(
        vault.resolve_conversation_interaction(stale_revision).await,
        Err(AgentFailure::Conflict)
    );

    let resolution = InteractionResolution {
        interaction_id: created.id,
        person_id,
        expected_revision: applied.revision,
        decision_id,
        owner_operation_id,
        resolved_at_unix_ms: NOW + 2,
    };
    let resolved = vault
        .resolve_conversation_interaction(resolution.clone())
        .await
        .unwrap();
    assert!(matches!(resolved.state, InteractionState::Resolved { .. }));

    assert_eq!(
        vault.resolve_conversation_interaction(resolution).await,
        Err(AgentFailure::Conflict)
    );
}

#[tokio::test]
async fn supersede_and_expire_follow_terminal_rules() {
    let (_root, vault, person_id, _keys, session_id) = setup().await;
    let run_id = admit_run(&vault, person_id, session_id, 0).await;
    let published = record(person_id, session_id, run_id, Uuid::new_v4(), NOW);
    let PublishAdmission::Created(created) = vault
        .publish_conversation_interaction(published)
        .await
        .unwrap()
    else {
        panic!("publish must create");
    };

    let replacement = Uuid::new_v4();
    let superseded = vault
        .supersede_conversation_interaction(floe_conversation::SupersedeInteraction {
            interaction_id: created.id,
            person_id,
            expected_revision: 1,
            superseded_by: Some(replacement),
        })
        .await
        .unwrap();
    assert_eq!(
        superseded.state,
        InteractionState::Superseded {
            superseded_by: Some(replacement)
        }
    );
    assert_eq!(
        vault
            .supersede_conversation_interaction(floe_conversation::SupersedeInteraction {
                interaction_id: created.id,
                person_id,
                expected_revision: superseded.revision,
                superseded_by: None,
            })
            .await,
        Err(AgentFailure::Conflict)
    );

    let expiring = record(person_id, session_id, run_id, Uuid::new_v4(), NOW);
    let PublishAdmission::Created(pending) = vault
        .publish_conversation_interaction(expiring)
        .await
        .unwrap()
    else {
        panic!("publish must create");
    };
    let early = vault
        .expire_conversation_interaction(floe_conversation::ExpireInteraction {
            interaction_id: pending.id,
            person_id,
            now_unix_ms: NOW,
        })
        .await
        .unwrap();
    assert!(matches!(
        early,
        floe_conversation::ExpireOutcome::NotExpired(_)
    ));
    let expired = vault
        .expire_conversation_interaction(floe_conversation::ExpireInteraction {
            interaction_id: pending.id,
            person_id,
            now_unix_ms: pending.expires_at_unix_ms,
        })
        .await
        .unwrap();
    assert!(matches!(
        expired,
        floe_conversation::ExpireOutcome::Expired(_)
    ));
    let again = vault
        .expire_conversation_interaction(floe_conversation::ExpireInteraction {
            interaction_id: pending.id,
            person_id,
            now_unix_ms: pending.expires_at_unix_ms + 1,
        })
        .await
        .unwrap();
    assert!(matches!(
        again,
        floe_conversation::ExpireOutcome::AlreadyTerminal(_)
    ));
}

#[tokio::test]
async fn reopen_recovers_every_state_and_rejoins_decisions() {
    let (root, vault, person_id, keys, session_id) = setup().await;
    let run_id = admit_run(&vault, person_id, session_id, 0).await;

    let publish = async |vault: &EncryptedAgentVault<Keys>, created_at: i64| {
        let candidate = record(person_id, session_id, run_id, Uuid::new_v4(), created_at);
        let PublishAdmission::Created(created) = vault
            .publish_conversation_interaction(candidate)
            .await
            .unwrap()
        else {
            panic!("publish must create");
        };
        created
    };

    let pending = publish(&vault, NOW).await;
    let resolving = publish(&vault, NOW + 1).await;
    let approve_resolving = decision_for(&resolving, InteractionDecisionKind::Approve, NOW + 10);
    let DecisionAdmission::Applied(resolving) = vault
        .record_conversation_interaction_decision(approve_resolving.clone())
        .await
        .unwrap()
    else {
        panic!("decision must apply");
    };

    let to_resolve = publish(&vault, NOW + 2).await;
    let approve = decision_for(&to_resolve, InteractionDecisionKind::Approve, NOW + 10);
    let DecisionAdmission::Applied(applied) = vault
        .record_conversation_interaction_decision(approve.clone())
        .await
        .unwrap()
    else {
        panic!("decision must apply");
    };
    let InteractionState::Resolving {
        decision_id,
        owner_operation_id,
    } = applied.state
    else {
        panic!("approve must move to resolving");
    };
    let resolved = vault
        .resolve_conversation_interaction(InteractionResolution {
            interaction_id: to_resolve.id,
            person_id,
            expected_revision: applied.revision,
            decision_id,
            owner_operation_id,
            resolved_at_unix_ms: NOW + 11,
        })
        .await
        .unwrap();
    assert!(matches!(resolved.state, InteractionState::Resolved { .. }));

    let to_deny = publish(&vault, NOW + 3).await;
    let deny = decision_for(&to_deny, InteractionDecisionKind::Deny, NOW + 10);
    let DecisionAdmission::Applied(denied) = vault
        .record_conversation_interaction_decision(deny)
        .await
        .unwrap()
    else {
        panic!("decision must apply");
    };

    let to_cancel = publish(&vault, NOW + 4).await;
    let dismiss = decision_for(&to_cancel, InteractionDecisionKind::Dismiss, NOW + 10);
    let DecisionAdmission::Applied(cancelled) = vault
        .record_conversation_interaction_decision(dismiss)
        .await
        .unwrap()
    else {
        panic!("decision must apply");
    };

    let to_supersede = publish(&vault, NOW + 5).await;
    let superseded = vault
        .supersede_conversation_interaction(floe_conversation::SupersedeInteraction {
            interaction_id: to_supersede.id,
            person_id,
            expected_revision: 1,
            superseded_by: None,
        })
        .await
        .unwrap();

    let to_expire = publish(&vault, NOW + 6).await;
    let expired = vault
        .expire_conversation_interaction(floe_conversation::ExpireInteraction {
            interaction_id: to_expire.id,
            person_id,
            now_unix_ms: to_expire.expires_at_unix_ms,
        })
        .await
        .unwrap();
    assert!(matches!(
        expired,
        floe_conversation::ExpireOutcome::Expired(_)
    ));
    drop(vault);

    let vault = EncryptedAgentVault::open(root.path(), person_id, keys)
        .await
        .unwrap();
    assert_eq!(
        reopened(&vault, pending.id).await.state,
        InteractionState::Pending
    );
    assert_eq!(reopened(&vault, resolving.id).await, resolving);
    assert!(matches!(
        reopened(&vault, resolved.id).await.state,
        InteractionState::Resolved { .. }
    ));
    assert!(matches!(
        reopened(&vault, denied.id).await.state,
        InteractionState::Denied { .. }
    ));
    assert!(matches!(
        reopened(&vault, cancelled.id).await.state,
        InteractionState::Cancelled { .. }
    ));
    assert!(matches!(
        reopened(&vault, superseded.id).await.state,
        InteractionState::Superseded { .. }
    ));
    assert_eq!(
        reopened(&vault, to_expire.id).await.state,
        InteractionState::Expired
    );

    // The recorded decision survives the restart: an identical retry rejoins.
    let DecisionAdmission::Rejoined(rejoined) = vault
        .record_conversation_interaction_decision(approve_resolving)
        .await
        .unwrap()
    else {
        panic!("identical retry must rejoin after reopen");
    };
    assert_eq!(rejoined, resolving);

    let listed = vault.run_conversation_interactions(run_id).await.unwrap();
    assert_eq!(listed.len(), 7);
    assert!(
        listed
            .windows(2)
            .all(|pair| pair[0].created_at_unix_ms <= pair[1].created_at_unix_ms)
    );
}

#[tokio::test]
async fn corrupt_rows_fail_closed_as_storage_failure() {
    let (_root, vault, person_id, _keys, session_id) = setup().await;
    let run_id = admit_run(&vault, person_id, session_id, 0).await;
    let published = record(person_id, session_id, run_id, Uuid::new_v4(), NOW);
    let PublishAdmission::Created(created) = vault
        .publish_conversation_interaction(published)
        .await
        .unwrap()
    else {
        panic!("publish must create");
    };

    let connection = vault.connection().unwrap();
    connection
        .execute(
            "UPDATE agent_conversation_interactions SET payload = ? WHERE interaction_id = ?",
            ("{broken json", created.id.to_string()),
        )
        .await
        .unwrap();
    assert_eq!(
        vault.conversation_interaction(created.id).await,
        Err(AgentFailure::VaultUnavailable)
    );

    let second = record(person_id, session_id, run_id, Uuid::new_v4(), NOW + 1);
    let PublishAdmission::Created(created_second) = vault
        .publish_conversation_interaction(second)
        .await
        .unwrap()
    else {
        panic!("publish must create");
    };
    connection
        .execute(
            "UPDATE agent_conversation_interactions SET state = 'resolved' WHERE interaction_id = ?",
            [created_second.id.to_string()],
        )
        .await
        .unwrap();
    assert_eq!(
        vault.conversation_interaction(created_second.id).await,
        Err(AgentFailure::VaultUnavailable)
    );
}
