use super::*;
use crate::vault::ManagerHistoryView;
use floe_conversation::{CompactionAdmission, CompactionAuthority};
use floe_kernel::OwnerActor;

async fn fixture() -> (Scenario, OwnerActor, RunRecord, CompactionAdmission) {
    let mut scenario = Scenario::new().await;
    for text in [
        "First complete historical input.",
        "Second complete historical input.",
        "Third complete historical input.",
    ] {
        let (run, _) = scenario.admit_run_and_append_input(text).await;
        scenario.cancel_owner_run(&run).await;
    }
    let (run, _) = scenario
        .admit_run_and_append_input("Current input remains exact.")
        .await;
    let actor = OwnerActor {
        person_id: scenario.person_id,
        device_id: run.device_id.clone(),
        runtime_epoch: 77,
    };
    let request = CompactionAdmission {
        authority: CompactionAuthority::BeforeFirstModel { run_id: run.run_id },
        session_id: scenario.session_id,
        expected_session_revision: scenario.session_revision,
        source_byte_limit: floe_conversation::COMPACTION_TRIGGER_BYTES,
        max_tokens: 1000,
        max_cost_micros: 1000,
        max_output_bytes: 1024,
    };
    (scenario, actor, run, request)
}
async fn foreground_intent(scenario: &Scenario, actor: &OwnerActor, run: &RunRecord) {
    let mut event = compaction_custody_intent(actor);
    let floe_agent_contract::JournalEvent::ModelIntent { plan, .. } = &mut event else {
        unreachable!()
    };
    plan.consumer = floe_conversation::CONVERSATION_CONSUMER.into();
    scenario
        .vault()
        .append_conversation_journal(
            run.run_id,
            "intent",
            &serde_json::to_string(&event).unwrap(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn compaction_active_prefix_preserves_current_input_run_and_recorder() {
    let (mut scenario, actor, run, request) = fixture().await;
    let mut idle = request.clone();
    idle.authority = CompactionAuthority::Idle;
    assert_eq!(
        scenario
            .vault()
            .admit_manager_compaction(&actor, idle)
            .await
            .err(),
        Some(AgentFailure::Conflict)
    );
    let before = scenario
        .vault()
        .read_manager_session_history_page(
            scenario.session_id,
            None,
            ManagerHistoryView::Transcript,
            16,
            floe_conversation::MAX_SESSION_BYTES,
        )
        .await
        .unwrap();
    let job = scenario
        .vault()
        .admit_manager_compaction(&actor, request)
        .await
        .unwrap()
        .unwrap();
    assert!(job.source.messages.len() < floe_conversation::COMPACTION_TRIGGER_MESSAGES);
    assert!(
        job.source
            .messages
            .iter()
            .all(|entry| entry.message_id != run.user_message_id)
    );
    assert_eq!(
        job.source.authority,
        CompactionAuthority::BeforeFirstModel { run_id: run.run_id }
    );
    record_compaction_answer(&scenario, &actor, &job, "Earlier inputs.").await;
    let receipt = scenario
        .vault()
        .complete_manager_compaction(&actor, job.claim)
        .await
        .unwrap();
    assert_eq!(receipt.session_revision, run.session_revision);
    let after = scenario
        .vault()
        .read_manager_session_history_page(
            scenario.session_id,
            None,
            ManagerHistoryView::Transcript,
            16,
            floe_conversation::MAX_SESSION_BYTES,
        )
        .await
        .unwrap();
    assert_eq!(before, after);
    let model = scenario
        .vault()
        .read_manager_session_history_page(
            scenario.session_id,
            None,
            ManagerHistoryView::ModelContext {
                required_user_id: Some(run.user_message_id),
            },
            16,
            floe_conversation::MAX_SESSION_BYTES,
        )
        .await
        .unwrap();
    assert!(
        model
            .messages
            .iter()
            .any(|entry| entry.alias_id == run.user_message_id)
    );
    assert!(model.messages.len() < after.messages.len());
    // The existing foreground recorder remains valid; no replacement Run or
    // User message is admitted just because its model context was compacted.
    scenario.cancel_owner_run(&run).await;
    scenario.reopen().await;
    assert_eq!(
        scenario
            .vault()
            .read_manager_session_history_page(
                scenario.session_id,
                None,
                ManagerHistoryView::Transcript,
                16,
                floe_conversation::MAX_SESSION_BYTES
            )
            .await
            .unwrap(),
        before
    );
}

#[tokio::test]
async fn compaction_active_first_intent_closes_the_window_before_dispatch() {
    let (scenario, actor, run, request) = fixture().await;
    let job = scenario
        .vault()
        .admit_manager_compaction(&actor, request.clone())
        .await
        .unwrap()
        .unwrap();
    foreground_intent(&scenario, &actor, &run).await;
    assert_eq!(
        scenario
            .vault()
            .admit_manager_compaction(&actor, request)
            .await
            .err(),
        Some(AgentFailure::Conflict)
    );
    assert!(matches!(
        scenario
            .vault()
            .append_manager_compaction(&actor, job.claim, compaction_custody_intent(&actor))
            .await,
        Err(AgentFailure::Conflict | AgentFailure::StaleContext)
    ));
    assert!(
        scenario
            .vault()
            .load_manager_compaction(&actor, job.claim)
            .await
            .unwrap()
            .entries
            .is_empty()
    );
}

#[tokio::test]
async fn compaction_active_first_intent_also_fences_checkpoint_installation() {
    let (scenario, actor, run, request) = fixture().await;
    let job = scenario
        .vault()
        .admit_manager_compaction(&actor, request)
        .await
        .unwrap()
        .unwrap();
    record_compaction_answer(&scenario, &actor, &job, "Earlier inputs.").await;
    foreground_intent(&scenario, &actor, &run).await;
    assert_eq!(
        scenario
            .vault()
            .complete_manager_compaction(&actor, job.claim)
            .await
            .err(),
        Some(AgentFailure::Conflict)
    );
}

#[tokio::test]
async fn compaction_active_authority_rejects_another_device_and_cancelled_run() {
    let (mut scenario, actor, run, request) = fixture().await;
    let mut other = actor.clone();
    other.device_id = "another-device".into();
    assert_eq!(
        scenario
            .vault()
            .admit_manager_compaction(&other, request.clone())
            .await
            .err(),
        Some(AgentFailure::PolicyDenied)
    );
    scenario.cancel_owner_run(&run).await;
    assert_eq!(
        scenario
            .vault()
            .admit_manager_compaction(&actor, request)
            .await
            .err(),
        Some(AgentFailure::Conflict)
    );
}
