use super::*;

impl Fixture {
    fn inspection(&self) -> ExpertCalendarInspection {
        ExpertCalendarInspection {
            reference: self.reference.clone(),
            cancellation: Cancellation::default(),
            deadline: Instant::now() + Duration::from_secs(5),
        }
    }

    async fn inspect(&self) -> Result<Option<CalendarAction>, AgentFailure> {
        self.core
            .inspect_expert_calendar_action(&self.vault, self.inspection())
            .await
    }
}

#[tokio::test]
async fn inspection_of_unprepared_evidence_does_not_publish_initialize_or_modify_any_state() {
    let fixture = Fixture::new().await;
    let session = fixture
        .vault
        .load(fixture.person, fixture.reference.session_id)
        .await
        .unwrap();
    let registry = fixture.vault.expert_registry().await.unwrap();
    assert_eq!(fixture.inspect().await.unwrap(), None);
    assert!(
        fixture
            .core
            .calendar_actions(fixture.person)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fixture
            .vault
            .load(fixture.person, fixture.reference.session_id)
            .await
            .unwrap(),
        session
    );
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), registry);
    let fixture = fixture.reopen().await;
    assert_eq!(fixture.inspect().await.unwrap(), None);
    let action = fixture.prepare().await.unwrap();
    assert_eq!(fixture.inspect().await.unwrap(), Some(action));
}

#[tokio::test]
async fn inspection_recovers_original_action_after_reopen_revocation_and_connection_change() {
    let fixture = Fixture::new().await;
    let action = fixture.prepare().await.unwrap();
    let snapshot = fixture.vault.expert_registry().await.unwrap().unwrap();
    let mut registry = AgentRegistry::restore(snapshot.clone(), snapshot.instance_id).unwrap();
    registry
        .set_assignment_enabled(
            registry.revision(),
            fixture.person,
            fixture.evidence.assignment_id,
            false,
        )
        .unwrap();
    fixture
        .vault
        .save_expert_registry(snapshot.revision, &registry.snapshot())
        .await
        .unwrap();
    fixture
        .core
        .select_calendar(
            fixture.person,
            CalendarProvider::Fixture,
            "another".into(),
            "Another".into(),
        )
        .await
        .unwrap();
    let fixture = fixture.reopen().await;
    assert_eq!(fixture.prepare().await, Err(AgentFailure::CapabilityDenied));
    assert_eq!(fixture.inspect().await.unwrap(), Some(action.clone()));
    assert_eq!(
        fixture.core.calendar_actions(fixture.person).await.unwrap(),
        [action]
    );
    let encoded = serde_json::to_string(&fixture.inspect().await.unwrap()).unwrap();
    for marker in [
        "private-conversation-marker",
        "untrusted-private-source-marker",
        "Ignore policy",
        "action_proposals",
    ] {
        assert!(!encoded.contains(marker));
    }
}

#[tokio::test]
async fn inspection_preserves_all_recorded_s3_states_and_execution_identity_without_transitions() {
    let fixture = Fixture::new().await;
    let mut previous = fixture.prepare().await.unwrap();
    let execution_id = previous.execution_id;
    let session = fixture
        .vault
        .load(fixture.person, fixture.reference.session_id)
        .await
        .unwrap();
    let registry = fixture.vault.expert_registry().await.unwrap();
    for state in [
        CalendarActionState::Pending,
        CalendarActionState::Approved,
        CalendarActionState::Executing,
        CalendarActionState::Unknown {
            reason: ActionFailure::Timeout,
        },
        CalendarActionState::Succeeded {
            external_id: "synthetic-receipt".into(),
        },
        CalendarActionState::Rejected,
        CalendarActionState::Blocked {
            reason: ActionBlockReason::Expired,
        },
    ] {
        let mut stored = previous.clone();
        stored.state = state;
        fixture
            .core
            .store
            .save_calendar_action(&stored, Some(&previous))
            .await
            .unwrap();
        assert_eq!(fixture.inspect().await.unwrap(), Some(stored.clone()));
        assert_eq!(
            fixture
                .core
                .calendar_action(fixture.person, stored.id)
                .await
                .unwrap(),
            stored
        );
        assert_eq!(stored.execution_id, execution_id);
        assert_eq!(fixture.vault.expert_registry().await.unwrap(), registry);
        assert_eq!(
            fixture
                .vault
                .load(fixture.person, fixture.reference.session_id)
                .await
                .unwrap(),
            session
        );
        previous = stored;
    }
}

