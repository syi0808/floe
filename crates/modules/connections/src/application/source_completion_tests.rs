//! Completion-admission owner tests. Evidence is a synthetic port; these do not
//! qualify a real Gateway, OS permission prompt, or the encrypted repository.
use super::*;
use floe_access::{GrantCommitKind, VerifiedGatewayBinding};
use floe_context_contract::{
    ConnectionId, ConnectorId, ExecutionOwnerId, GrantSourceBinding, ResourceHandle,
    SourceAuthority,
};
use floe_execution::{
    Cancellation,
    budget::{BudgetConfig, BudgetLedger, ModelUsage},
};
use floe_kernel::{PersonId, TraceContext};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Evidence {
    result: Result<SourceExpectation, AgentFailure>,
    calls: AtomicUsize,
}
impl SourceReviewEvidence for Evidence {
    fn observe<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: &'a SourceConnection,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<SourceExpectation, AgentFailure>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { self.result.clone() })
    }
    fn inspect_selection<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: &'a SourceConnection,
        _: &'a [crate::ConnectionResource],
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<String, AgentFailure>> {
        Box::pin(async { panic!("completion must not inspect a new selection") })
    }
}
fn scope() -> ExecutionScope {
    ExecutionScope::root(
        Cancellation::new(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
        BudgetLedger::new(BudgetConfig::new(64, 64), ModelUsage::default()).root_lease(),
        TraceContext::new(Uuid::new_v4()),
    )
}
fn fixture(
    kind: SourceOperationKind,
) -> (
    OwnerActor,
    SourceConnection,
    SourceOperationRecord,
    GrantCommitReceipt,
) {
    let actor = OwnerActor {
        person_id: PersonId::new(),
        device_id: "fixture.device".into(),
        runtime_epoch: 1,
    };
    let source = SourceConnection::establish_remote(
        actor.person_id,
        ConnectorId::try_new("google.gmail").unwrap(),
        ConnectionId::try_new("fixture.mail").unwrap(),
        ExecutionOwnerId::try_new("fixture.server").unwrap(),
        vec![
            crate::ConnectionResource::new(
                ResourceHandle::try_new("inbox").unwrap(),
                "Inbox".into(),
                None,
            )
            .unwrap(),
        ],
        SourceAuthority::new(),
    )
    .unwrap();
    let expected = SourceExpectation {
        source: GrantSourceBinding::try_new(
            actor.person_id,
            source.connection_id().clone(),
            source.connector_id().clone(),
            source.execution_owner_id().clone(),
        )
        .unwrap(),
        revision: Some(source.revision()),
        provider_revision: Some(23),
        authority: source.source_authority(),
        physical_resources: vec![ResourceHandle::try_new("inbox").unwrap()],
        subject_fingerprint: "fixture.subject".into(),
        gateway: Some(VerifiedGatewayBinding {
            person_id: actor.person_id.to_string(),
            device_id: actor.device_id.clone(),
            client_id: "fixture.client".into(),
            producer_instance: "fixture.instance".into(),
            producer_key_fingerprint: "a".repeat(64),
            producer_audience: "fixture.audience".into(),
            enrollment_id: "fixture.enrollment".into(),
            credential_generation: 1,
        }),
    };
    let review = (kind == SourceOperationKind::ConnectionReviewApply).then(|| ReviewRef {
        id: Uuid::new_v4(),
        revision: 1,
        digest: [7; 32],
    });
    let commit_kind = match kind {
        SourceOperationKind::ConnectionDisconnect => GrantCommitKind::Disconnect,
        SourceOperationKind::ConnectionObservePause => GrantCommitKind::PauseObserve,
        SourceOperationKind::ConnectionReviewApply => GrantCommitKind::Reviewed {
            review: review.clone().unwrap(),
        },
        _ => panic!("not a completion fixture"),
    };
    let (mut operation, reservation) =
        prepare_source_reservation(&actor, Uuid::new_v4(), [1; 32], review, expected, kind)
            .unwrap();
    let receipt = GrantCommitReceipt {
        reservation,
        kind: commit_kind,
        commit_id: Uuid::new_v4(),
        grants: vec![],
        cleanup_ids: vec![],
        commit_digest: [9; 32],
    };
    operation.phase = SourceOperationPhase::GrantCommitted {
        receipt_id: receipt.commit_id,
        receipt_digest: receipt.digest().unwrap(),
    };
    operation.validate().unwrap();
    receipt.validate().unwrap();
    (actor, source, operation, receipt)
}

#[tokio::test]
async fn committed_disconnect_can_replay_after_remote_source_disappears() {
    let (actor, source, mut operation, receipt) =
        fixture(SourceOperationKind::ConnectionDisconnect);
    let evidence = Evidence {
        result: Err(AgentFailure::CapabilityUnavailable),
        calls: AtomicUsize::new(0),
    };
    let original = serde_json::to_vec(&receipt).unwrap();
    for phase in [
        operation.phase.clone(),
        SourceOperationPhase::CleaningUp {
            receipt_id: receipt.commit_id,
            cleanup_revision: 1,
        },
        SourceOperationPhase::RepairRequired {
            receipt_id: receipt.commit_id,
            reason: SourceRepairReason::CleanupUncertain,
        },
    ] {
        operation.phase = phase;
        // Simulate owner recreation from the retained intent, not a new preview.
        let restored: GrantCommitReceipt = serde_json::from_slice(&original).unwrap();
        assert_eq!(
            completion_source_matches(
                &evidence,
                &actor,
                &operation,
                &restored,
                Some(&source),
                &scope()
            )
            .await,
            Ok(true)
        );
        assert_eq!(serde_json::to_vec(&restored).unwrap(), original);
    }
    assert_eq!(evidence.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn committed_pause_does_not_depend_on_provider_permission() {
    let (actor, source, operation, receipt) = fixture(SourceOperationKind::ConnectionObservePause);
    let evidence = Evidence {
        result: Err(AgentFailure::PolicyDenied),
        calls: AtomicUsize::new(0),
    };
    assert_eq!(
        completion_source_matches(
            &evidence,
            &actor,
            &operation,
            &receipt,
            Some(&source),
            &scope()
        )
        .await,
        Ok(true)
    );
    assert_eq!(evidence.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn committed_reductions_still_require_exact_local_source_cas() {
    for kind in [
        SourceOperationKind::ConnectionDisconnect,
        SourceOperationKind::ConnectionObservePause,
    ] {
        let (actor, mut source, operation, receipt) = fixture(kind);
        let evidence = Evidence {
            result: Err(AgentFailure::PolicyDenied),
            calls: AtomicUsize::new(0),
        };
        assert_eq!(
            completion_source_matches(&evidence, &actor, &operation, &receipt, None, &scope())
                .await,
            Ok(false)
        );
        source.disconnect(source.revision()).unwrap();
        assert_eq!(
            completion_source_matches(
                &evidence,
                &actor,
                &operation,
                &receipt,
                Some(&source),
                &scope()
            )
            .await,
            Ok(false)
        );
        assert_eq!(evidence.calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn reviewed_completion_retains_fresh_evidence_and_error_checks() {
    let (actor, source, operation, receipt) = fixture(SourceOperationKind::ConnectionReviewApply);
    let evidence = Evidence {
        result: Err(AgentFailure::PolicyDenied),
        calls: AtomicUsize::new(0),
    };
    assert_eq!(
        completion_source_matches(
            &evidence,
            &actor,
            &operation,
            &receipt,
            Some(&source),
            &scope()
        )
        .await,
        Err(AgentFailure::PolicyDenied)
    );
    let mut evidence = Evidence {
        result: Ok(receipt.reservation.source.clone()),
        calls: AtomicUsize::new(0),
    };
    assert_eq!(
        completion_source_matches(
            &evidence,
            &actor,
            &operation,
            &receipt,
            Some(&source),
            &scope()
        )
        .await,
        Ok(true)
    );
    evidence.result.as_mut().unwrap().provider_revision = Some(24);
    assert_eq!(
        completion_source_matches(
            &evidence,
            &actor,
            &operation,
            &receipt,
            Some(&source),
            &scope()
        )
        .await,
        Ok(false)
    );
    assert_eq!(evidence.calls.load(Ordering::SeqCst), 2);
}
