use super::*;

#[tokio::test]
async fn terminal_history_does_not_block_current_session_interactions() {
    let mut scenario = Scenario::new().await;
    let (first, _) = scenario.admit_run_and_append_input("first run").await;
    let first = scenario.complete_owner_run(&first).await;
    let (second, _) = scenario.admit_run_and_append_input("second run").await;
    let second = scenario.complete_owner_run(&second).await;
    let mut connection = scenario.vault().connection().expect("test connection");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("begin interaction fixture");
    for _ in 0..64 {
        let interaction = resolved_binding_interaction(&first);
        super::super::super::conversation_interactions::insert_interaction(
            &transaction,
            &interaction,
        )
        .await
        .expect("store terminal history within per-Run cap");
    }
    let mut pending = resolved_binding_interaction(&second);
    pending.state = floe_conversation::InteractionState::Pending;
    pending.revision = 1;
    super::super::super::conversation_interactions::insert_interaction(&transaction, &pending)
        .await
        .expect("store current pending interaction");
    transaction
        .commit()
        .await
        .expect("commit interaction fixture");
    drop(connection);
    let result = scenario
        .vault()
        .session_conversation_interactions(scenario.session_id, &first.device_id, None)
        .await;
    assert!(
        result.is_ok(),
        "terminal lifetime history must not exhaust a per-Run budget: {result:?}"
    );
    let page = result.unwrap();
    assert_eq!(page.interactions.len(), 64);
    assert!(page.next_cursor.is_some());
    let records = &page.interactions;
    assert_eq!(
        records.first().map(|r| r.id),
        Some(pending.id),
        "active review precedes terminal history"
    );
    let next = scenario
        .vault()
        .session_conversation_interactions(scenario.session_id, &first.device_id, page.next_cursor)
        .await
        .expect("read terminal tail page");
    assert_eq!(next.interactions.len(), 1);
    assert!(next.next_cursor.is_none());
    assert!(!records.iter().any(|r| r.id == next.interactions[0].id));
    let replay = scenario
        .vault()
        .conversation_interaction(pending.id)
        .await
        .expect("exact reference survives paging")
        .unwrap();
    assert_eq!(replay, pending);
    let foreign = scenario
        .vault()
        .session_conversation_interactions(scenario.session_id, "foreign-device", None)
        .await
        .expect("device-scoped empty page");
    assert!(foreign.interactions.is_empty());
}

#[tokio::test]
async fn page_boundary_keeps_its_observed_group_after_cursor_interaction_expires() {
    let mut scenario = Scenario::new().await;
    let mut device = String::new();
    for index in 0..9 {
        let (run, _) = scenario
            .admit_run_and_append_input(&format!("run {index}"))
            .await;
        let run = scenario.complete_owner_run(&run).await;
        device = run.device_id.clone();
        let mut connection = scenario.vault().connection().expect("test connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .expect("begin fixture");
        for _ in 0..8 {
            let mut interaction = resolved_binding_interaction(&run);
            interaction.state = floe_conversation::InteractionState::Pending;
            interaction.revision = 1;
            super::super::super::conversation_interactions::insert_interaction(
                &transaction,
                &interaction,
            )
            .await
            .expect("store within active per-Run budget");
        }
        transaction.commit().await.expect("commit fixture");
    }
    let page = scenario
        .vault()
        .session_conversation_interactions(scenario.session_id, &device, None)
        .await
        .unwrap();
    assert_eq!(page.interactions.len(), 64);
    let cursor = page.next_cursor.expect("explicit remaining active reviews");
    assert!(!cursor.terminal);
    scenario
        .vault()
        .expire_conversation_interaction(floe_conversation::ExpireInteraction {
            interaction_id: cursor.interaction_id,
            person_id: scenario.person_id,
            now_unix_ms: 1 + floe_conversation::INTERACTION_PENDING_LIFETIME_MS,
        })
        .await
        .expect("terminal transition after first page");
    let next = scenario
        .vault()
        .session_conversation_interactions(scenario.session_id, &device, Some(cursor))
        .await
        .unwrap();
    assert_eq!(
        next.interactions
            .iter()
            .filter(|r| !r.state.is_terminal())
            .count(),
        8,
        "cursor must not jump past still-active rows after its own record transitions"
    );
    assert_eq!(
        next.interactions.last().map(|r| r.id),
        Some(cursor.interaction_id)
    );
    assert!(next.next_cursor.is_none());
    let refreshed = scenario
        .vault()
        .session_conversation_interactions(scenario.session_id, &device, None)
        .await
        .unwrap();
    assert!(
        refreshed
            .interactions
            .iter()
            .all(|r| !r.state.is_terminal())
    );
    let invalid = scenario
        .vault()
        .session_conversation_interactions(
            scenario.session_id,
            &device,
            Some(floe_conversation::InteractionListCursor {
                interaction_id: Uuid::nil(),
                ..cursor
            }),
        )
        .await;
    assert!(matches!(invalid, Err(AgentFailure::InvalidInput)));
}
