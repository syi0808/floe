//! The one stateless App route for admitted product commands, queries and
//! bounded observations. It owns dispatch only; product decisions stay with
//! their domain owners and external boundary conversion stays outside App.

use std::time::Duration;

use floe_execution::Cancellation;
use uuid::Uuid;

use crate::api::*;
use crate::{AppComposition, CallerContext, HostRequest};

const PRODUCT_SCOPE: Duration = Duration::from_secs(35);
// Day snapshots inspect external source metadata and must finish before the
// Flutter query's 35-second transport deadline.
const DAY_SNAPSHOT_SCOPE: Duration = Duration::from_secs(30);
const OBSERVATION_SCOPE: Duration = Duration::from_secs(5);

impl HostRequest<'_, AppComposition> {
    /// Route a typed product command while this request's AppHost admission is
    /// held. `command_id` remains independent of the transport request ID.
    pub fn product_command(
        &self,
        request: ProductCommandRequest,
    ) -> Result<ProductCommandOutcome, ProductCommandFailure> {
        route_command(self.services(), self.caller(), request)
    }

    /// Route a typed product query while this request's AppHost admission is
    /// held. Queries receive their exact admitted request identity as scope.
    pub fn product_query(
        &self,
        query: ProductQuery,
    ) -> Result<ProductQueryOutcome, ProductFailure> {
        route_query(self.services(), self.caller(), self.request_id(), query)
    }

    /// Read one bounded product observation. Dropping its caller only ends the
    /// observation; it never creates a Run cancellation request.
    pub fn observe_product(
        &self,
        observation: ProductObservation,
    ) -> Result<ProductObservationOutcome, ProductFailure> {
        route_observation(
            self.services(),
            self.caller(),
            self.request_id(),
            observation,
        )
    }
}

