//! Product New-session order, independent of accumulated transcript revision.
use super::*;

async fn new_product_session(
    scenario: &Scenario,
) -> (CommandId, floe_conversation::SessionReceipt) {
    let command_id = CommandId::new();
    let admitted = scenario
        .vault()
        .start_conversation_session(StartSessionRequest {
            principal: scenario.person_id.to_string(),
            command_id,
        })
        .await
        .unwrap();
    let SessionStartAdmission::Started(receipt) = admitted else {
        panic!("new product session expected");
    };
    (command_id, receipt)
}

#[tokio::test]
async fn manager_resume_prefers_new_pristine_session_over_old_bound_history() {
    let mut scenario = Scenario::new().await;
    let old = scenario.session_id;
    let (run, _) = scenario
        .admit_run_and_append_input("Old conversation")
        .await;
    let (_, new) = new_product_session(&scenario).await;
    // A previous Session's later settlement cannot steal the New selection.
    scenario.cancel_owner_run(&run).await;
    assert_ne!(old, new.session_id);
    assert!(scenario.session_revision > new.session_revision);
    let resumed = scenario
        .vault()
        .resume_manager_conversation_session(&scenario.identity)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        resumed.id, new.session_id,
        "New gesture must survive reopening before the first message"
    );
    scenario.reopen().await;
    assert_eq!(
        scenario
            .vault()
            .resume_manager_conversation_session(&scenario.identity)
            .await
            .unwrap()
            .unwrap()
            .id,
        new.session_id
    );
}

#[tokio::test]
async fn manager_resume_uses_selection_order_not_largest_revision() {
    let mut scenario = Scenario::new().await;
    for _ in 0..3 {
        let (run, _) = scenario
            .admit_run_and_append_input("Older lengthy conversation")
            .await;
        scenario.cancel_owner_run(&run).await;
    }
    let old_revision = scenario.session_revision;
    let (new_command, new) = new_product_session(&scenario).await;
    scenario.session_id = new.session_id;
    scenario.session_revision = new.session_revision;
    scenario.conversation_id = ConversationId::new();
    scenario.branch_id = ConversationBranchId::new();
    scenario.last_core_input_request = None;
    scenario.last_core_recorder = None;
    let (run, _) = scenario
        .admit_run_and_append_input("New short conversation")
        .await;
    scenario.cancel_owner_run(&run).await;
    assert!(old_revision > scenario.session_revision);
    scenario.reopen().await;
    assert_eq!(
        scenario
            .vault()
            .resume_manager_conversation_session(&scenario.identity)
            .await
            .unwrap()
            .unwrap()
            .id,
        new.session_id
    );
    let (_, newest) = new_product_session(&scenario).await;
    assert!(matches!(
        scenario
            .vault()
            .start_conversation_session(StartSessionRequest {
                principal: scenario.person_id.to_string(),
                command_id: new_command,
            })
            .await
            .unwrap(),
        SessionStartAdmission::Replayed(_)
    ));
    assert_eq!(
        scenario
            .vault()
            .resume_manager_conversation_session(&scenario.identity)
            .await
            .unwrap()
            .unwrap()
            .id,
        newest.session_id,
        "old Start acknowledgement must not steal the newer selection"
    );
}

#[tokio::test]
async fn manager_resume_keeps_exact_identity_and_never_rebinds_newer_history() {
    let mut scenario = Scenario::new().await;
    let old_identity = scenario.identity.clone();
    let old_session = scenario.session_id;
    let (run, _) = scenario
        .admit_run_and_append_input("Original Manager policy")
        .await;
    scenario.cancel_owner_run(&run).await;
    let (_, new) = new_product_session(&scenario).await;
    scenario.session_id = new.session_id;
    scenario.session_revision = 0;
    scenario.conversation_id = ConversationId::new();
    scenario.branch_id = ConversationBranchId::new();
    scenario.identity.definition_revision += 1;
    let (run, _) = scenario
        .admit_run_and_append_input("New pinned Manager policy")
        .await;
    scenario.cancel_owner_run(&run).await;
    scenario.reopen().await;
    for _ in 0..2 {
        assert_eq!(
            scenario
                .vault()
                .resume_manager_conversation_session(&old_identity)
                .await
                .unwrap()
                .unwrap()
                .id,
            old_session
        );
        assert_eq!(
            scenario
                .vault()
                .resume_manager_conversation_session(&scenario.identity)
                .await
                .unwrap()
                .unwrap()
                .id,
            new.session_id
        );
    }
    let mut absent = scenario.identity.clone();
    absent.definition_revision += 1;
    assert!(
        scenario
            .vault()
            .resume_manager_conversation_session(&absent)
            .await
            .unwrap()
            .is_none()
    );
    absent.person_id = PersonId::new();
    assert_eq!(
        scenario
            .vault()
            .resume_manager_conversation_session(&absent)
            .await
            .unwrap_err(),
        AgentFailure::CapabilityDenied
    );
    let connection = scenario.vault().connection().unwrap();
    let mut rows = connection
        .query(
            "SELECT count(*), max(selection_sequence) FROM agent_conversation_session_commands",
            (),
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    assert_eq!(row.get::<i64>(0).unwrap(), 2);
    assert_eq!(
        row.get::<i64>(1).unwrap(),
        2,
        "observations never advance selection"
    );
}

#[tokio::test]
async fn manager_resume_rejects_nonpristine_unbound_state() {
    let scenario = Scenario::new().await;
    let connection = scenario.vault().connection().unwrap();
    connection.execute("UPDATE agent_sessions SET revision = 1, payload = json_set(payload, '$.revision', 1) WHERE id = ?", [scenario.session_id.to_string()]).await.unwrap();
    assert!(
        scenario
            .vault()
            .resume_manager_conversation_session(&scenario.identity)
            .await
            .is_err(),
        "unbound history must not be retargeted as pristine"
    );
}

#[tokio::test]
async fn manager_resume_and_start_reject_invalid_selection_order_without_repair() {
    // An integrity failure retires the store. Use independent fixtures so
    // both entry points actually inspect the corrupt ordering themselves.
    for start_first in [false, true] {
        let scenario = Scenario::new().await;
        let connection = scenario.vault().connection().unwrap();
        connection
            .execute(
                "UPDATE agent_conversation_session_commands SET selection_sequence = 9",
                (),
            )
            .await
            .unwrap();
        if start_first {
            let start = scenario
                .vault()
                .start_conversation_session(StartSessionRequest {
                    principal: scenario.person_id.to_string(),
                    command_id: CommandId::new(),
                })
                .await;
            assert!(
                matches!(
                    start,
                    Err(floe_conversation::SessionStartFailure::Indeterminate(
                        AgentFailure::VaultUnavailable
                    ))
                ),
                "invalid order Start outcome: {start:?}"
            );
        } else {
            assert_eq!(
                scenario
                    .vault()
                    .resume_manager_conversation_session(&scenario.identity)
                    .await
                    .unwrap_err(),
                AgentFailure::VaultUnavailable
            );
        }
        let mut rows = connection
            .query(
                "SELECT count(*), max(selection_sequence) FROM agent_conversation_session_commands",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        assert_eq!(row.get::<i64>(0).unwrap(), 1);
        assert_eq!(
            row.get::<i64>(1).unwrap(),
            9,
            "corruption must not be silently repaired"
        );
    }
}
