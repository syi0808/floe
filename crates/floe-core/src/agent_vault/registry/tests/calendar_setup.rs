use super::*;

fn setup_request(fixture: &Fixture, revision: u64) -> CalendarExpertSetup {
    CalendarExpertSetup {
        instance_id: fixture.vault.registry_instance_id(),
        expected_revision: revision,
        setup_id: Uuid::new_v4(),
        provider: floe_domain::CalendarProvider::EventKit,
        calendar_ids: vec!["setup-private-calendar-canary".into()],
    }
}

#[tokio::test]
async fn setup_bootstraps_without_a_sample_and_replays_after_reopen_without_resurrection() {
    let mut fixture = Fixture::new().await;
    let request = setup_request(&fixture, 0);
    let installed = fixture
        .vault
        .install_calendar_expert(request.clone(), Cancellation::default())
        .await
        .unwrap();
    assert_eq!(installed.registry.revision, 1);
    assert_eq!(installed.registry.installations.len(), 2);
    assert_eq!(installed.registry.assignments.len(), 2);
    assert!(
        installed
            .registry
            .installations
            .iter()
            .all(|entry| !entry.enabled)
    );
    assert!(
        installed
            .registry
            .assignments
            .iter()
            .all(|entry| !entry.enabled)
    );
    let before = fixture.vault.expert_registry().await.unwrap().unwrap();
    assert!(!before.calendar_views[0].enabled);
    assert_eq!(fixture.receipts().await, 0);
    let mut sessions = fixture
        .vault
        .connection()
        .unwrap()
        .query("SELECT count(*) FROM agent_sessions", ())
        .await
        .unwrap();
    assert_eq!(
        sessions
            .next()
            .await
            .unwrap()
            .unwrap()
            .get::<i64>(0)
            .unwrap(),
        0
    );
    drop(sessions);
    let mut registry = AgentRegistry::restore(before, request.instance_id).unwrap();
    for enabled in [true, false] {
        let revision = registry.revision();
        registry
            .set_calendar_view_enabled(
                revision,
                fixture.person,
                installed.setup.view_handle,
                enabled,
            )
            .unwrap();
        fixture
            .vault
            .save_expert_registry(revision, &registry.snapshot())
            .await
            .unwrap();
    }
    fixture.sample().await;
    let before = fixture.vault.expert_registry().await.unwrap().unwrap();
    fixture.vault.checkpoint().await.unwrap();
    drop(fixture.vault);
    fixture.vault =
        EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
            .await
            .unwrap();
    let replay = fixture
        .vault
        .install_calendar_expert(request, Cancellation::default())
        .await
        .unwrap();
    assert_eq!(replay.setup, installed.setup);
    assert_eq!(replay.registry.revision, before.revision);
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), Some(before));
    assert_eq!(fixture.receipts().await, 1);
    for entry in fs::read_dir(fixture.root.path().join(fixture.person.to_string())).unwrap() {
        let bytes = fs::read(entry.unwrap().path()).unwrap();
        for marker in ["setup-private-calendar-canary", "calendar_setups"] {
            assert!(
                !bytes
                    .windows(marker.len())
                    .any(|window| window == marker.as_bytes())
            );
        }
    }
}

#[tokio::test]
async fn setup_and_sample_coexist_in_both_orders_without_regranting_disabled_sample() {
    for sample_first in [false, true] {
        let fixture = Fixture::new().await;
        let original = if sample_first {
            Some(expert(&fixture.sample().await))
        } else {
            None
        };
        let revision = fixture
            .vault
            .registry_overview()
            .await
            .unwrap()
            .map_or(0, |overview| overview.revision);
        let request = setup_request(&fixture, revision);
        let installed = fixture
            .vault
            .install_calendar_expert(request.clone(), Cancellation::default())
            .await
            .unwrap();
        let result = expert(&fixture.sample().await);
        if let Some(original) = original {
            assert_eq!(original.assignment_id, result.assignment_id);
            assert_eq!(result.state_revision, original.state_revision + 1);
        } else {
            assert_eq!(result.state_revision, 1);
        }
        let before = fixture.vault.expert_registry().await.unwrap().unwrap();
        assert_eq!(before.packages.len(), 4);
        assert_eq!(before.assignments.len(), 4);
        assert_eq!(
            before.calendar_setups,
            std::slice::from_ref(&installed.setup)
        );
        let mut registry = AgentRegistry::restore(before.clone(), request.instance_id).unwrap();
        registry
            .set_assignment_enabled(before.revision, fixture.person, result.assignment_id, false)
            .unwrap();
        fixture
            .vault
            .save_expert_registry(before.revision, &registry.snapshot())
            .await
            .unwrap();
        let disabled = registry.snapshot();
        let replay = fixture
            .vault
            .install_calendar_expert(request, Cancellation::default())
            .await
            .unwrap();
        assert_eq!(replay.setup, installed.setup);
        let session = fixture.vault.create_sample_session().await.unwrap();
        let attempted = fixture
            .vault
            .run_persisted_agent_sample(
                AgentFixtureTurn {
                    person_id: fixture.person,
                    session_id: session.id,
                    expected_revision: session.revision,
                    prompt: AgentFixturePrompt::Today,
                },
                Cancellation::default(),
                Duration::ZERO,
                |_| {},
            )
            .await
            .unwrap();
        assert!(
            !attempted
                .messages
                .iter()
                .any(|message| matches!(message, AgentMessage::Capability { result: Ok(_), .. }))
        );
        assert_eq!(
            fixture.vault.expert_registry().await.unwrap(),
            Some(disabled)
        );
    }
}