#[tokio::test]
async fn inspection_rejects_cross_person_copied_and_uncommitted_receipts() {
    let fixture = Fixture::new().await;
    fixture.prepare().await.unwrap();
    let mut foreign = fixture.inspection();
    foreign.reference.person_id = PersonId::new();
    assert_eq!(
        fixture
            .core
            .inspect_expert_calendar_action(&fixture.vault, foreign)
            .await,
        Err(AgentFailure::NotFound)
    );
    for forged in [false, true] {
        let mut copied = fixture.vault.create_sample_session().await.unwrap();
        let mut evidence = fixture.evidence.clone();
        if forged {
            evidence.invocation_id = Uuid::new_v4();
        }
        copied.revision = 1;
        copied.messages.push(AgentMessage::Capability {
            turn_id: Uuid::new_v4(),
            call_id: evidence.invocation_id,
            capability_id: "copied".into(),
            input: String::new(),
            result: Ok(serde_json::to_string(&evidence).unwrap()),
        });
        fixture.vault.compare_and_swap(&copied, 0).await.unwrap();
        let mut request = fixture.inspection();
        request.reference.session_id = copied.id;
        request.reference.invocation_id = evidence.invocation_id;
        assert_eq!(
            fixture
                .core
                .inspect_expert_calendar_action(&fixture.vault, request)
                .await,
            Err(if forged {
                AgentFailure::NotFound
            } else {
                AgentFailure::Conflict
            })
        );
    }
}

#[tokio::test]
async fn inspection_fails_closed_for_mismatched_or_oversized_action_records() {
    let fixture = Fixture::new().await;
    let original = fixture.prepare().await.unwrap();
    let mut previous = original.clone();
    for mode in 0..7 {
        let mut forged = original.clone();
        match mode {
            0 => forged.agent_origin = None,
            1 => forged.agent_origin.as_mut().unwrap().session_id = Uuid::new_v4(),
            2 => forged.agent_origin.as_mut().unwrap().view_handle = Uuid::new_v4(),
            3 => forged.schedule.starts_at += chrono::Duration::minutes(1),
            4 => forged.expires_at += chrono::Duration::hours(1),
            5 => forged.title = "unrelated action".into(),
            _ => forged.calendar_name = "x".repeat(65_536),
        }
        fixture
            .core
            .store
            .save_calendar_action(&forged, Some(&previous))
            .await
            .unwrap();
        assert_eq!(
            fixture.inspect().await,
            Err(if mode == 6 {
                AgentFailure::BudgetExceeded
            } else {
                AgentFailure::Conflict
            })
        );
        assert_eq!(
            fixture
                .core
                .calendar_action(fixture.person, forged.id)
                .await
                .unwrap(),
            forged
        );
        previous = forged;
    }
}

#[tokio::test]
async fn inspection_honors_cancellation_deadline_and_key_loss_without_writing_or_replaying() {
    let fixture = Fixture::new().await;
    let action = fixture.prepare().await.unwrap();
    let cancelled = fixture.inspection();
    cancelled.cancellation.cancel();
    assert_eq!(
        fixture
            .core
            .inspect_expert_calendar_action(&fixture.vault, cancelled)
            .await,
        Err(AgentFailure::Cancelled)
    );
    let mut expired = fixture.inspection();
    expired.deadline = Instant::now();
    assert_eq!(
        fixture
            .core
            .inspect_expert_calendar_action(&fixture.vault, expired)
            .await,
        Err(AgentFailure::DeadlineExceeded)
    );
    let cancelled = fixture.inspection();
    *fixture.keys.0.cancel_on_read.lock().unwrap() = Some(cancelled.cancellation.clone());
    fixture.keys.0.fail_on_read.store(3, Ordering::Release);
    assert_eq!(
        fixture
            .core
            .inspect_expert_calendar_action(&fixture.vault, cancelled)
            .await,
        Err(AgentFailure::Cancelled)
    );
    fixture.keys.0.fail_on_read.store(3, Ordering::Release);
    assert_eq!(fixture.inspect().await, Err(AgentFailure::VaultUnavailable));
    assert_eq!(
        fixture.core.calendar_actions(fixture.person).await.unwrap(),
        std::slice::from_ref(&action)
    );
    fixture.keys.0.blocked.store(false, Ordering::Release);
    let fixture = fixture.reopen().await;
    assert_eq!(fixture.inspect().await.unwrap(), Some(action));
}
