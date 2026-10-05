//! Configure-only completion after durable Access invalidation. This workflow
//! needs source storage, native evidence and the shared owner lifetime only.
use super::source_operation::{check, source_error};
use crate::*;
use floe_access::GrantCommitReceipt;
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, OwnerActor};
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct ConfigurationFinalization<'a> {
    pub expected: &'a floe_access::SourceExpectation,
    pub original: &'a SourceConnection,
    pub successor: &'a SourceConnection,
    pub operation: SourceOperationRecord,
    pub receipt: GrantCommitReceipt,
}

pub(super) async fn finalize(
    sources: &dyn ConnectionsRepository,
    evidence: &dyn SourceReviewEvidence,
    closed: &AtomicBool,
    actor: &OwnerActor,
    request: ConfigurationFinalization<'_>,
    scope: &ExecutionScope,
) -> Result<SourceOperationRecord, AgentFailure> {
    let ConfigurationFinalization {
        expected,
        original,
        successor,
        mut operation,
        receipt,
    } = request;
    let check_live = || {
        if closed.load(Ordering::Acquire) {
            return Err(AgentFailure::Interrupted);
        }
        check(actor, scope)
    };
    check_live()?;
    receipt.validate()?;
    operation.validate().map_err(source_error)?;
    original
        .validate_successor(successor)
        .map_err(|_| AgentFailure::Conflict)?;
    if operation.kind != SourceOperationKind::ConnectionConfigure
        || operation.device_id != actor.device_id
        || original.person_id() != actor.person_id
        || receipt.kind != floe_access::GrantCommitKind::InvalidateSource
        || !operation.expected.matches(Some(original))
        || receipt.reservation.source != *expected
        || !operation.matches_evidence(&receipt.reservation)
        || receipt.reservation.source.gateway.is_some()
    {
        return Err(AgentFailure::Conflict);
    }
    let current = sources
        .load(actor.person_id, original.connection_id())
        .await
        .map_err(source_error)?;
    if current.as_ref() != Some(original) {
        return Err(AgentFailure::Conflict);
    }
    let digest = receipt.digest()?;
    if matches!(operation.phase, SourceOperationPhase::Reserved) {
        check_live()?;
        operation = sources
            .compare_and_swap_operation(SourceOperationChange {
                operation_id: operation.operation_id,
                expected_revision: operation.revision,
                expected_source: operation.expected.clone(),
                next_phase: SourceOperationPhase::GrantCommitted {
                    receipt_id: receipt.commit_id,
                    receipt_digest: digest,
                },
                proof: SourceOperationProof::Committed(receipt.clone()),
                successor: None,
            })
            .await
            .map_err(source_error)?;
    }
    match operation.phase {
        SourceOperationPhase::GrantCommitted {
            receipt_id,
            receipt_digest,
        } if receipt_id == receipt.commit_id && receipt_digest == digest => {}
        SourceOperationPhase::RepairRequired {
            receipt_id,
            reason: SourceRepairReason::SourceChanged,
        } if receipt_id == receipt.commit_id => {}
        _ => return Err(AgentFailure::Conflict),
    }
    check_live()?;
    // The old selection's native subject is deliberately not consulted: all of
    // its grants are paused and the reviewed candidate owns the next source fact.
    let candidate = evidence
        .inspect_selection(actor, successor, successor.resources(), scope)
        .await;
    check_live()?;
    let candidate = candidate?;
    if !floe_access::valid_subject_fingerprint(&candidate) {
        return Err(AgentFailure::PolicyDenied);
    }
    let (phase, successor) = if Some(candidate.as_str()) == successor.native_subject_fingerprint() {
        (
            SourceOperationPhase::Completed {
                receipt_id: receipt.commit_id,
            },
            Some(successor.clone()),
        )
    } else {
        (
            SourceOperationPhase::ConfigurationRejectedAfterInvalidation {
                receipt_id: receipt.commit_id,
            },
            None,
        )
    };
    sources
        .compare_and_swap_operation(SourceOperationChange {
            operation_id: operation.operation_id,
            expected_revision: operation.revision,
            expected_source: operation.expected.clone(),
            next_phase: phase,
            proof: SourceOperationProof::Committed(receipt),
            successor,
        })
        .await
        .map_err(source_error)
}