fn route_command(
    app: &AppComposition,
    caller: &CallerContext,
    request: ProductCommandRequest,
) -> Result<ProductCommandOutcome, ProductCommandFailure> {
    let command_id = request.command_id;
    if !command_id.is_valid() {
        return Err(command_failure(
            ProductCommandDisposition::NotAdmitted,
            ProductFailure::Conversation(floe_kernel::AgentFailure::InvalidInput),
        ));
    }
    let command_scope =
        || crate::host_scope(command_id.as_uuid(), Cancellation::new(), PRODUCT_SCOPE);

    match request.command {
        ProductCommand::Conversation(command) => {
            let owners = app.ready_owners(caller).map_err(|failure| {
                command_failure(
                    ProductCommandDisposition::NotAdmitted,
                    ProductFailure::Conversation(failure),
                )
            })?;
            let actor = caller.owner_actor();
            let scope = command_scope();
            app.execute_owner(async move {
                use floe_conversation::{
                    SessionStartAdmission as Admission, SessionStartFailure as StartFailure,
                };
                let service = owners.conversation.as_ref();
                let outcome = match command {
                    ConversationCommand::StartSession => {
                        let receipt = service
                            .start_session(&actor, command_id, &scope)
                            .await
                            .map_err(|failure| match failure {
                                StartFailure::NotAdmitted(reason) => command_failure(
                                    ProductCommandDisposition::NotAdmitted,
                                    ProductFailure::Conversation(reason),
                                ),
                                StartFailure::Indeterminate(reason) => command_failure(
                                    ProductCommandDisposition::Indeterminate,
                                    ProductFailure::Conversation(reason),
                                ),
                            })?;
                        let receipt = match receipt {
                            Admission::Started(receipt) | Admission::Replayed(receipt) => receipt,
                            Admission::NotApplied(refusal) => {
                                return Err(command_failure(
                                    ProductCommandDisposition::NotApplied,
                                    ProductFailure::Conversation(refusal.reason()),
                                ));
                            }
                        };
                        let session = service
                            .get_session(&actor, receipt.session_id, None, &scope)
                            .await
                            .map_err(|reason| {
                                command_failure(
                                    ProductCommandDisposition::Admitted,
                                    ProductFailure::Conversation(reason),
                                )
                            })?;
                        ConversationCommandOutcome::Session(session)
                    }
                    ConversationCommand::StartTurn {
                        session_id,
                        expected_revision,
                        text,
                        continuation_id,
                        retry_of,
                    } => {
                        let turn = floe_conversation::StartTurn {
                            command_id,
                            session_id,
                            expected_revision,
                            text,
                            continuation_ref: continuation_id
                                .map(|id| floe_conversation::ContinuationToken { id }),
                            retry_of,
                        };
                        let receipt =
                            service
                                .start_turn(&actor, turn, &scope)
                                .await
                                .map_err(|reason| {
                                    command_failure(
                                        ProductCommandDisposition::Indeterminate,
                                        ProductFailure::Conversation(reason),
                                    )
                                })?;
                        ConversationCommandOutcome::Turn(receipt)
                    }
                    ConversationCommand::CancelRun { run_id } => {
                        let receipt = service
                            .cancel_run(&actor, command_id, run_id, &scope)
                            .await
                            .map_err(|reason| {
                                command_failure(
                                    ProductCommandDisposition::Indeterminate,
                                    ProductFailure::Conversation(reason),
                                )
                            })?;
                        ConversationCommandOutcome::CancelRun(receipt)
                    }
                    ConversationCommand::ResolveInteraction {
                        interaction_id,
                        session_id,
                        expected_revision,
                        decision,
                        target_digest,
                    } => {
                        let result = service
                            .resolve_interaction(
                                &actor,
                                floe_conversation::ResolveInteraction {
                                    command_id: command_id.as_uuid(),
                                    interaction_id,
                                    session_id,
                                    expected_revision,
                                    decision,
                                    target_digest,
                                },
                                &scope,
                            )
                            .await
                            .map_err(|reason| {
                                command_failure(
                                    ProductCommandDisposition::Indeterminate,
                                    ProductFailure::Conversation(reason),
                                )
                            })?;
                        ConversationCommandOutcome::Interaction(result)
                    }
                    ConversationCommand::RefreshInteraction {
                        interaction_id,
                        session_id,
                        expected_revision,
                    } => {
                        let result = service
                            .refresh_interaction(
                                &actor,
                                floe_conversation::RefreshInteraction {
                                    command_id: command_id.as_uuid(),
                                    interaction_id,
                                    session_id,
                                    expected_revision,
                                },
                                &scope,
                            )
                            .await
                            .map_err(|reason| {
                                command_failure(
                                    ProductCommandDisposition::Indeterminate,
                                    ProductFailure::Conversation(reason),
                                )
                            })?;
                        ConversationCommandOutcome::InteractionRefresh(result)
                    }
                };
                Ok(ProductCommandOutcome::Conversation(outcome))
            })
        }
        ProductCommand::Connections(command) => {
            let owners = app.ready_owners(caller).map_err(|failure| {
                command_failure(
                    ProductCommandDisposition::NotAdmitted,
                    ProductFailure::Connections(failure),
                )
            })?;
            let actor = caller.owner_actor();
            let scope = command_scope();
            app.execute_owner(async move {
                use floe_connections::{ConnectionsCommandFailure as Failure, ObserveDecision};
                let service = &owners.connections;
                let result = match command {
                    ConnectionsCommand::PairingStart { address_text } => service
                        .start_pairing(&actor, command_id.as_uuid(), &address_text, &scope)
                        .await
                        .map(ConnectionsCommandOutcome::Pairing),
                    ConnectionsCommand::PairingCancel {
                        operation_ref,
                        expected_revision,
                    } => service
                        .cancel_pairing(
                            &actor,
                            command_id.as_uuid(),
                            operation_ref,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ConnectionsCommandOutcome::Pairing),
                    ConnectionsCommand::GatewayForget {
                        gateway_ref,
                        expected_revision,
                    } => service
                        .forget_gateway(
                            &actor,
                            command_id.as_uuid(),
                            gateway_ref,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ConnectionsCommandOutcome::Gateway),
                    ConnectionsCommand::IntegrationPrepareReview {
                        integration_ref,
                        expected_revision,
                    } => service
                        .prepare_integration_review(
                            &actor,
                            command_id.as_uuid(),
                            integration_ref,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ConnectionsCommandOutcome::IntegrationReview),
                    ConnectionsCommand::IntegrationStart {
                        integration_ref,
                        review_ref,
                        expected_revision,
                    } => service
                        .start_integration(
                            &actor,
                            command_id.as_uuid(),
                            integration_ref,
                            review_ref,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ConnectionsCommandOutcome::Operation),
                    ConnectionsCommand::OperationCancel {
                        operation_ref,
                        expected_revision,
                    } => service
                        .cancel_operation(
                            &actor,
                            command_id.as_uuid(),
                            operation_ref,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ConnectionsCommandOutcome::Operation),
                    ConnectionsCommand::SourcePrepareReview {
                        source_ref,
                        expected_revision,
                    } => service
                        .prepare_source_review(
                            &actor,
                            command_id.as_uuid(),
                            source_ref,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ConnectionsCommandOutcome::SourceReview),
                    ConnectionsCommand::SourceConfigure {
                        source_ref,
                        review_ref,
                        selected_resource_refs,
                        expected_revision,
                    } => service
                        .configure_source(
                            &actor,
                            command_id.as_uuid(),
                            source_ref,
                            review_ref,
                            selected_resource_refs,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ConnectionsCommandOutcome::SourceConfiguration),
                    ConnectionsCommand::Disconnect {
                        source_ref,
                        expected_revision,
                    } => service
                        .disconnect(
                            &actor,
                            command_id.as_uuid(),
                            source_ref,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ConnectionsCommandOutcome::Operation),
                    ConnectionsCommand::ObservePrepareReview {
                        source_ref,
                        expected_revision,
                        requested_processing,
                    } => service
                        .prepare_observe_review(
                            &actor,
                            command_id.as_uuid(),
                            source_ref,
                            expected_revision,
                            requested_processing,
                            &scope,
                        )
                        .await
                        .map(ConnectionsCommandOutcome::ObserveReview),
                    ConnectionsCommand::ObserveEnable {
                        source_ref,
                        review_ref,
                        expected_revision,
                    } => service
                        .apply_observe(
                            &actor,
                            command_id.as_uuid(),
                            source_ref,
                            expected_revision,
                            review_ref,
                            ObserveDecision::Allow,
                            &scope,
                        )
                        .await
                        .map(ConnectionsCommandOutcome::Source),
                    ConnectionsCommand::ObservePause {
                        source_ref,
                        expected_revision,
                    } => service
                        .pause_observe(
                            &actor,
                            command_id.as_uuid(),
                            source_ref,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ConnectionsCommandOutcome::Source),
                    ConnectionsCommand::GatewayManagementLaunch {
                        gateway_ref,
                        expected_revision,
                    } => service
                        .request_management_launch(
                            &actor,
                            command_id.as_uuid(),
                            gateway_ref,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ConnectionsCommandOutcome::Launch),
                };
                result
                    .map(ProductCommandOutcome::Connections)
                    .map_err(|failure| {
                        let (disposition, reason) = match failure {
                            Failure::NotAdmitted(reason) => {
                                (ProductCommandDisposition::NotAdmitted, reason)
                            }
                            Failure::NotApplied(reason) => {
                                (ProductCommandDisposition::NotApplied, reason)
                            }
                            Failure::Admitted(reason) => {
                                (ProductCommandDisposition::Admitted, reason)
                            }
                            Failure::Indeterminate(reason) => {
                                (ProductCommandDisposition::Indeterminate, reason)
                            }
                        };
                        command_failure(disposition, ProductFailure::Connections(reason))
                    })
            })
        }
        ProductCommand::Day(command) => {
            let actor = caller.owner_actor();
            let scope = command_scope();
            let service = app.core.day.clone();
            app.execute_owner(async move {
                let result = match command {
                    DayCommand::Refresh(day) => service
                        .refresh_day(&actor, command_id.as_uuid(), day, &scope)
                        .await
                        .map(DayCommandOutcome::Refresh),
                    DayCommand::Mutate { day, mutation } => service
                        .mutate(
                            &actor,
                            floe_day::DayMutationRequest {
                                command_id: command_id.as_uuid(),
                                day,
                                mutation,
                            },
                            &scope,
                        )
                        .await
                        .map(DayCommandOutcome::Mutation),
                };
                result.map(ProductCommandOutcome::Day).map_err(|failure| {
                    command_failure(
                        ProductCommandDisposition::Indeterminate,
                        ProductFailure::Day(crate::core::day_error(failure)),
                    )
                })
            })
        }
        ProductCommand::Actions(command) => {
            let owners = app.ready_owners(caller).map_err(|failure| {
                command_failure(
                    ProductCommandDisposition::NotAdmitted,
                    ProductFailure::Actions(failure),
                )
            })?;
            let actor = caller.owner_actor();
            let scope = command_scope();
            app.execute_owner(async move {
                let outcome = match command {
                    ActionsCommand::Submit { intent } => owners
                        .actions
                        .submit(&actor, command_id.as_uuid(), intent, &scope)
                        .await
                        .map(ActionsCommandResult::Action),
                    ActionsCommand::Decide {
                        action_ref,
                        review_ref,
                        decision,
                        expected_revision,
                    } => owners
                        .actions
                        .decide(
                            &actor,
                            command_id.as_uuid(),
                            action_ref,
                            review_ref,
                            decision,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ActionsCommandResult::Action),
                    ActionsCommand::Reconcile {
                        action_ref,
                        expected_revision,
                    } => owners
                        .actions
                        .reconcile(
                            &actor,
                            command_id.as_uuid(),
                            action_ref,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ActionsCommandResult::Action),
                    ActionsCommand::SetAuthority {
                        mode,
                        expected_revision,
                    } => owners
                        .actions
                        .set_calendar_create_authority(
                            &actor,
                            command_id.as_uuid(),
                            mode,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ActionsCommandResult::Authority),
                };
                outcome
                    .map(|value| ProductCommandOutcome::Actions(value))
                    .map_err(|reason| {
                        command_failure(
                            ProductCommandDisposition::Indeterminate,
                            ProductFailure::Actions(reason),
                        )
                    })
            })
        }
        ProductCommand::Experts(command) => {
            let command_id = command_id;
            let owners = app.ready_owners(caller).map_err(|failure| {
                command_failure(
                    ProductCommandDisposition::NotAdmitted,
                    ProductFailure::Experts(failure),
                )
            })?;
            let actor = caller.owner_actor();
            let scope = command_scope();
            app.execute_owner(async move {
                let outcome = match command {
                    ExpertCommand::SetInstallationEnabled {
                        installation_ref,
                        expected_revision,
                        enabled,
                    } => owners
                        .experts
                        .set_installation_enabled(
                            &actor,
                            command_id,
                            installation_ref,
                            expected_revision,
                            enabled,
                            &scope,
                        )
                        .await
                        .map(ExpertCommandResult::Directory),
                    ExpertCommand::PrepareBindingReview {
                        assignment_ref,
                        requirement_ref,
                        expected_binding_revision,
                    } => owners
                        .experts
                        .prepare_binding_review(
                            &actor,
                            command_id,
                            assignment_ref,
                            requirement_ref,
                            expected_binding_revision,
                            &scope,
                        )
                        .await
                        .map(ExpertCommandResult::BindingReview),
                    ExpertCommand::ReplaceBinding {
                        review_ref,
                        expected_binding_revision,
                        candidate_refs,
                    } => owners
                        .experts
                        .replace_binding(
                            &actor,
                            command_id,
                            review_ref,
                            expected_binding_revision,
                            candidate_refs,
                            &scope,
                        )
                        .await
                        .map(ExpertCommandResult::Directory),
                };
                outcome
                    .map(ProductCommandOutcome::Experts)
                    .map_err(|reason| {
                        command_failure(
                            ProductCommandDisposition::Indeterminate,
                            ProductFailure::Experts(reason),
                        )
                    })
            })
        }
        ProductCommand::Memory(command) => {
            let owners = app.ready_owners(caller).map_err(|failure| {
                command_failure(
                    ProductCommandDisposition::NotAdmitted,
                    ProductFailure::Memory(failure),
                )
            })?;
            let actor = caller.owner_actor();
            let scope = command_scope();
            app.execute_owner(async move {
                let outcome = match command {
                    MemoryCommand::Decide {
                        candidate_id,
                        decision,
                    } => {
                        owners
                            .knowledge
                            .decide(&actor, command_id, candidate_id, decision, &scope)
                            .await
                    }
                };
                outcome
                    .map(ProductCommandOutcome::Memory)
                    .map_err(|reason| {
                        command_failure(
                            ProductCommandDisposition::Indeterminate,
                            ProductFailure::Memory(reason),
                        )
                    })
            })
        }
    }
}

