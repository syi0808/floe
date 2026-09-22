use super::*;
use crate::VaultLifecycleCommand;
use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};

fn finish_local(
    worker: &Worker,
    caller: &crate::CallerContext,
    operation_id: Uuid,
) -> WorkerResult {
    finish_owner(worker, caller, operation_id, LocalOperationOwner::Vault)
}

fn finish_owner(
    worker: &Worker,
    caller: &crate::CallerContext,
    operation_id: Uuid,
    owner: LocalOperationOwner,
) -> WorkerResult {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let result = worker
            .local_request(caller, operation_id, None, owner, false)
            .unwrap();
        if result.done {
            return result;
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
}

#[test]
fn admitted_sessions_and_management_reads_share_canonical_owners_not_fixture_sessions() {
    let directory = tempfile::tempdir().unwrap();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    let person = PersonId::new();
    let caller = remote_caller(person, "verified-device");
    assert_eq!(perform(&worker, person, WorkerAction::Create).failure, None);
    let submit = |intent: LocalOperationIntent| {
        let operation_id = Uuid::new_v4();
        let owner = intent.owner();
        worker
            .local_request(&caller, operation_id, Some(intent), owner, false)
            .unwrap();
        let result = finish_owner(&worker, &caller, operation_id, owner);
        assert_eq!(result.failure, None);
        for other in [
            LocalOperationOwner::Vault,
            LocalOperationOwner::Conversation,
            LocalOperationOwner::Experts,
            LocalOperationOwner::Access,
            LocalOperationOwner::Knowledge,
            LocalOperationOwner::Connections,
        ] {
            if other != owner {
                assert_eq!(
                    worker
                        .local_request(&caller, operation_id, None, other, false)
                        .unwrap_err(),
                    AgentFailure::NotFound
                );
            }
        }
        worker
            .local_request(&caller, operation_id, None, owner, true)
            .unwrap();
        result
    };
    let started = submit(LocalOperationIntent::ConversationSession(
        ConversationSessionOperation::Start,
    ))
    .session
    .unwrap();
    assert_eq!(started.person_id, person);
    assert!(started.scope.is_none());
    assert_eq!(
        started.data_classes,
        [floe_agent_contract::DataClass::Personal]
    );
    let loaded = submit(LocalOperationIntent::ConversationSession(
        ConversationSessionOperation::Get {
            session_id: started.id,
        },
    ))
    .session
    .unwrap();
    assert_eq!(loaded, started);
    let resumed = submit(LocalOperationIntent::ConversationSession(
        ConversationSessionOperation::Resume,
    ))
    .session
    .unwrap();
    assert_eq!(resumed.id, started.id);
    assert!(
        submit(LocalOperationIntent::ExpertInspection(
            crate::ExpertInspection::Registry
        ))
        .registry
        .is_some()
    );
    assert!(
        submit(LocalOperationIntent::ExpertInspection(
            crate::ExpertInspection::Calendar
        ))
        .calendar_experts
        .is_some()
    );
    assert!(
        submit(LocalOperationIntent::KnowledgeInspection(
            crate::KnowledgeInspection::Memory
        ))
        .memory
        .is_some()
    );
    assert!(
        submit(LocalOperationIntent::KnowledgeInspection(
            crate::KnowledgeInspection::Review
        ))
        .memory_review
        .is_some()
    );
    assert!(
        submit(LocalOperationIntent::Connections)
            .connections
            .is_some()
    );
}

#[test]
fn local_expert_and_access_intents_inject_only_the_admitted_device() {
    let caller = remote_caller(PersonId::new(), "verified-device");
    let setup = crate::CalendarExpertInstall {
        instance_id: Uuid::new_v4(),
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
        provider: CalendarProvider::EventKit,
        calendar_ids: vec!["calendar".into()],
        connection_scope: crate::CalendarScope::Selected,
        connection_revision: 1,
        source_authority: Some(crate::SourceAuthority::new()),
        reviewed_native_subject_fingerprint: Some("a".repeat(64)),
    };
    let WorkerAction::CalendarExperts { setup: Some(bound) } =
        LocalOperationIntent::ExpertCommand(crate::ExpertCommand::InstallCalendar(setup.clone()))
            .action(&caller)
    else {
        panic!("wrong owner action")
    };
    assert_eq!(bound.device_id, caller.device_id());
    assert_eq!(
        bound.reviewed_native_subject_fingerprint,
        setup.reviewed_native_subject_fingerprint
    );
    assert_eq!(bound.source_authority, setup.source_authority);
    let expected_grant_id = crate::GrantId::new();
    let expected_grant_authority = crate::GrantAuthority::new();
    let intent = LocalOperationIntent::LocalAccessCommand(crate::LocalAccessCommand::Personal {
        connector: "attention.macos".into(),
        change: crate::PersonalAccessChange::Review {
            expected_native_subject_fingerprint: "b".repeat(64),
            consumers: vec!["assistant".into()],
            feasibility_query: None,
            expected_grant_id: Some(expected_grant_id),
            expected_grant_authority: Some(expected_grant_authority),
        },
    });
    let WorkerAction::PersonalAccess { change } = intent.action(&caller) else {
        panic!("wrong owner action")
    };
    assert_eq!(change.device_id, caller.device_id());
    let crate::PersonalAccessChange::Review {
        expected_grant_id: actual_id,
        expected_grant_authority: actual_authority,
        expected_native_subject_fingerprint,
        consumers,
        ..
    } = change.change
    else {
        panic!("wrong review")
    };
    assert_eq!(actual_id, Some(expected_grant_id));
    assert_eq!(actual_authority, Some(expected_grant_authority));
    assert_eq!(expected_native_subject_fingerprint, "b".repeat(64));
    assert_eq!(consumers, ["assistant"]);
}

#[test]
fn local_vault_results_bind_identity_epoch_and_exact_intent() {
    let directory = tempfile::tempdir().unwrap();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    let person = PersonId::new();
    let caller = remote_caller(person, "verified-device");
    let operation_id = Uuid::new_v4();
    worker
        .local_request(
            &caller,
            operation_id,
            Some(LocalOperationIntent::VaultStatus),
            LocalOperationOwner::Vault,
            false,
        )
        .unwrap();
    assert_eq!(
        finish_local(&worker, &caller, operation_id).state,
        Some(VaultState::Missing)
    );
    for operation in [
        WorkerOperation::Poll { after_sequence: 0 },
        WorkerOperation::Stop,
        WorkerOperation::Release,
    ] {
        assert_eq!(
            worker.request(person, operation_id, operation).unwrap_err(),
            AgentFailure::PolicyDenied
        );
    }
    for foreign in [
        remote_caller(PersonId::new(), "verified-device"),
        remote_caller(person, "other-device"),
        crate::CallerContext::verified(
            crate::LocalIdentityClaim {
                person_id: person.0,
                device_id: "verified-device".into(),
            },
            2,
        )
        .unwrap(),
    ] {
        assert_eq!(
            worker
                .local_request(
                    &foreign,
                    operation_id,
                    None,
                    LocalOperationOwner::Vault,
                    false
                )
                .unwrap_err(),
            AgentFailure::NotFound
        );
    }
    assert_eq!(
        worker
            .local_request(
                &caller,
                operation_id,
                Some(LocalOperationIntent::VaultCommand(
                    VaultLifecycleCommand::Create
                )),
                LocalOperationOwner::Vault,
                false
            )
            .unwrap_err(),
        AgentFailure::Conflict
    );
    assert_eq!(
        worker
            .remote_request(&caller, operation_id, None, false, false)
            .unwrap_err(),
        AgentFailure::PolicyDenied
    );
    assert_eq!(
        worker
            .local_request(
                &caller,
                operation_id,
                None,
                LocalOperationOwner::Conversation,
                false
            )
            .unwrap_err(),
        AgentFailure::NotFound
    );
    assert_eq!(
        worker
            .local_request(
                &caller,
                operation_id,
                Some(LocalOperationIntent::VaultStatus),
                LocalOperationOwner::Vault,
                false
            )
            .unwrap()
            .request_id,
        operation_id
    );
    worker
        .local_request(
            &caller,
            operation_id,
            None,
            LocalOperationOwner::Vault,
            true,
        )
        .unwrap();
    assert_eq!(
        worker
            .local_request(
                &caller,
                operation_id,
                None,
                LocalOperationOwner::Vault,
                false
            )
            .unwrap_err(),
        AgentFailure::NotFound
    );
    assert_eq!(
        worker
            .local_request(
                &caller,
                Uuid::nil(),
                Some(LocalOperationIntent::VaultStatus),
                LocalOperationOwner::Vault,
                false
            )
            .unwrap_err(),
        AgentFailure::InvalidInput
    );
}

#[test]
fn admitted_vault_lifecycle_reuses_keys_and_preserves_exclusive_commands() {
    let directory = tempfile::tempdir().unwrap();
    let keys = Keys::default();
    let worker = Worker::new(directory.path().join("vaults"), keys.clone()).unwrap();
    let caller = remote_caller(PersonId::new(), "verified-device");
    for (command, expected) in [
        (VaultLifecycleCommand::Create, VaultState::Ready),
        (VaultLifecycleCommand::Lock, VaultState::Locked),
        (VaultLifecycleCommand::Unlock, VaultState::Ready),
    ] {
        let operation_id = Uuid::new_v4();
        let intent = LocalOperationIntent::VaultCommand(command);
        assert!(intent.action(&caller).is_exclusive_host());
        worker
            .local_request(
                &caller,
                operation_id,
                Some(intent),
                LocalOperationOwner::Vault,
                false,
            )
            .unwrap();
        let result = finish_local(&worker, &caller, operation_id);
        assert_eq!(result.failure, None);
        assert_eq!(result.state, Some(expected));
        worker
            .local_request(
                &caller,
                operation_id,
                None,
                LocalOperationOwner::Vault,
                true,
            )
            .unwrap();
    }
}
