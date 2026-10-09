use crate::{
    DayError, ManualCalendarDestination, ManualCalendarOperation, ManualCalendarOperationPage,
    ManualCalendarOperationReceipt,
};
use floe_execution::BoxFuture;
use floe_execution::ExecutionScope;
use floe_kernel::{CommandFailure, OwnerActor};
use uuid::Uuid;

/// Implemented by Calendar Operations. Day invokes it after local actor
/// admission and without holding the Day mutation lock.
pub trait ExternalCalendarOperationPort: Send + Sync {
    fn list_destinations<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Vec<ManualCalendarDestination>, DayError>>;

    fn execute_manual<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: Uuid,
        operation: ManualCalendarOperation,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ManualCalendarOperationReceipt, CommandFailure<DayError>>>;

    /// Query a durable operation after submission, including after the Day
    /// view or its transport has been replaced.
    fn inspect_manual<'a>(
        &'a self,
        actor: &'a OwnerActor,
        operation_id: Uuid,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ManualCalendarOperationReceipt, DayError>>;

    /// Return the bounded recent direct Day operations for caller recovery.
    fn list_manual<'a>(
        &'a self,
        actor: &'a OwnerActor,
        cursor: Option<Uuid>,
        limit: u16,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ManualCalendarOperationPage, DayError>>;

    fn reconcile_manual<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: Uuid,
        operation_id: Uuid,
        expected_revision: u64,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ManualCalendarOperationReceipt, CommandFailure<DayError>>>;
}
