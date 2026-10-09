use crate::{ActionIntent, ActionStatus, CalendarOperationsService};
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, CommandFailure, OwnerActor};
use uuid::Uuid;

impl floe_day::ExternalCalendarOperationPort for CalendarOperationsService {
    fn list_destinations<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<Vec<floe_day::ManualCalendarDestination>, floe_day::DayError>,
    > {
        Box::pin(async move {
            self.destinations(actor, scope)
                .await
                .map(|values| {
                    values
                        .into_iter()
                        .map(|choice| floe_day::ManualCalendarDestination {
                            destination_ref: choice.destination_ref,
                            label: choice.label,
                        })
                        .collect()
                })
                .map_err(day_failure)
        })
    }

    fn execute_manual<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: Uuid,
        operation: floe_day::ManualCalendarOperation,
        scope: &'a ExecutionScope,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_day::ManualCalendarOperationReceipt, CommandFailure<floe_day::DayError>>,
    > {
        Box::pin(async move {
            let intent = match operation {
                floe_day::ManualCalendarOperation::Create {
                    destination_ref,
                    title,
                    schedule,
                } => ActionIntent::DirectCreate {
                    destination_ref,
                    title,
                    schedule,
                },
                floe_day::ManualCalendarOperation::Update {
                    event_ref,
                    expected_revision,
                    title,
                    schedule,
                } => ActionIntent::DirectUpdate {
                    event_ref,
                    expected_revision,
                    title,
                    schedule,
                },
                floe_day::ManualCalendarOperation::Delete {
                    event_ref,
                    expected_revision,
                } => ActionIntent::DirectDelete {
                    event_ref,
                    expected_revision,
                },
            };
            let snapshot = self
                .submit(actor, command_id, intent, scope)
                .await
                .map_err(|failure| failure.map_failure(day_failure))?;
            day_receipt(snapshot).map_err(CommandFailure::Admitted)
        })
    }

    fn inspect_manual<'a>(
        &'a self,
        actor: &'a OwnerActor,
        operation_id: Uuid,
        scope: &'a ExecutionScope,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_day::ManualCalendarOperationReceipt, floe_day::DayError>,
    > {
        Box::pin(async move {
            let snapshot = self
                .inspect(actor, operation_id, scope)
                .await
                .map_err(day_failure)?;
            if snapshot.origin != crate::ActionOriginKind::Direct {
                return Err(floe_day::DayError::validation(
                    "operation is not a direct Day command",
                ));
            }
            day_receipt(snapshot)
        })
    }

    fn list_manual<'a>(
        &'a self,
        actor: &'a OwnerActor,
        cursor: Option<Uuid>,
        limit: u16,
        scope: &'a ExecutionScope,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_day::ManualCalendarOperationPage, floe_day::DayError>,
    > {
        Box::pin(async move {
            if !(1..=100).contains(&limit) {
                return Err(floe_day::DayError::validation("invalid operation limit"));
            }
            let page = self
                .list_direct(actor, cursor, limit, scope)
                .await
                .map_err(day_failure)?;
            let operations = page
                .actions
                .into_iter()
                .map(day_receipt)
                .collect::<Result<Vec<_>, _>>()?;
            let result = floe_day::ManualCalendarOperationPage {
                operations,
                next_cursor: page.next_cursor,
            };
            result.validate(limit).map_err(day_failure)?;
            Ok(result)
        })
    }

    fn reconcile_manual<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: Uuid,
        operation_id: Uuid,
        expected_revision: u64,
        scope: &'a ExecutionScope,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_day::ManualCalendarOperationReceipt, CommandFailure<floe_day::DayError>>,
    > {
        Box::pin(async move {
            let snapshot = self
                .reconcile_direct(actor, command_id, operation_id, expected_revision, scope)
                .await
                .map_err(|failure| failure.map_failure(day_failure))?;
            day_receipt(snapshot).map_err(CommandFailure::Admitted)
        })
    }
}

fn day_receipt(
    snapshot: crate::ActionSnapshot,
) -> Result<floe_day::ManualCalendarOperationReceipt, floe_day::DayError> {
    let (status, collection_pending) = match snapshot.status {
        ActionStatus::PendingReview | ActionStatus::Approved => {
            (floe_day::ManualCalendarOperationStatus::Pending, false)
        }
        ActionStatus::Executing => (floe_day::ManualCalendarOperationStatus::Executing, false),
        ActionStatus::Blocked { .. } => (floe_day::ManualCalendarOperationStatus::Blocked, false),
        ActionStatus::Failed { .. }
        | ActionStatus::Rejected
        | ActionStatus::Cancelled
        | ActionStatus::Expired => (floe_day::ManualCalendarOperationStatus::NotApplied, false),
        ActionStatus::Unknown { .. } => (floe_day::ManualCalendarOperationStatus::Unknown, false),
        ActionStatus::Succeeded { collection } => (
            floe_day::ManualCalendarOperationStatus::Succeeded,
            collection == crate::ActionCollectionStatus::Pending,
        ),
    };
    let receipt = floe_day::ManualCalendarOperationReceipt {
        operation_id: snapshot.action_ref,
        revision: snapshot.revision,
        status,
        collection_pending,
    };
    receipt.validate().map_err(day_failure)?;
    Ok(receipt)
}

fn day_failure(error: AgentFailure) -> floe_day::DayError {
    match error {
        AgentFailure::InvalidInput => {
            floe_day::DayError::validation("invalid external calendar command")
        }
        AgentFailure::NotFound => floe_day::DayError::not_found("calendar event", "requested"),
        AgentFailure::Conflict
        | AgentFailure::PolicyDenied
        | AgentFailure::StaleContext
        | AgentFailure::AccessReviewRequired => {
            floe_day::DayError::conflict("calendar operation could not be admitted")
        }
        _ => floe_day::DayError::storage("Calendar Operations is unavailable"),
    }
}
