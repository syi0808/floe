use super::*;
use floe_kernel::{CommandId, OwnerActor, PersonId};
fn completed() -> RunRecord {
    RunRecord {
        expert_environment: floe_experts::RunExpertEnvironmentIdentity {
            revision: 1,
            digest: [7; 32],
        },
        run_id: RunId::new(),
        command_id: CommandId::new(),
        session_id: Uuid::new_v4(),
        person_id: PersonId::new(),
        device_id: "synthetic-device".into(),
        user_message_id: Uuid::new_v4(),
        initial_session_revision: 0,
        request_digest: [9; 32],
        state: RunState::Completed,
        pending_terminal: None,
        output: Some("Synthetic result".into()),
        coverage: floe_agent_contract::DependencyCoverage::Independent,
        issue: None,
        blocked: None,
        session_revision: 2,
        aggregate_revision: 2,
        journal_revision: 1,
        executor_generation: 1,
        continuation_of: None,
        continuation_executor_generation: None,
        continuation_level: 0,
        retry_of: None,
        resume_of: None,
        resume_lineage: 0,
    }
}
fn actor() -> OwnerActor {
    OwnerActor {
        person_id: PersonId::new(),
        device_id: "test-device".into(),
        runtime_epoch: 1,
    }
}
fn input(
    actor: &OwnerActor,
    before: SurfaceAnchor,
    text: &str,
    origin: SurfaceInputOrigin,
) -> SurfaceInputReceipt {
    crate::AppendSurfaceInput {
        command_id: CommandId::new(),
        expected: before,
        input: crate::SurfaceInput {
            message_id: MessageId::new(),
            text: text.into(),
            origin,
        },
    }
    .admit(actor)
    .unwrap()
}
#[test]
fn surface_projection_rejects_wrong_owner_cursor_and_malformed_suffix() {
    let actor = actor();
    let empty = SurfaceAnchor::empty(Uuid::new_v4()).unwrap();
    let first = input(&actor, empty, "one", SurfaceInputOrigin::Text);
    let second = input(&actor, first.through, "two", SurfaceInputOrigin::Text);
    let one = SurfaceMessage::input(&first).unwrap();
    let two = SurfaceMessage::input(&second).unwrap();
    assert_eq!(
        SurfaceSnapshot::from_suffix(empty, vec![]).unwrap().older,
        None
    );
    assert_eq!(
        SurfaceSnapshot::from_suffix(second.through, vec![two.clone()])
            .unwrap()
            .older,
        Some(first.through)
    );
    assert!(SurfaceSnapshot::from_suffix(second.through, vec![]).is_err());
    assert!(SurfaceSnapshot::from_suffix(second.through, vec![two.clone(), one.clone()]).is_err());
    let mut bad = two.clone();
    bad.before = empty;
    assert!(SurfaceSnapshot::from_suffix(second.through, vec![one.clone(), bad]).is_err());
    let mut bad = two.clone();
    bad.message_id = one.message_id;
    assert!(SurfaceSnapshot::from_suffix(second.through, vec![one.clone(), bad]).is_err());
    let mut bad = two;
    bad.content = SurfaceMessageContent::Input {
        text: "  ".into(),
        origin: SurfaceInputKind::Text,
    };
    assert!(SurfaceSnapshot::from_suffix(second.through, vec![one, bad]).is_err());
    for limit in [0, MAX_SURFACE_PAGE_ITEMS + 1] {
        assert!(
            SurfaceReadRequest {
                session_id: empty.session_id,
                through: None,
                limit
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        SurfaceReadRequest {
            session_id: Uuid::new_v4(),
            through: Some(first.through),
            limit: 1
        }
        .validate()
        .is_err()
    );
}
#[test]
fn voice_projection_keeps_original_text_without_provider_identity() {
    let actor = actor();
    let voice = Uuid::new_v4();
    let receipt = input(
        &actor,
        SurfaceAnchor::empty(Uuid::new_v4()).unwrap(),
        "Original spoken words",
        SurfaceInputOrigin::Voice {
            voice_session_id: voice,
            utterance_id: "provider-private-utterance".into(),
        },
    );
    let message = SurfaceMessage::input(&receipt).unwrap();
    assert_eq!(
        message.content,
        SurfaceMessageContent::Input {
            text: receipt.input.text,
            origin: SurfaceInputKind::Voice
        }
    );
    let json = serde_json::to_string(&message).unwrap();
    assert!(!json.contains(&voice.to_string()));
    assert!(!json.contains("provider-private"));
    assert!(!json.contains("device_id"));
    assert!(!json.contains("person_id"));
}
#[test]
fn full_size_partial_body_stays_original_and_is_bound_to_canonical_run() {
    let mut run = completed();
    run.state = RunState::Failed;
    run.issue = Some(AgentFailure::BudgetExceeded);
    run.output = Some("x".repeat(floe_agent_contract::MAX_OUTPUT_BYTES));
    let actor = OwnerActor {
        person_id: run.person_id,
        device_id: run.device_id.clone(),
        runtime_epoch: 1,
    };
    let status = SurfaceOutcomeReceipt::admit(
        &actor,
        SurfaceAnchor::empty(run.session_id).unwrap(),
        SurfaceOutcomeReference::for_run(&run).unwrap(),
    )
    .unwrap();
    let result = SurfaceResultReceipt::admit(
        &actor,
        status.through,
        SurfaceResultReference::for_run(&run).unwrap(),
    )
    .unwrap();
    let messages = vec![
        SurfaceMessage::outcome(&status, &run).unwrap(),
        SurfaceMessage::result(&result, &run).unwrap(),
    ];
    let page = SurfaceSnapshot::from_suffix(result.through, messages).unwrap();
    assert_eq!(
        page.messages[1].content,
        SurfaceMessageContent::Result {
            run_id: run.run_id,
            completeness: SurfaceResultCompleteness::Partial,
            text: run.output.clone().unwrap()
        }
    );
    for changed in [0, 1, 2] {
        let mut other = run.clone();
        match changed {
            0 => other.person_id = PersonId::new(),
            1 => other.device_id = "another-device".into(),
            _ => other.session_id = Uuid::new_v4(),
        }
        assert!(SurfaceMessage::result(&result, &other).is_err());
        assert!(SurfaceMessage::outcome(&status, &other).is_err());
    }
    run.output = Some("different".into());
    assert!(SurfaceMessage::result(&result, &run).is_err());
    assert!(SurfaceMessage::outcome(&status, &run).is_err());
}
#[test]
fn byte_bounded_pages_keep_whole_bodies_and_reconstruct_original_history() {
    let mut run = completed();
    run.output = Some("x".repeat(floe_agent_contract::MAX_OUTPUT_BYTES));
    let actor = OwnerActor {
        person_id: run.person_id,
        device_id: run.device_id.clone(),
        runtime_epoch: 1,
    };
    let mut through = SurfaceAnchor::empty(run.session_id).unwrap();
    let mut all = vec![];
    for _ in 0..40 {
        run.run_id = RunId::new();
        let receipt = SurfaceResultReceipt::admit(
            &actor,
            through,
            SurfaceResultReference::for_run(&run).unwrap(),
        )
        .unwrap();
        through = receipt.through;
        all.push(SurfaceMessage::result(&receipt, &run).unwrap());
    }
    let latest = SurfaceSnapshot::from_suffix(through, all.clone()).unwrap();
    assert!(latest.messages.len() < all.len());
    assert!(serde_json::to_vec(&latest).unwrap().len() <= MAX_SURFACE_SNAPSHOT_BYTES);
    let older = latest.older.unwrap();
    let previous =
        SurfaceSnapshot::from_suffix(older, all[..older.sequence as usize].to_vec()).unwrap();
    assert!(previous.older.is_none());
    let mut reconstructed = previous.messages;
    reconstructed.extend(latest.messages);
    assert_eq!(
        reconstructed, all,
        "no clipping, summary replacement, duplicate or missing entry"
    );
}
