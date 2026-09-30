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

pub(super) fn finish_owner(
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
        wait_for_job(worker, operation_id, deadline);
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
fn terminal_session_reads_can_overlap_the_finishing_turn_job() {
    let caller = remote_caller(PersonId::new(), "verified-device");
    let get = LocalOperationIntent::ConversationSession(ConversationSessionOperation::Get {
        session_id: Uuid::new_v4(),
    });
    let start = LocalOperationIntent::ConversationSession(ConversationSessionOperation::Start);
    assert!(get.action(&caller).is_concurrent_host());
    assert!(!start.action(&caller).is_concurrent_host());
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