#[tokio::test]
async fn setup_rejects_changed_retry_stale_new_request_wrong_instance_and_cancelled_work() {
    let fixture = Fixture::new().await;
    let request = setup_request(&fixture, 0);
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert_eq!(
        fixture
            .vault
            .install_calendar_expert(request.clone(), cancellation)
            .await,
        Err(AgentFailure::Cancelled)
    );
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), None);
    fixture
        .vault
        .install_calendar_expert(request.clone(), Cancellation::default())
        .await
        .unwrap();
    let before = fixture.vault.expert_registry().await.unwrap().unwrap();
    for mode in 0..5 {
        let mut changed = request.clone();
        match mode {
            0 => changed.calendar_ids = vec!["different".into()],
            1 => changed.provider = floe_domain::CalendarProvider::Fixture,
            2 => changed.expected_revision = before.revision,
            3 => changed.setup_id = Uuid::new_v4(),
            _ => changed.instance_id = Uuid::new_v4(),
        }
        assert!(
            fixture
                .vault
                .install_calendar_expert(changed, Cancellation::default())
                .await
                .is_err()
        );
        assert_eq!(
            fixture.vault.expert_registry().await.unwrap(),
            Some(before.clone())
        );
    }
}

#[tokio::test]
async fn setup_key_loss_before_commit_rolls_back_both_initialization_and_existing_registry() {
    for initialized in [false, true] {
        let mut fixture = Fixture::new().await;
        let before = if initialized {
            Some(fixture.prepare().await)
        } else {
            None
        };
        let revision = before.as_ref().map_or(0, |snapshot| snapshot.revision);
        let request = setup_request(&fixture, revision);
        fixture.keys.0.fail_on_read.store(3, Ordering::Release);
        assert_eq!(
            fixture
                .vault
                .install_calendar_expert(request.clone(), Cancellation::default())
                .await,
            Err(AgentFailure::VaultUnavailable)
        );
        assert_eq!(
            fixture.vault.check_access(),
            Err(AgentFailure::VaultUnavailable)
        );
        drop(fixture.vault);
        fixture.keys.0.blocked.store(false, Ordering::Release);
        fixture.vault =
            EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
                .await
                .unwrap();
        assert_eq!(fixture.vault.expert_registry().await.unwrap(), before);
        let result = fixture
            .vault
            .install_calendar_expert(request, Cancellation::default())
            .await
            .unwrap();
        assert_eq!(result.registry.revision, revision + 1);
        assert_eq!(
            fixture
                .vault
                .expert_registry()
                .await
                .unwrap()
                .unwrap()
                .calendar_setups
                .len(),
            1
        );
    }
}

#[tokio::test]
async fn setup_post_commit_key_failure_reconciles_the_durable_receipt_after_reopen() {
    for fail_on_read in [4, 5] {
        let mut fixture = Fixture::new().await;
        let request = setup_request(&fixture, 0);
        fixture
            .keys
            .0
            .fail_on_read
            .store(fail_on_read, Ordering::Release);
        assert_eq!(
            fixture
                .vault
                .install_calendar_expert(request.clone(), Cancellation::default())
                .await,
            Err(AgentFailure::VaultUnavailable)
        );
        drop(fixture.vault);
        fixture.keys.0.blocked.store(false, Ordering::Release);
        fixture.vault =
            EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
                .await
                .unwrap();
        let committed = fixture.vault.expert_registry().await.unwrap().unwrap();
        assert_eq!(committed.revision, 1);
        let replay = fixture
            .vault
            .install_calendar_expert(request, Cancellation::default())
            .await
            .unwrap();
        assert_eq!(replay.setup, committed.calendar_setups[0]);
        assert_eq!(
            fixture.vault.expert_registry().await.unwrap(),
            Some(committed)
        );
    }
}

