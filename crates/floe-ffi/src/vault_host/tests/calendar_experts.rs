use floe_agent::{CalendarExpertSetup, RegistryConfiguration, RegistryConfigurationTarget};

use super::*;

#[test]
fn calendar_setup_worker_inspects_without_initializing_installs_and_reconciles_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    let person = PersonId::new();
    let keys = Keys::default();
    let worker = Worker::new(root.clone(), keys.clone()).unwrap();
    let inspect = AgentVaultActionDto::CalendarExperts { setup: None };
    assert_eq!(
        perform(&worker, person, inspect.clone()).failure,
        Some(AgentFailure::VaultUnavailable)
    );
    assert!(!root.exists());
    perform(&worker, person, AgentVaultActionDto::Create {});
    let empty = perform(&worker, person, inspect.clone())
        .calendar_experts
        .unwrap();
    assert_eq!(empty.registry.revision, 0);
    assert!(empty.views.is_empty() && empty.setups.is_empty());
    assert!(
        perform(
            &worker,
            person,
            AgentVaultActionDto::Registry { change: None }
        )
        .registry
        .is_none()
    );
    let setup = CalendarExpertSetup {
        instance_id: empty.registry.instance_id,
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
        provider: floe_domain::CalendarProvider::EventKit,
        calendar_ids: vec!["explicit-native-setup-canary".into()],
    };
    let action = AgentVaultActionDto::CalendarExperts {
        setup: Some(setup.clone()),
    };
    let id = Uuid::new_v4();
    worker
        .request(
            person,
            id,
            AgentVaultOperationDto::Submit {
                action: action.clone(),
            },
        )
        .unwrap();
    let completed = wait(&worker, person, id);
    assert_eq!(
        worker
            .request(
                person,
                id,
                AgentVaultOperationDto::Submit {
                    action: action.clone()
                }
            )
            .unwrap(),
        completed
    );
    assert_eq!(
        worker.request(
            PersonId::new(),
            id,
            AgentVaultOperationDto::Poll { after_sequence: 0 }
        ),
        Err(AgentFailure::NotFound)
    );
    assert!(
        completed.events.is_empty() && completed.session.is_none() && completed.registry.is_none()
    );
    let installed = completed.calendar_experts.unwrap();
    assert_eq!(installed.registry.revision, 1);
    assert!(!installed.views[0].enabled);
    assert!(
        installed
            .registry
            .assignments
            .iter()
            .all(|entry| !entry.enabled)
    );
    worker
        .request(person, id, AgentVaultOperationDto::Release {})
        .unwrap();
    assert_eq!(
        perform(&worker, person, action.clone())
            .calendar_experts
            .as_ref(),
        Some(&installed)
    );
    let mut changed = setup;
    changed.calendar_ids = vec!["retarget".into()];
    assert_eq!(
        perform(
            &worker,
            person,
            AgentVaultActionDto::CalendarExperts {
                setup: Some(changed)
            }
        )
        .failure,
        Some(AgentFailure::Conflict)
    );
    for enabled in [true, false] {
        let current = perform(&worker, person, inspect.clone())
            .calendar_experts
            .unwrap();
        let result = perform(
            &worker,
            person,
            AgentVaultActionDto::Registry {
                change: Some(RegistryConfiguration {
                    instance_id: current.registry.instance_id,
                    expected_revision: current.registry.revision,
                    target: RegistryConfigurationTarget::CalendarView {
                        id: installed.views[0].handle,
                        enabled,
                    },
                }),
            },
        );
        assert!(result.failure.is_none());
    }
    let before = perform(&worker, person, inspect.clone())
        .calendar_experts
        .unwrap();
    perform(&worker, person, AgentVaultActionDto::Lock {});
    drop(worker);
    let worker = Worker::new(root, keys.clone()).unwrap();
    perform(&worker, person, AgentVaultActionDto::Unlock {});
    assert_eq!(
        perform(&worker, person, action).calendar_experts.as_ref(),
        Some(&before)
    );
    assert_eq!(
        perform(&worker, PersonId::new(), inspect.clone()).failure,
        Some(AgentFailure::NotFound)
    );
    keys.0.unavailable.store(true, Ordering::Release);
    let denied = perform(&worker, person, inspect.clone());
    assert_eq!(denied.failure, Some(AgentFailure::VaultUnavailable));
    assert!(denied.calendar_experts.is_none());
    keys.0.unavailable.store(false, Ordering::Release);
    assert_eq!(
        perform(&worker, person, inspect).failure,
        Some(AgentFailure::VaultUnavailable)
    );
}

#[test]
fn blocked_setup_keeps_worker_ownership_until_cancelled_work_really_finishes() {
    let directory = tempfile::tempdir().unwrap();
    let keys = Keys::default();
    let worker = Worker::new(directory.path().join("vaults"), keys.clone()).unwrap();
    let person = PersonId::new();
    perform(&worker, person, AgentVaultActionDto::Create {});
    let inspect = AgentVaultActionDto::CalendarExperts { setup: None };
    let empty = perform(&worker, person, inspect.clone())
        .calendar_experts
        .unwrap();
    let request = CalendarExpertSetup {
        instance_id: empty.registry.instance_id,
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
        provider: floe_domain::CalendarProvider::Fixture,
        calendar_ids: vec!["bounded-scope".into()],
    };
    *keys.0.paused.lock().unwrap() = true;
    keys.0.entered.store(false, Ordering::Release);
    let id = Uuid::new_v4();
    worker
        .request(
            person,
            id,
            AgentVaultOperationDto::Submit {
                action: AgentVaultActionDto::CalendarExperts {
                    setup: Some(request.clone()),
                },
            },
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !keys.0.entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !worker
            .request(person, id, AgentVaultOperationDto::Stop {})
            .unwrap()
            .done
    );
    assert_eq!(
        worker.request(person, id, AgentVaultOperationDto::Release {}),
        Err(AgentFailure::Conflict)
    );
    assert_eq!(
        worker.request(
            person,
            Uuid::new_v4(),
            AgentVaultOperationDto::Submit {
                action: inspect.clone()
            }
        ),
        Err(AgentFailure::Conflict)
    );
    *keys.0.paused.lock().unwrap() = false;
    keys.0.wake.notify_all();
    let cancelled = wait(&worker, person, id);
    assert_eq!(cancelled.failure, Some(AgentFailure::Cancelled));
    assert!(cancelled.calendar_experts.is_none());
    worker
        .request(person, id, AgentVaultOperationDto::Release {})
        .unwrap();
    assert_eq!(
        perform(&worker, person, inspect).calendar_experts.as_ref(),
        Some(&empty)
    );
    let retry = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarExperts {
            setup: Some(request),
        },
    );
    assert_eq!(retry.calendar_experts.unwrap().registry.revision, 1);
}