fn route_query(
    app: &AppComposition,
    caller: &CallerContext,
    request_id: Uuid,
    query: ProductQuery,
) -> Result<ProductQueryOutcome, ProductFailure> {
    let scope = || crate::host_scope(request_id, Cancellation::new(), PRODUCT_SCOPE);
    match query {
        ProductQuery::Conversation(query) => {
            let owners = app
                .ready_owners(caller)
                .map_err(ProductFailure::Conversation)?;
            let actor = caller.owner_actor();
            let scope = scope();
            app.execute_owner(async move {
                let service = owners.conversation.as_ref();
                let outcome = match query {
                    ConversationQuery::ResumeSession => service
                        .resume_session(&actor, &scope)
                        .await
                        .map(ConversationQueryOutcome::Session),
                    ConversationQuery::GetSession {
                        session_id,
                        before_message_id,
                    } => service
                        .get_session(&actor, session_id, before_message_id, &scope)
                        .await
                        .map(|snapshot| ConversationQueryOutcome::Session(Some(snapshot))),
                    ConversationQuery::GetCommand { command_id } => service
                        .read_command(&actor, command_id, &scope)
                        .await
                        .map(ConversationQueryOutcome::Command),
                    ConversationQuery::GetRun { run_id } => service
                        .read_run(&actor, run_id, &scope)
                        .await
                        .map(ConversationQueryOutcome::Run),
                    ConversationQuery::GetMessage { message_id } => service
                        .read_message(&actor, message_id, &scope)
                        .await
                        .map(ConversationQueryOutcome::Message),
                    ConversationQuery::GetInteraction { interaction_id } => service
                        .read_interaction(&actor, interaction_id, &scope)
                        .await
                        .map(ConversationQueryOutcome::Interaction),
                    ConversationQuery::ListInteractions { session_id } => service
                        .list_interactions(&actor, session_id, &scope)
                        .await
                        .map(ConversationQueryOutcome::Interactions),
                };
                outcome
                    .map(ProductQueryOutcome::Conversation)
                    .map_err(ProductFailure::Conversation)
            })
        }
        ProductQuery::Connections(query) => {
            let owners = app
                .ready_owners(caller)
                .map_err(ProductFailure::Connections)?;
            let actor = caller.owner_actor();
            let scope = scope();
            app.execute_owner(async move {
                let service = &owners.connections;
                let outcome = match query {
                    ConnectionsQuery::Overview => service
                        .overview(&actor, &scope)
                        .await
                        .map(ConnectionsQueryOutcome::Overview),
                    ConnectionsQuery::PairingGet { operation_ref } => service
                        .get_pairing(&actor, operation_ref, &scope)
                        .await
                        .map(ConnectionsQueryOutcome::Pairing),
                    ConnectionsQuery::GatewayGet { gateway_ref } => service
                        .get_gateway(&actor, gateway_ref, &scope)
                        .await
                        .map(ConnectionsQueryOutcome::Gateway),
                    ConnectionsQuery::IntegrationInspectReview { review_ref } => service
                        .inspect_integration_review(&actor, review_ref, &scope)
                        .await
                        .map(ConnectionsQueryOutcome::IntegrationReview),
                    ConnectionsQuery::OperationGet { operation_ref } => service
                        .get_operation(&actor, operation_ref, &scope)
                        .await
                        .map(ConnectionsQueryOutcome::Operation),
                    ConnectionsQuery::SourceInspectReview { review_ref } => service
                        .inspect_source_review(&actor, review_ref, &scope)
                        .await
                        .map(ConnectionsQueryOutcome::SourceReview),
                    ConnectionsQuery::ObserveInspectReview { review_ref } => service
                        .inspect_observe_review(&actor, review_ref, &scope)
                        .await
                        .map(ConnectionsQueryOutcome::ObserveReview),
                };
                outcome
                    .map(ProductQueryOutcome::Connections)
                    .map_err(ProductFailure::Connections)
            })
        }
        ProductQuery::Day(query) => {
            let actor = caller.owner_actor();
            let timeout = match &query {
                DayProductQuery::Snapshot(_) => DAY_SNAPSHOT_SCOPE,
                DayProductQuery::RefreshGet { .. } => PRODUCT_SCOPE,
            };
            let scope = crate::host_scope(request_id, Cancellation::new(), timeout);
            let service = app.core.day.clone();
            app.execute_owner(async move {
                let outcome = match query {
                    DayProductQuery::Snapshot(day) => service
                        .snapshot(&actor, day, &scope)
                        .await
                        .map(DayQueryOutcome::Snapshot),
                    DayProductQuery::RefreshGet { operation_ref } => service
                        .get_refresh(&actor, operation_ref, &scope)
                        .await
                        .map(DayQueryOutcome::Refresh),
                };
                outcome
                    .map(ProductQueryOutcome::Day)
                    .map_err(|failure| ProductFailure::Day(crate::core::day_error(failure)))
            })
        }
        ProductQuery::Actions(query) => {
            let owners = app.ready_owners(caller).map_err(ProductFailure::Actions)?;
            let actor = caller.owner_actor();
            let scope = scope();
            app.execute_owner(async move {
                let outcome = match query {
                    ActionsQuery::Destinations => owners
                        .actions
                        .destinations(&actor, &scope)
                        .await
                        .map(ActionsQueryResult::Destinations),
                    ActionsQuery::ProposalPreview {
                        receipt,
                        artifact_id,
                    } => owners
                        .actions
                        .proposal_preview(&actor, receipt, artifact_id, &scope)
                        .await
                        .map(ActionsQueryResult::ProposalPreview),
                    ActionsQuery::Authority => owners
                        .actions
                        .inspect_authority(&actor, &scope)
                        .await
                        .map(ActionsQueryResult::Authority),
                    ActionsQuery::Inspect { action_ref } => owners
                        .actions
                        .inspect(&actor, action_ref, &scope)
                        .await
                        .map(ActionsQueryResult::Action),
                    ActionsQuery::List { cursor, limit } => owners
                        .actions
                        .list(&actor, cursor, limit, &scope)
                        .await
                        .map(ActionsQueryResult::Page),
                };
                outcome
                    .map(ProductQueryOutcome::Actions)
                    .map_err(ProductFailure::Actions)
            })
        }
        ProductQuery::Experts(query) => {
            let owners = app.ready_owners(caller).map_err(ProductFailure::Experts)?;
            let actor = caller.owner_actor();
            let scope = scope();
            app.execute_owner(async move {
                let outcome = match query {
                    ExpertQuery::Directory => owners
                        .experts
                        .directory(&actor, &scope)
                        .await
                        .map(ExpertQueryResult::Directory),
                    ExpertQuery::InspectBinding {
                        assignment_ref,
                        requirement_ref,
                    } => owners
                        .experts
                        .inspect_binding(&actor, assignment_ref, requirement_ref, &scope)
                        .await
                        .map(ExpertQueryResult::Binding),
                    ExpertQuery::InspectBindingReview { review_ref } => owners
                        .experts
                        .inspect_binding_review(&actor, review_ref, &scope)
                        .await
                        .map(ExpertQueryResult::BindingReview),
                };
                outcome
                    .map(ProductQueryOutcome::Experts)
                    .map_err(ProductFailure::Experts)
            })
        }
        ProductQuery::Memory(query) => {
            let owners = app.ready_owners(caller).map_err(ProductFailure::Memory)?;
            let actor = caller.owner_actor();
            let scope = scope();
            app.execute_owner(async move {
                let outcome = match query {
                    MemoryQuery::Overview { limit } => owners
                        .knowledge
                        .overview(&actor, limit, &scope)
                        .await
                        .map(MemoryQueryResult::Overview),
                    MemoryQuery::Review => owners
                        .knowledge
                        .review(&actor, &scope)
                        .await
                        .map(MemoryQueryResult::Review),
                };
                outcome
                    .map(ProductQueryOutcome::Memory)
                    .map_err(ProductFailure::Memory)
            })
        }
    }
}

