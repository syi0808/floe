//! Completion-admission owner tests. Evidence is a synthetic port; these do not
//! qualify a real Gateway, OS permission prompt, or the encrypted repository.
use super::*;
use floe_access::{GrantCommitKind, GrantReservation, VerifiedGatewayBinding};
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
    SourceExpectation,
) {
    let actor = OwnerActor {
        person_id: PersonId::new(),
        device_id: "fixture.device".into(),
        runtime_epoch: 1,
    };
    let source = SourceConnection::establish_remote(
        actor.person_id,
        ConnectorId::try_new("gmail").unwrap(),
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
    let (mut operation, reservation) = if kind == SourceOperationKind::ConnectionReviewApply {
        let (record, observed) = prepare_source_reservation(
            &actor,
            Uuid::new_v4(),
            [1; 32],
            review,
            expected.clone(),
            kind,
        )
        .unwrap();
        (record, GrantReservation::Observed(observed))
    } else {
        let record =
            prepare_reduction_reservation(&actor, Uuid::new_v4(), [1; 32], &source, kind).unwrap();
        let reservation = GrantReservation::Reduction(record.identity());
        (record, reservation)
    };
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
    (actor, source, operation, receipt, expected)
}

#[tokio::test]
async fn committed_disconnect_can_replay_after_remote_source_disappears() {
    let (actor, source, mut operation, receipt, target) =
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
        if !matches!(operation.phase, SourceOperationPhase::GrantCommitted { .. }) {
            operation.cleanup_target = Some(
                crate::SourceCleanupTarget::from_observation(&operation, target.clone()).unwrap(),
            );
        }
        operation.validate().unwrap();
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
    let (actor, source, operation, receipt, _) =
        fixture(SourceOperationKind::ConnectionObservePause);
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
        let (actor, mut source, operation, receipt, _) = fixture(kind);
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
    let (actor, source, operation, receipt, _) =
        fixture(SourceOperationKind::ConnectionReviewApply);
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
        result: Ok(receipt.reservation.observed().unwrap().clone()),
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

#[test]
fn cleanup_target_is_installed_once_and_retained_after_uncertainty() {
    let (_, source, operation, receipt, target) =
        fixture(SourceOperationKind::ConnectionDisconnect);
    let target = crate::SourceCleanupTarget::from_observation(&operation, target).unwrap();
    let prepare = SourceOperationChange {
        operation_id: operation.operation_id,
        expected_revision: operation.revision,
        expected_source: operation.expected.clone(),
        next_phase: SourceOperationPhase::CleaningUp {
            receipt_id: receipt.commit_id,
            cleanup_revision: 1,
        },
        proof: SourceOperationProof::Committed(receipt.clone()),
        successor: None,
        prepare_cleanup: Some(target.clone()),
    };
    let cleaning = prepare.validate(&operation, Some(&source)).unwrap();
    assert_eq!(cleaning.cleanup_target.as_ref(), Some(&target));
    let mut repair = SourceOperationChange {
        operation_id: cleaning.operation_id,
        expected_revision: cleaning.revision,
        expected_source: cleaning.expected.clone(),
        next_phase: SourceOperationPhase::RepairRequired {
            receipt_id: receipt.commit_id,
            reason: SourceRepairReason::CleanupUncertain,
        },
        proof: SourceOperationProof::Committed(receipt.clone()),
        successor: None,
        prepare_cleanup: None,
    };
    let uncertain = repair.validate(&cleaning, Some(&source)).unwrap();
    let restored: SourceOperationRecord =
        serde_json::from_slice(&serde_json::to_vec(&uncertain).unwrap()).unwrap();
    assert_eq!(restored.cleanup_target.as_ref(), Some(&target));
    repair.expected_revision = restored.revision;
    repair.next_phase = SourceOperationPhase::CleaningUp {
        receipt_id: receipt.commit_id,
        cleanup_revision: 1,
    };
    let mut changed = target.clone();
    changed.provider_revision = 24;
    repair.prepare_cleanup = Some(changed);
    assert!(repair.validate(&restored, Some(&source)).is_err());
    repair.prepare_cleanup = None;
    assert_eq!(
        repair
            .validate(&restored, Some(&source))
            .unwrap()
            .cleanup_target,
        Some(target)
    );
    let mut unprepared = prepare.clone();
    unprepared.prepare_cleanup = None;
    assert!(unprepared.validate(&operation, Some(&source)).is_err());
}

#[test]
fn minimal_reduction_reservation_cannot_admit_review_or_wrong_person() {
    let (actor, source, record, receipt, _) = fixture(SourceOperationKind::ConnectionObservePause);
    assert!(
        receipt
            .reservation
            .validate_for(&GrantCommitKind::Reviewed {
                review: ReviewRef {
                    id: Uuid::new_v4(),
                    revision: 1,
                    digest: [1; 32]
                }
            })
            .is_err()
    );
    assert!(
        receipt
            .reservation
            .validate_for(&GrantCommitKind::InvalidateSource)
            .is_err()
    );
    let mut absent = record.identity();
    absent.source_revision = None;
    assert!(
        GrantReservation::Reduction(absent)
            .validate_for(&GrantCommitKind::PauseObserve)
            .is_err()
    );
    let mut other = actor.clone();
    other.person_id = PersonId::new();
    assert!(
        prepare_reduction_reservation(
            &other,
            Uuid::new_v4(),
            [1; 32],
            &source,
            SourceOperationKind::ConnectionObservePause
        )
        .is_err()
    );
    assert!(
        prepare_reduction_reservation(
            &actor,
            Uuid::new_v4(),
            [1; 32],
            &source,
            SourceOperationKind::ConnectionReviewApply
        )
        .is_err()
    );
}

#[test]
fn cleanup_target_is_bounded_and_rejects_changed_source_observation() {
    let (_, _, operation, _, mut observed) = fixture(SourceOperationKind::ConnectionDisconnect);
    observed.physical_resources = (0..floe_context_contract::MAX_SOURCE_RESOURCES)
        .map(|index| ResourceHandle::try_new(format!("resource.{index:08}")).unwrap())
        .collect();
    let target =
        crate::SourceCleanupTarget::from_observation(&operation, observed.clone()).unwrap();
    assert!(serde_json::to_vec(&target).unwrap().len() < 4096);
    assert_eq!(target.provider_revision, 23);
    observed.authority = SourceAuthority::new();
    assert!(crate::SourceCleanupTarget::from_observation(&operation, observed).is_err());
}
