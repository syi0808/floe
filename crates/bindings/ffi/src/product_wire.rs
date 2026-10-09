//! Product DTO validation and conversion at the FFI boundary.
//!
//! Product execution lives in AppHost's typed router. This module translates
//! the stable wire DTOs to that API and projects its owner results back.

use crate::app_wire::{
    AppCommandFailure, AppCommandResult, AppWireResult, agent_failure, day_error, internal_error,
    structural_error, validation,
};
use floe_app::{
    AppComposition, CallerContext, DayCommand, DayProductQuery, ProductCommand,
    ProductCommandDisposition, ProductCommandFailure, ProductCommandOutcome, ProductCommandRequest,
    ProductFailure, ProductObservation, ProductObservationOutcome, ProductQuery,
    ProductQueryOutcome,
};
use floe_kernel::CommandId;
use floe_protocol::*;
use uuid::Uuid;

pub(crate) fn command(
    host_request: &floe_app::HostRequest<'_, AppComposition>,
    command_id: Uuid,
    value: AppProductCommandDto,
) -> AppCommandResult<AppCommandResultDto> {
    let command = command_in(value).map_err(AppCommandFailure::NotAdmitted)?;
    let id = CommandId::from_uuid(command_id)
        .ok_or_else(|| AppCommandFailure::NotAdmitted(validation("command_id")))?;
    let result = host_request
        .product_command(ProductCommandRequest {
            command_id: id,
            command: command.clone(),
        })
        .map_err(|failure| command_failure(failure, command_id))?;
    command_out(&command, command_id, host_request.caller(), result)
        .map_err(AppCommandFailure::Indeterminate)
}

fn command_in(value: AppProductCommandDto) -> AppWireResult<ProductCommand> {
    use AppProductCommandDto as C;
    Ok(match value {
        command @ (C::ConversationSessionStart { .. }
        | C::ConversationSetCalendarPolicy { .. }
        | C::ConversationStartTurn { .. }
        | C::ConversationCalendarProposalSubmit { .. }
        | C::ConversationCancelRun { .. }
        | C::ConversationInteractionResolve { .. }
        | C::ConversationInteractionRefresh { .. }
        | C::AssistantFeatureSourcePrepareReview { .. }
        | C::AssistantFeatureConfigure { .. }) => {
            ProductCommand::Conversation(crate::conversation_wire::command_in(command)?)
        }
        command @ (C::ConnectionsPairingStart { .. }
        | C::ConnectionsPairingCancel { .. }
        | C::ConnectionsGatewayForget { .. }
        | C::ConnectionsIntegrationPrepareReview { .. }
        | C::ConnectionsIntegrationStart { .. }
        | C::ConnectionsOperationCancel { .. }
        | C::ConnectionsSourcePrepareReview { .. }
        | C::ConnectionsSourceConfigure { .. }
        | C::ConnectionsDisconnect { .. }
        | C::ConnectionsObservePrepareReview { .. }
        | C::ConnectionsObserveSet { .. }
        | C::ConnectionsGatewayManagementLaunch { .. }) => {
            ProductCommand::Connections(crate::connections_wire::command_in(command)?)
        }
        C::DayRefresh { day } => ProductCommand::Day(DayCommand::Refresh(
            crate::day_wire::read(day).map_err(structural_error)?,
        )),
        C::DayMutate { day, mutation } => ProductCommand::Day(DayCommand::Mutate {
            day: crate::day_wire::read(day).map_err(structural_error)?,
            mutation: crate::day_wire::mutation(mutation).map_err(structural_error)?,
        }),
        C::DayExternalCalendarOperation { operation } => {
            ProductCommand::Day(DayCommand::ExternalCalendarOperation {
                operation: crate::day_wire::manual_operation(operation)
                    .map_err(structural_error)?,
            })
        }
        C::DayExternalCalendarOperationReconcile {
            operation_ref,
            expected_revision,
        } => ProductCommand::Day(DayCommand::ReconcileExternalCalendarOperation {
            operation_ref: operation_ref.get(),
            expected_revision,
        }),
        command @ C::MemoryDecide { .. } => {
            ProductCommand::Memory(crate::memory_wire::command_in(command)?)
        }
    })
}

