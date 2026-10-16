#[path = "support/exact_recovery.rs"]
mod exact_recovery;
use exact_recovery::observe_exact_request;
use floe_kernel::{AgentFailure, CommandFailure};
use std::cell::RefCell;

#[tokio::test]
async fn uncertain_client_recovery_preserves_request_and_is_bounded() {
    let request = (
        floe_kernel::CommandId::new(),
        7_u64,
        "original intent".to_owned(),
    );
    for failure in [
        CommandFailure::Indeterminate(AgentFailure::StorageBusy),
        CommandFailure::Admitted(AgentFailure::StorageBusy),
        CommandFailure::NotApplied(AgentFailure::StorageBusy),
        CommandFailure::NotAdmitted(AgentFailure::StorageBusy),
        CommandFailure::Indeterminate(AgentFailure::StorageUnavailable),
        CommandFailure::Indeterminate(AgentFailure::Conflict),
        CommandFailure::Admitted(AgentFailure::AccessReviewRequired),
    ] {
        let requests = RefCell::new(Vec::new());
        let retryable = |failure: &CommandFailure<AgentFailure>| {
            matches!(
                failure,
                CommandFailure::Indeterminate(AgentFailure::StorageBusy)
                    | CommandFailure::Admitted(AgentFailure::StorageBusy)
            )
        };
        let result: Result<(), _> = observe_exact_request(
            request.clone(),
            |next| {
                requests.borrow_mut().push(next);
                std::future::ready(Err(failure))
            },
            retryable,
        )
        .await;
        assert_eq!(result, Err(failure));
        assert_eq!(
            *requests.borrow(),
            vec![request.clone(); if retryable(&failure) { 4 } else { 1 }]
        );
    }
}

#[tokio::test]
async fn exact_recovery_stops_at_first_success() {
    let request = (floe_kernel::CommandId::new(), 11_u64);
    let requests = RefCell::new(Vec::new());
    let result = observe_exact_request(
        request,
        |next| {
            requests.borrow_mut().push(next);
            std::future::ready(if requests.borrow().len() == 1 {
                Err(AgentFailure::StorageBusy)
            } else {
                Ok(17)
            })
        },
        |failure| *failure == AgentFailure::StorageBusy,
    )
    .await;
    assert_eq!(result, Ok(17));
    assert_eq!(*requests.borrow(), vec![request, request]);
}

#[test]
fn connections_recovery_accepts_only_unsettled_busy_not_negative_receipts() {
    use floe_connections::ConnectionsCommandFailure as F;
    use floe_kernel::AgentFailure as A;
    for failure in [
        F::NotAdmitted(A::StorageBusy),
        F::Indeterminate(A::StorageBusy),
        F::Admitted(A::StorageBusy),
    ] {
        assert!(exact_recovery::connections_busy(&failure));
    }
    for failure in [
        F::NotApplied(A::StorageBusy),
        F::NotApplied(A::Conflict),
        F::Indeterminate(A::StorageUnavailable),
        F::Admitted(A::AccessReviewRequired),
        F::NotAdmitted(A::PolicyDenied),
    ] {
        assert!(!exact_recovery::connections_busy(&failure));
    }
}

#[test]
fn synchronous_exact_recovery_preserves_body_and_exhausts_without_replacement() {
    let request = (
        floe_kernel::CommandId::new(),
        "unchanged body".to_owned(),
        9,
    );
    for failure in [
        AgentFailure::StorageBusy,
        AgentFailure::Conflict,
        AgentFailure::StorageUnavailable,
    ] {
        let requests = RefCell::new(Vec::new());
        let result: Result<(), _> = exact_recovery::observe_exact_request_sync(
            request.clone(),
            |next| {
                requests.borrow_mut().push(next);
                Err(failure)
            },
            |failure| *failure == AgentFailure::StorageBusy,
        );
        assert_eq!(result, Err(failure));
        assert_eq!(
            *requests.borrow(),
            vec![
                request.clone();
                if failure == AgentFailure::StorageBusy {
                    4
                } else {
                    1
                }
            ]
        );
    }
}