fn route_observation(
    app: &AppComposition,
    caller: &CallerContext,
    request_id: Uuid,
    observation: ProductObservation,
) -> Result<ProductObservationOutcome, ProductFailure> {
    let owners = app
        .ready_owners(caller)
        .map_err(ProductFailure::Conversation)?;
    let actor = caller.owner_actor();
    let scope = crate::host_scope(request_id, Cancellation::new(), OBSERVATION_SCOPE);
    app.execute_owner(async move {
        owners
            .conversation
            .read_events(
                &actor,
                floe_conversation::ReadConversationEvents {
                    runtime_epoch: observation.runtime_epoch,
                    cursor: observation.cursor,
                    limit: observation.limit,
                },
                &scope,
            )
            .await
            .map(ProductObservationOutcome::Conversation)
            .map_err(ProductFailure::Conversation)
    })
}

fn command_failure(
    disposition: ProductCommandDisposition,
    failure: ProductFailure,
) -> ProductCommandFailure {
    ProductCommandFailure {
        disposition,
        failure,
    }
}

#[cfg(test)]
mod tests {
    use super::{DAY_SNAPSHOT_SCOPE, PRODUCT_SCOPE};

    #[test]
    fn day_snapshot_owner_deadline_precedes_the_flutter_transport_deadline() {
        assert_eq!(DAY_SNAPSHOT_SCOPE.as_secs(), 30);
        assert_eq!(PRODUCT_SCOPE.as_secs(), 35);
        assert!(DAY_SNAPSHOT_SCOPE < PRODUCT_SCOPE);
    }
}