fn command_out(
    command: &ProductCommand,
    command_id: Uuid,
    caller: &CallerContext,
    result: ProductCommandOutcome,
) -> AppWireResult<AppCommandResultDto> {
    Ok(match (command, result) {
        (ProductCommand::Conversation(request), ProductCommandOutcome::Conversation(value)) => {
            return crate::conversation_wire::command_out(request, command_id, caller, value);
        }
        (ProductCommand::Connections(request), ProductCommandOutcome::Connections(value)) => {
            return crate::connections_wire::command_out(request, value);
        }
        (
            ProductCommand::Day(DayCommand::Refresh(_)),
            ProductCommandOutcome::Day(floe_app::DayCommandOutcome::Refresh(value)),
        ) => AppCommandResultDto::DayRefresh {
            refresh: crate::day_wire::refresh(value).map_err(structural_error)?,
        },
        (
            ProductCommand::Day(DayCommand::Mutate { .. }),
            ProductCommandOutcome::Day(floe_app::DayCommandOutcome::Mutation(value)),
        ) => {
            if value.command_id != command_id {
                return Err(internal_error());
            }
            AppCommandResultDto::DayMutation {
                command_id: value.command_id,
                mutation: MutationResultDto {
                    snapshot: crate::conversion::day_snapshot_to_dto(value.snapshot).map_err(
                        |failure| structural_error(floe_protocol::wire::conversion_error(failure)),
                    )?,
                    changed_item: value
                        .changed_item
                        .map(crate::conversion::timeline_item_to_dto)
                        .transpose()
                        .map_err(|failure| {
                            structural_error(floe_protocol::wire::conversion_error(failure))
                        })?,
                    capture: value.capture.map(crate::conversion::capture_to_dto),
                },
            }
        }
        (
            ProductCommand::Day(DayCommand::ExternalCalendarOperation { .. }),
            ProductCommandOutcome::Day(floe_app::DayCommandOutcome::ExternalCalendarOperation(
                value,
            )),
        )
        | (
            ProductCommand::Day(DayCommand::ReconcileExternalCalendarOperation { .. }),
            ProductCommandOutcome::Day(
                floe_app::DayCommandOutcome::ReconciledExternalCalendarOperation(value),
            ),
        ) => AppCommandResultDto::DayExternalCalendarOperation {
            operation: crate::day_wire::manual_operation_receipt(value)
                .map_err(structural_error)?,
        },
        (ProductCommand::Memory(request), ProductCommandOutcome::Memory(value)) => {
            return crate::memory_wire::command_out(request, command_id, value);
        }
        _ => return Err(internal_error()),
    })
}

pub(crate) fn query(
    host_request: &floe_app::HostRequest<'_, AppComposition>,
    query: AppProductQueryDto,
) -> AppWireResult<AppQueryResultDto> {
    let query = query_in(query)?;
    let result = host_request
        .product_query(query.clone())
        .map_err(|failure| product_failure(failure, host_request.request_id()))?;
    query_out(&query, host_request.caller(), result)
}

fn query_in(value: AppProductQueryDto) -> AppWireResult<ProductQuery> {
    use AppProductQueryDto as Q;
    Ok(match value {
        query @ (Q::ConversationSessionResume { .. }
        | Q::ConversationCalendarPolicy { .. }
        | Q::ConversationSessionGet { .. }
        | Q::ConversationGetCommand { .. }
        | Q::ConversationGetRun { .. }
        | Q::ConversationGetMessage { .. }
        | Q::ConversationInteractionGet { .. }
        | Q::ConversationInteractionList { .. }
        | Q::AssistantFeatureSnapshot { .. }
        | Q::AssistantFeatureSourceReviewInspect { .. }) => {
            ProductQuery::Conversation(crate::conversation_wire::query_in(query)?)
        }
        query @ (Q::ConnectionsOverview { .. }
        | Q::ConnectionsPairingGet { .. }
        | Q::ConnectionsGatewayGet { .. }
        | Q::ConnectionsIntegrationInspectReview { .. }
        | Q::ConnectionsOperationGet { .. }
        | Q::ConnectionsSourceInspectReview { .. }
        | Q::ConnectionsObserveInspectReview { .. }) => {
            ProductQuery::Connections(crate::connections_wire::query_in(query)?)
        }
        Q::DaySnapshot { day } => ProductQuery::Day(DayProductQuery::Snapshot(
            crate::day_wire::read(day).map_err(structural_error)?,
        )),
        Q::DayRefreshGet { operation_ref } => ProductQuery::Day(DayProductQuery::RefreshGet {
            operation_ref: operation_ref.get(),
        }),
        Q::DayCalendarDestinations {} => {
            ProductQuery::Day(DayProductQuery::ExternalCalendarDestinations)
        }
        Q::DayExternalCalendarOperationGet { operation_ref } => {
            ProductQuery::Day(DayProductQuery::ExternalCalendarOperationGet {
                operation_ref: operation_ref.get(),
            })
        }
        Q::DayExternalCalendarOperations { cursor, limit } => {
            ProductQuery::Day(DayProductQuery::ExternalCalendarOperations {
                cursor: cursor.map(|cursor| cursor.get()),
                limit,
            })
        }
        query @ (Q::MemoryOverview {} | Q::MemoryReview {}) => {
            ProductQuery::Memory(crate::memory_wire::query_in(query)?)
        }
    })
}

