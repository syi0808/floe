use crate::{SourceConnection, SourceRepositoryError};
use floe_access::{
    GrantAbortReceipt, GrantCommitKind, GrantCommitReceipt, ReviewRef, SourceReservationEvidence,
};
use floe_context_contract::{GrantSourceBinding, SourceAuthority};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceOperationExpectation {
    pub source: GrantSourceBinding,
    pub revision: Option<u64>,
    pub authority: SourceAuthority,
}
impl SourceOperationExpectation {
    pub fn matches(&self, current: Option<&SourceConnection>) -> bool {
        match (self.revision, current) {
            (None, None) => true,
            (Some(revision), Some(current)) => {
                current.revision() == revision
                    && current.person_id() == self.source.person_id()
                    && current.connector_id() == self.source.connector()
                    && *current.connection_id() == self.source.connection_id()
                    && current.execution_owner_id() == self.source.execution_owner()
                    && current.source_authority() == self.authority
            }
            _ => false,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceOperationKind {
    ConnectionReviewApply,
    ConnectionDisconnect,
    ConnectionObservePause,
    ConnectionConfigure,
    ConnectionPresentation,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceOperationPhase {
    Reserved,
    PresentationCommitted,
    ConfigurationRejectedAfterInvalidation {
        receipt_id: Uuid,
    },
    GrantCommitted {
        receipt_id: Uuid,
        receipt_digest: [u8; 32],
    },
    CleaningUp {
        receipt_id: Uuid,
        cleanup_revision: u64,
    },
    Completed {
        receipt_id: Uuid,
    },
    Aborted {
        reason: SourceAbortReason,
    },
    RepairRequired {
        receipt_id: Uuid,
        reason: SourceRepairReason,
    },
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceAbortReason {
    ReviewChanged,
    ReviewExpired,
    Cancelled,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRepairReason {
    SourceChanged,
    CleanupUncertain,
    ReceiptMismatch,
}
impl SourceOperationPhase {
    pub fn holds_fence(&self) -> bool {
        !matches!(
            self,
            Self::Completed { .. }
                | Self::PresentationCommitted
                | Self::Aborted { .. }
                | Self::ConfigurationRejectedAfterInvalidation { .. }
        )
    }
    pub fn receipt_id(&self) -> Option<Uuid> {
        match self {
            Self::ConfigurationRejectedAfterInvalidation { receipt_id }
            | Self::GrantCommitted { receipt_id, .. }
            | Self::CleaningUp { receipt_id, .. }
            | Self::Completed { receipt_id }
            | Self::RepairRequired { receipt_id, .. } => Some(*receipt_id),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceOperationRecord {
    pub device_id: String,
    pub operation_id: Uuid,
    pub command_id: Uuid,
    pub request_digest: [u8; 32],
    pub reservation_id: Uuid,
    pub reservation_generation: u64,
    pub expected: SourceOperationExpectation,
    pub review: Option<ReviewRef>,
    pub kind: SourceOperationKind,
    pub phase: SourceOperationPhase,
    pub revision: u64,
}
impl SourceOperationRecord {
    pub fn validate(&self) -> Result<(), SourceRepositoryError> {
        if self.operation_id.is_nil()
            || self.command_id.is_nil()
            || self.reservation_id.is_nil()
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.device_id.chars().any(char::is_control)
            || self.request_digest == [0; 32]
            || self.reservation_generation == 0
            || self.revision == 0
            || self.revision > i64::MAX as u64
            || self.expected.source.validate().is_err()
            || !self.expected.authority.is_valid()
            || self.expected.revision == Some(0)
            || self.phase.receipt_id().is_some_and(|id| id.is_nil())
            || matches!(
                self.kind,
                SourceOperationKind::ConnectionReviewApply
                    | SourceOperationKind::ConnectionConfigure
                    | SourceOperationKind::ConnectionPresentation
            ) != self.review.is_some()
            || self
                .review
                .as_ref()
                .is_some_and(|review| review.validate().is_err())
            || (self.kind == SourceOperationKind::ConnectionPresentation
                && self.expected.revision.is_none())
            || (self.kind == SourceOperationKind::ConnectionPresentation
                && !matches!(
                    self.phase,
                    SourceOperationPhase::Reserved
                        | SourceOperationPhase::PresentationCommitted
                        | SourceOperationPhase::Aborted { .. }
                ))
            || (self.kind != SourceOperationKind::ConnectionPresentation
                && matches!(self.phase, SourceOperationPhase::PresentationCommitted))
            || (matches!(
                self.phase,
                SourceOperationPhase::ConfigurationRejectedAfterInvalidation { .. }
            ) && self.kind != SourceOperationKind::ConnectionConfigure)
            || self.identity().validate().is_err()
        {
            return Err(SourceRepositoryError::Corrupt);
        }
        Ok(())
    }
    pub fn identity(&self) -> floe_access::GrantOperationIdentity {
        floe_access::GrantOperationIdentity {
            device_id: self.device_id.clone(),
            operation_id: self.operation_id,
            command_id: self.command_id,
            request_digest: self.request_digest,
            reservation_id: self.reservation_id,
            reservation_generation: self.reservation_generation,
            source: self.expected.source.clone(),
            source_revision: self.expected.revision,
            source_authority: self.expected.authority,
        }
    }
    pub fn matches_evidence(&self, evidence: &SourceReservationEvidence) -> bool {
        self.device_id == evidence.device_id
            && self.operation_id == evidence.operation_id
            && self.command_id == evidence.command_id
            && self.request_digest == evidence.request_digest
            && self.reservation_id == evidence.reservation_id
            && self.reservation_generation == evidence.reservation_generation
            && self.expected.source == evidence.source.source
            && self.expected.revision == evidence.source.revision
            && self.expected.authority == evidence.source.authority
    }
}
#[derive(Clone, Debug)]
pub struct SourceOperationReservation {
    pub record: SourceOperationRecord,
}
#[derive(Clone, Debug)]
pub struct SourceOperationAdmission {
    pub record: SourceOperationRecord,
    pub replayed: bool,
}
#[derive(Clone, Debug)]
pub enum SourceOperationProof {
    Committed(GrantCommitReceipt),
    Aborted(GrantAbortReceipt),
}
#[derive(Clone, Debug)]
pub struct SourceOperationChange {
    pub operation_id: Uuid,
    pub expected_revision: u64,
    pub expected_source: SourceOperationExpectation,
    pub next_phase: SourceOperationPhase,
    pub proof: SourceOperationProof,
    pub successor: Option<SourceConnection>,
}
impl SourceOperationChange {
    pub fn validate(
        &self,
        current: &SourceOperationRecord,
        source: Option<&SourceConnection>,
    ) -> Result<SourceOperationRecord, SourceRepositoryError> {
        current.validate()?;
        if self.operation_id != current.operation_id
            || self.expected_revision != current.revision
            || self.expected_source != current.expected
            || !current.phase.holds_fence()
        {
            return Err(SourceRepositoryError::Conflict);
        }
        if current.kind == SourceOperationKind::ConnectionPresentation {
            return Err(SourceRepositoryError::Conflict);
        }
        let committed = match &self.proof {
            SourceOperationProof::Committed(receipt) => {
                if !current.matches_evidence(&receipt.reservation)
                    || receipt.commit_id.is_nil()
                    || !matches!(
                        (&current.kind, &receipt.kind),
                        (
                            SourceOperationKind::ConnectionReviewApply,
                            GrantCommitKind::Reviewed { .. }
                        ) | (
                            SourceOperationKind::ConnectionDisconnect,
                            GrantCommitKind::Disconnect
                        ) | (
                            SourceOperationKind::ConnectionObservePause,
                            GrantCommitKind::PauseObserve
                        ) | (
                            SourceOperationKind::ConnectionConfigure,
                            GrantCommitKind::InvalidateSource
                        )
                    )
                    || matches!(&receipt.kind,GrantCommitKind::Reviewed{review} if current.review.as_ref()!=Some(review))
                    || current
                        .phase
                        .receipt_id()
                        .is_some_and(|id| id != receipt.commit_id)
                    || self.next_phase.receipt_id() != Some(receipt.commit_id)
                {
                    return Err(SourceRepositoryError::Conflict);
                }
                if let SourceOperationPhase::GrantCommitted { receipt_digest, .. } = self.next_phase
                {
                    if receipt
                        .digest()
                        .map_err(|_| SourceRepositoryError::Corrupt)?
                        != receipt_digest
                    {
                        return Err(SourceRepositoryError::Conflict);
                    }
                }
                if current.kind == SourceOperationKind::ConnectionConfigure {
                    if let SourceOperationPhase::GrantCommitted { receipt_digest, .. } =
                        current.phase
                    {
                        if receipt
                            .digest()
                            .map_err(|_| SourceRepositoryError::Corrupt)?
                            != receipt_digest
                        {
                            return Err(SourceRepositoryError::Conflict);
                        }
                    }
                }
                true
            }
            SourceOperationProof::Aborted(receipt) => {
                if current.identity() != receipt.identity
                    || receipt.abort_id.is_nil()
                    || !matches!(current.phase, SourceOperationPhase::Reserved)
                    || !matches!(self.next_phase, SourceOperationPhase::Aborted { .. })
                    || self.successor.is_some()
                {
                    return Err(SourceRepositoryError::Conflict);
                }
                false
            }
        };
        if committed
            && !matches!(self.next_phase, SourceOperationPhase::RepairRequired { .. })
            && !current.expected.matches(source)
        {
            return Err(SourceRepositoryError::Conflict);
        }
        if committed {
            let allowed = match (&current.phase, &self.next_phase) {
                (SourceOperationPhase::Reserved, SourceOperationPhase::GrantCommitted { .. }) => {
                    true
                }
                (
                    SourceOperationPhase::GrantCommitted { .. },
                    SourceOperationPhase::Completed { .. }
                    | SourceOperationPhase::CleaningUp { .. },
                ) => true,
                (
                    SourceOperationPhase::CleaningUp {
                        cleanup_revision: old,
                        ..
                    },
                    SourceOperationPhase::CleaningUp {
                        cleanup_revision: next,
                        ..
                    },
                ) => old.checked_add(1) == Some(*next),
                (
                    SourceOperationPhase::CleaningUp { .. },
                    SourceOperationPhase::Completed { .. },
                ) => true,
                (
                    SourceOperationPhase::RepairRequired {
                        reason: SourceRepairReason::CleanupUncertain,
                        ..
                    },
                    SourceOperationPhase::CleaningUp { .. },
                ) => true,
                (
                    SourceOperationPhase::GrantCommitted { .. }
                    | SourceOperationPhase::RepairRequired {
                        reason: SourceRepairReason::SourceChanged,
                        ..
                    },
                    SourceOperationPhase::ConfigurationRejectedAfterInvalidation { .. },
                ) if current.kind == SourceOperationKind::ConnectionConfigure => true,
                (
                    SourceOperationPhase::RepairRequired {
                        reason: SourceRepairReason::SourceChanged,
                        ..
                    },
                    SourceOperationPhase::Completed { .. },
                ) if current.kind == SourceOperationKind::ConnectionConfigure => true,
                (SourceOperationPhase::Reserved, SourceOperationPhase::RepairRequired { .. })
                    if current.kind == SourceOperationKind::ConnectionConfigure =>
                {
                    false
                }
                (_, SourceOperationPhase::RepairRequired { .. }) => true,
                _ => false,
            };
            if !allowed {
                return Err(SourceRepositoryError::Conflict);
            }
        }
        if committed
            && matches!(self.next_phase, SourceOperationPhase::Completed { .. })
            && matches!(
                current.kind,
                SourceOperationKind::ConnectionConfigure
                    | SourceOperationKind::ConnectionDisconnect
            )
            && self.successor.is_none()
        {
            return Err(SourceRepositoryError::Conflict);
        }
        if matches!(
            self.next_phase,
            SourceOperationPhase::ConfigurationRejectedAfterInvalidation { .. }
        ) && self.successor.is_some()
        {
            return Err(SourceRepositoryError::Conflict);
        }
        if let Some(next) = &self.successor {
            if !matches!(self.next_phase, SourceOperationPhase::Completed { .. }) {
                return Err(SourceRepositoryError::Conflict);
            }
            next.validate()
                .map_err(|_| SourceRepositoryError::Corrupt)?;
            match source {
                Some(source) => source
                    .validate_successor(next)
                    .map_err(|_| SourceRepositoryError::Conflict)?,
                None if self.expected_source.revision.is_none()
                    && next.revision() == 1
                    && next.person_id() == self.expected_source.source.person_id()
                    && next.connector_id() == self.expected_source.source.connector()
                    && *next.connection_id() == self.expected_source.source.connection_id()
                    && next.execution_owner_id()
                        == self.expected_source.source.execution_owner()
                    && next.source_authority() == self.expected_source.authority => {}
                _ => return Err(SourceRepositoryError::Conflict),
            }
        }
        // A repeated observation of the same repair receipt is not new state.
        if self.next_phase == current.phase && self.successor.is_none() {
            return Ok(current.clone());
        }
        let mut next = current.clone();
        next.phase = self.next_phase.clone();
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or(SourceRepositoryError::Conflict)?;
        next.validate()?;
        Ok(next)
    }
}

/// Native observation is complete before this local-only storage decision.
#[derive(Clone, Debug)]
pub enum SourcePresentationDecision {
    Apply(SourceConnection),
    Reject(SourceAbortReason),
}
impl SourceOperationReservation {
    pub fn settle_presentation(
        &self,
        source: Option<&SourceConnection>,
        decision: SourcePresentationDecision,
    ) -> Result<(SourceOperationRecord, Option<SourceConnection>), SourceRepositoryError> {
        self.record.validate()?;
        if self.record.kind != SourceOperationKind::ConnectionPresentation
            || self.record.revision != 1
            || self.record.phase != SourceOperationPhase::Reserved
        {
            return Err(SourceRepositoryError::Conflict);
        }
        let current = source.ok_or(SourceRepositoryError::Conflict)?;
        if current.person_id() != self.record.expected.source.person_id()
            || current.connector_id() != self.record.expected.source.connector()
            || *current.connection_id() != self.record.expected.source.connection_id()
            || current.execution_owner_id() != self.record.expected.source.execution_owner()
        {
            return Err(SourceRepositoryError::Conflict);
        }
        let mut record = self.record.clone();
        let successor = match decision {
            SourcePresentationDecision::Apply(next) => {
                if self.record.expected.matches(Some(current))
                    && current.is_presentation_successor(&next) == Ok(true)
                {
                    record.phase = SourceOperationPhase::PresentationCommitted;
                    Some(next)
                } else {
                    record.phase = SourceOperationPhase::Aborted {
                        reason: SourceAbortReason::ReviewChanged,
                    };
                    None
                }
            }
            SourcePresentationDecision::Reject(reason) => {
                record.phase = SourceOperationPhase::Aborted { reason };
                None
            }
        };
        record.validate()?;
        Ok((record, successor))
    }
}
