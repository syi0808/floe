use crate::{
    SourceOperationAdmission, SourceOperationChange, SourceOperationRecord,
    SourceOperationReservation, SourceRepositoryError,
};
use floe_context_contract::ConnectionId;
use floe_execution::BoxFuture;
use floe_kernel::PersonId;
use uuid::Uuid;
/// Compare-only evidence of completed as well as active source reservations.
/// The storage insertion ordinal is private and never becomes authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceReservationWatermark {
    NeverReserved,
    Reserved {
        operation_id: Uuid,
        reservation_id: Uuid,
        reservation_generation: u64,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceReservationFence {
    pub watermark: SourceReservationWatermark,
    pub fenced: bool,
}
impl SourceReservationFence {
    pub fn validate(&self) -> Result<(), SourceRepositoryError> {
        match &self.watermark {
            SourceReservationWatermark::NeverReserved if !self.fenced => Ok(()),
            SourceReservationWatermark::Reserved {
                operation_id,
                reservation_id,
                reservation_generation,
            } if !operation_id.is_nil()
                && !reservation_id.is_nil()
                && *reservation_generation > 0 =>
            {
                Ok(())
            }
            _ => Err(SourceRepositoryError::Corrupt),
        }
    }
}

pub trait SourceOperationRepository: Send + Sync {
    fn reserve<'a>(
        &'a self,
        request: SourceOperationReservation,
    ) -> BoxFuture<'a, Result<SourceOperationAdmission, SourceRepositoryError>>;
    fn load_operation<'a>(
        &'a self,
        id: Uuid,
    ) -> BoxFuture<'a, Result<Option<SourceOperationRecord>, SourceRepositoryError>>;
    fn compare_and_swap_operation<'a>(
        &'a self,
        change: SourceOperationChange,
    ) -> BoxFuture<'a, Result<SourceOperationRecord, SourceRepositoryError>>;
    fn list_nonterminal<'a>(
        &'a self,
        person_id: PersonId,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<SourceOperationRecord>, SourceRepositoryError>>;
    fn read_reservation_fence<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<SourceReservationFence, SourceRepositoryError>>;
    fn source_is_fenced<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<bool, SourceRepositoryError>>;
}

pub trait ConnectionsRepository: crate::SourceRepository + SourceOperationRepository {}
impl<T: crate::SourceRepository + SourceOperationRepository + ?Sized> ConnectionsRepository for T {}