fn query_out(
    query: &ProductQuery,
    caller: &CallerContext,
    result: ProductQueryOutcome,
) -> AppWireResult<AppQueryResultDto> {
    Ok(match (query, result) {
        (ProductQuery::Conversation(request), ProductQueryOutcome::Conversation(value)) => {
            return crate::conversation_wire::query_out(request, caller, value);
        }
        (ProductQuery::Connections(request), ProductQueryOutcome::Connections(value)) => {
            return crate::connections_wire::query_out(request, value);
        }
        (
            ProductQuery::Day(DayProductQuery::Snapshot(_)),
            ProductQueryOutcome::Day(floe_app::DayQueryOutcome::Snapshot(value)),
        ) => AppQueryResultDto::DaySnapshot {
            snapshot: crate::conversion::day_snapshot_to_dto(value).map_err(|failure| {
                structural_error(floe_protocol::wire::conversion_error(failure))
            })?,
        },
        (
            ProductQuery::Day(DayProductQuery::RefreshGet { operation_ref }),
            ProductQueryOutcome::Day(floe_app::DayQueryOutcome::Refresh(value)),
        ) => {
            if value.operation_ref != *operation_ref {
                return Err(internal_error());
            }
            AppQueryResultDto::DayRefresh {
                refresh: crate::day_wire::refresh(value).map_err(structural_error)?,
            }
        }
        (
            ProductQuery::Day(DayProductQuery::ExternalCalendarDestinations),
            ProductQueryOutcome::Day(floe_app::DayQueryOutcome::ExternalCalendarDestinations(
                value,
            )),
        ) => AppQueryResultDto::DayCalendarDestinations {
            destinations: crate::day_wire::manual_destinations(value).map_err(structural_error)?,
        },
        (
            ProductQuery::Day(DayProductQuery::ExternalCalendarOperationGet { operation_ref }),
            ProductQueryOutcome::Day(floe_app::DayQueryOutcome::ExternalCalendarOperation(value)),
        ) => {
            let operation =
                crate::day_wire::manual_operation_receipt(value).map_err(structural_error)?;
            if operation.operation_ref.get() != *operation_ref {
                return Err(internal_error());
            }
            AppQueryResultDto::DayExternalCalendarOperation { operation }
        }
        (
            ProductQuery::Day(DayProductQuery::ExternalCalendarOperations { .. }),
            ProductQueryOutcome::Day(floe_app::DayQueryOutcome::ExternalCalendarOperations(value)),
        ) => AppQueryResultDto::DayExternalCalendarOperations {
            operations: crate::day_wire::manual_operation_receipts(value)
                .map_err(structural_error)?,
        },
        (ProductQuery::Memory(request), ProductQueryOutcome::Memory(value)) => {
            return crate::memory_wire::query_out(request, value, caller);
        }
        _ => return Err(internal_error()),
    })
}

pub(crate) fn observe(
    host_request: &floe_app::HostRequest<'_, AppComposition>,
    request: AppEventsRequestDto,
) -> AppWireResult<AppEventsResultDto> {
    let result = host_request
        .observe_product(ProductObservation {
            runtime_epoch: request.runtime_epoch,
            cursor: request.cursor,
            limit: request.limit,
        })
        .map_err(|failure| product_failure(failure, host_request.request_id()))?;
    let ProductObservationOutcome::Conversation(read) = result;
    crate::conversation_wire::events_out(host_request.caller(), read)
}

fn command_failure(failure: ProductCommandFailure, correlation_id: Uuid) -> AppCommandFailure {
    let disposition = failure.disposition;
    let error = product_failure(failure.failure, correlation_id);
    match disposition {
        ProductCommandDisposition::NotAdmitted => AppCommandFailure::NotAdmitted(error),
        ProductCommandDisposition::NotApplied => AppCommandFailure::NotApplied(error),
        ProductCommandDisposition::Admitted => AppCommandFailure::Admitted(error),
        ProductCommandDisposition::Indeterminate => AppCommandFailure::Indeterminate(error),
    }
}

fn product_failure(failure: ProductFailure, correlation_id: Uuid) -> AppWireErrorDto {
    match failure {
        ProductFailure::Conversation(reason) => {
            crate::conversation_wire::failure_dto(reason, correlation_id)
        }
        ProductFailure::Connections(reason) | ProductFailure::Memory(reason) => {
            agent_failure(reason)
        }
        ProductFailure::Day(reason) => day_error(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_command_disposition_survives_ffi_and_app_wire_projection() {
        let request_id = Uuid::new_v4();
        let cases = [
            (
                ProductCommandDisposition::NotAdmitted,
                AppCommandDispositionDto::NotAdmitted,
            ),
            (
                ProductCommandDisposition::NotApplied,
                AppCommandDispositionDto::NotApplied,
            ),
            (
                ProductCommandDisposition::Admitted,
                AppCommandDispositionDto::Admitted,
            ),
            (
                ProductCommandDisposition::Indeterminate,
                AppCommandDispositionDto::Indeterminate,
            ),
        ];

        for (route_disposition, wire_disposition) in cases {
            let failure = ProductCommandFailure {
                disposition: route_disposition,
                failure: ProductFailure::Conversation(floe_kernel::AgentFailure::ModelUnavailable),
            };
            let failure = command_failure(failure, request_id);
            let (disposition, error) = failure.into_parts();
            assert_eq!(disposition, wire_disposition);

            let response =
                AppResponseDto::<serde_json::Value>::command_error(request_id, disposition, error);
            let response = serde_json::to_value(response).expect("serialize command failure");
            assert_eq!(response["status"], "command_error");
            assert_eq!(
                response["disposition"],
                serde_json::to_value(wire_disposition).unwrap()
            );
        }
    }
}