#[tokio::test]
async fn staged_setup_cancellation_rolls_back_initial_tables_and_existing_registry() {
    for initialized in [false, true] {
        let fixture = Fixture::new().await;
        let before = if initialized {
            Some(fixture.prepare().await)
        } else {
            None
        };
        let mut registry = match &before {
            Some(snapshot) => {
                AgentRegistry::restore(snapshot.clone(), snapshot.instance_id).unwrap()
            }
            None => AgentRegistry::new(fixture.vault.registry_instance_id()),
        };
        let revision = registry.revision();
        registry
            .install_calendar_expert(fixture.person, &setup_request(&fixture, revision))
            .unwrap();
        let checks = std::sync::atomic::AtomicUsize::new(0);
        let check = || {
            if checks.fetch_add(1, Ordering::AcqRel) > 0 {
                Err(AgentFailure::Cancelled)
            } else {
                Ok(())
            }
        };
        let result = if initialized {
            fixture
                .vault
                .save_expert_registry_checked(revision, &registry.snapshot(), check)
                .await
        } else {
            fixture
                .vault
                .initialize_expert_registry_checked(&registry.snapshot(), check)
                .await
        };
        assert_eq!(result, Err(AgentFailure::Cancelled));
        assert_eq!(checks.load(Ordering::Acquire), 2);
        assert_eq!(fixture.vault.expert_registry().await.unwrap(), before);
        fixture
            .vault
            .install_calendar_expert(setup_request(&fixture, revision), Cancellation::default())
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn registry_cas_rejects_setup_receipt_removal_replacement_appropriation_and_preenablement() {
    let fixture = Fixture::new().await;
    let request = setup_request(&fixture, 0);
    fixture
        .vault
        .install_calendar_expert(request.clone(), Cancellation::default())
        .await
        .unwrap();
    let before = fixture.vault.expert_registry().await.unwrap().unwrap();
    for mode in 0..4 {
        let mut changed = before.clone();
        changed.revision += 1;
        match mode {
            0 => changed.calendar_setups.clear(),
            1 => changed.calendar_setups[0].setup_id = Uuid::new_v4(),
            2 => changed.calendar_setups[0].expected_revision = before.revision,
            _ => {
                let mut registry =
                    AgentRegistry::restore(before.clone(), request.instance_id).unwrap();
                registry
                    .install_calendar_expert(
                        fixture.person,
                        &setup_request(&fixture, before.revision),
                    )
                    .unwrap();
                changed = registry.snapshot();
                changed.installations.last_mut().unwrap().enabled = true;
            }
        }
        assert_eq!(
            fixture
                .vault
                .save_expert_registry(before.revision, &changed)
                .await,
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            fixture.vault.expert_registry().await.unwrap(),
            Some(before.clone())
        );
    }
    let fresh = Fixture::new().await;
    let mut unreceipted = before.clone();
    unreceipted.instance_id = fresh.vault.registry_instance_id();
    unreceipted.calendar_setups.clear();
    for binding in &mut unreceipted.calendar_views {
        binding.person_id = fresh.person;
    }
    for assignment in &mut unreceipted.assignments {
        assignment.person_id = fresh.person;
    }
    fresh
        .vault
        .initialize_expert_registry(&unreceipted)
        .await
        .unwrap();
    let mut appropriated = unreceipted.clone();
    appropriated.revision += 1;
    let mut receipt = before.calendar_setups[0].clone();
    receipt.person_id = fresh.person;
    receipt.expected_revision = unreceipted.revision;
    appropriated.calendar_setups.push(receipt);
    assert_eq!(
        fresh
            .vault
            .save_expert_registry(unreceipted.revision, &appropriated)
            .await,
        Err(AgentFailure::Conflict)
    );
    assert_eq!(
        fresh.vault.expert_registry().await.unwrap(),
        Some(unreceipted)
    );
}
