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
        ProductCommand::VoiceStart { surface_session_id } => {
            let owners = app.ready_owners(caller).map_err(|failure| {
                command_failure(
                    ProductCommandDisposition::NotAdmitted,
                    ProductFailure::Voice(failure),
                )
            })?;
            let actor = caller.owner_actor();
            let scope = command_scope();
            let start_scope = crate::live_voice::live_scope(
                command_id.as_uuid(),
                Cancellation::new(),
                Duration::from_secs(70),
            );
            let registry = app.core.live_media_registry.clone();
            let voice_command = floe_voice::VoiceStartCommand {
                command_id: command_id.as_uuid(),
                surface_session_id,
            };
            app.execute_owner(async move {
                if let Some(status) = read_voice_start_status(
                    owners.voice.as_ref(),
                    &actor,
                    command_id.as_uuid(),
                    &scope,
                )
                .await
                .map_err(|failure| {
                    command_failure(
                        ProductCommandDisposition::Indeterminate,
                        ProductFailure::Voice(failure),
                    )
                })? {
                    if status.surface_session_id != surface_session_id {
                        return Err(command_failure(
                            ProductCommandDisposition::Indeterminate,
                            ProductFailure::Voice(floe_kernel::AgentFailure::Conflict),
                        ));
                    }
                    return Ok(ProductCommandOutcome::VoiceStart(status));
                }

                let (sender, receiver) = tokio::sync::oneshot::channel();
                let bound = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
                let task_owners = owners.clone();
                let driver_owners = owners.clone();
                let task_registry = registry.clone();
                let task_actor = actor.clone();
                let driver_actor = actor.clone();
                let task_scope = start_scope.clone();
                let driver_scope = start_scope.clone();
                let signal = bound.clone();
                tokio::spawn(async move {
                    let result = task_owners
                        .voice
                        .run_start_controller(
                            &task_actor,
                            voice_command,
                            &task_scope,
                            move |preparation| {
                                let bound = bound.clone();
                                Box::pin(async move {
                                    let result = crate::live_voice::drive_start(
                                        preparation,
                                        driver_owners,
                                        task_registry,
                                        driver_actor,
                                        surface_session_id,
                                        command_id.as_uuid(),
                                        driver_scope,
                                        bound.clone(),
                                    )
                                    .await;
                                    if let Err(failure) = &result {
                                        crate::live_voice::signal_start_failure(
                                            &bound,
                                            failure.clone(),
                                        );
                                    }
                                    result
                                })
                            },
                        )
                        .await;
                    if let Err(failure) = result {
                        crate::live_voice::signal_start_failure(&signal, failure.into_failure());
                    }
                });

                // Return once the exact muted lease is bound. The Voice owner
                // retains Gateway events and Stop handling in its own task.
                let observed = tokio::time::timeout(Duration::from_secs(8), receiver).await;
                if let Ok(Ok(Err(failure))) = observed {
                    if let Some(status) = read_voice_start_status(
                        owners.voice.as_ref(),
                        &actor,
                        command_id.as_uuid(),
                        &scope,
                    )
                    .await
                    .map_err(|read| {
                        command_failure(
                            ProductCommandDisposition::Indeterminate,
                            ProductFailure::Voice(read),
                        )
                    })? {
                        return Ok(ProductCommandOutcome::VoiceStart(status));
                    }
                    return Err(command_failure(
                        ProductCommandDisposition::Admitted,
                        ProductFailure::Voice(failure),
                    ));
                }
                read_voice_start_status(owners.voice.as_ref(), &actor, command_id.as_uuid(), &scope)
                    .await
                    .map_err(|failure| {
                        command_failure(
                            ProductCommandDisposition::Indeterminate,
                            ProductFailure::Voice(failure),
                        )
                    })?
                    .map(ProductCommandOutcome::VoiceStart)
                    .ok_or_else(|| {
                        command_failure(
                            ProductCommandDisposition::Indeterminate,
                            ProductFailure::Voice(floe_kernel::AgentFailure::StorageUnavailable),
                        )
                    })
            })
        }
        ProductCommand::VoiceStartCancel { surface_session_id } => {
            let owners = app.ready_owners(caller).map_err(|failure| {
                command_failure(
                    ProductCommandDisposition::NotAdmitted,
                    ProductFailure::Voice(failure),
                )
            })?;
            let actor = caller.owner_actor();
            let scope = command_scope();
            let voice_command = floe_voice::VoiceStartCommand {
                command_id: command_id.as_uuid(),
                surface_session_id,
            };
            app.execute_owner(async move {
                owners
                    .voice
                    .cancel_start(&actor, voice_command, &scope)
                    .await
                    .map_err(|failure| owner_command_failure(failure, ProductFailure::Voice))?;
                read_voice_start_status(owners.voice.as_ref(), &actor, command_id.as_uuid(), &scope)
                    .await
                    .and_then(|status| status.ok_or(floe_kernel::AgentFailure::StorageUnavailable))
                    .map(ProductCommandOutcome::VoiceStartCancelled)
                    .map_err(|failure| {
                        command_failure(
                            ProductCommandDisposition::Indeterminate,
                            ProductFailure::Voice(failure),
                        )
                    })
            })
        }
        ProductCommand::VoiceStop {
            voice_session_id,
            surface_session_id,
        } => {
            let owners = app.ready_owners(caller).map_err(|failure| {
                command_failure(
                    ProductCommandDisposition::NotAdmitted,
                    ProductFailure::Voice(failure),
                )
            })?;
            let actor = caller.owner_actor();
            let scope = command_scope();
            let registry = app.core.live_media_registry.clone();
            app.execute_owner(async move {
                crate::live_voice::stop_bound_call(
                    owners.as_ref(),
                    registry,
                    &actor,
                    voice_session_id,
                    surface_session_id,
                    &scope,
                )
                .await
                .map_err(|failure| {
                    command_failure(
                        ProductCommandDisposition::Indeterminate,
                        ProductFailure::Voice(failure),
                    )
                })?;
                read_voice_start_status(owners.voice.as_ref(), &actor, voice_session_id, &scope)
                    .await
                    .and_then(|status| status.ok_or(floe_kernel::AgentFailure::StorageUnavailable))
                    .map(ProductCommandOutcome::VoiceStopped)
                    .map_err(|failure| {
                        command_failure(
                            ProductCommandDisposition::Indeterminate,
                            ProductFailure::Voice(failure),
                        )
                    })
            })
        }
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
                    ConversationCommand::AssistantFeature(command) => {
                        let outcome = match command {
                            AssistantFeatureCommand::PrepareSourceReview {
                                feature_ref,
                                source_scope_ref,
                                requirement_ref,
                                expected_binding_revision,
                            } => owners
                                .experts
                                .prepare_assistant_feature_source_review(
                                    &actor,
                                    command_id,
                                    feature_ref,
                                    source_scope_ref,
                                    requirement_ref,
                                    expected_binding_revision,
                                    &scope,
                                )
                                .await
                                .map(AssistantFeatureCommandResult::SourceReview),
                            AssistantFeatureCommand::Configure {
                                feature_ref,
                                expected_revision,
                                enabled,
                                source_selections,
                            } => owners
                                .experts
                                .configure_assistant_feature(
                                    &actor,
                                    command_id,
                                    feature_ref,
                                    expected_revision,
                                    enabled,
                                    source_selections,
                                    &scope,
                                )
                                .await
                                .map(AssistantFeatureCommandResult::Snapshot),
                        };
                        outcome
                            .map(ConversationCommandOutcome::AssistantFeature)
                            .map_err(|failure| {
                                owner_command_failure(failure, ProductFailure::Conversation)
                            })?
                    }
                    ConversationCommand::SetCalendarOperationPolicy {
                        mode,
                        expected_revision,
                    } => service
                        .set_calendar_operation_policy(
                            &actor,
                            command_id,
                            mode,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(ConversationCommandOutcome::CalendarOperationPolicy)
                        .map_err(|failure| {
                            owner_command_failure(failure, ProductFailure::Conversation)
                        })?,
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
                    ConversationCommand::SubmitTextWork { expected, text } => service
                        .submit_text_work(
                            &actor,
                            floe_conversation::SubmitTextWork {
                                command_id,
                                expected,
                                text,
                                retry_of: None,
                            },
                            &scope,
                        )
                        .await
                        .map(ConversationCommandOutcome::Work)
                        .map_err(|failure| {
                            owner_command_failure(failure, ProductFailure::Conversation)
                        })?,
                    ConversationCommand::RetryWork {
                        expected,
                        text,
                        source_run_id,
                    } => service
                        .submit_text_work(
                            &actor,
                            floe_conversation::SubmitTextWork {
                                command_id,
                                expected,
                                text,
                                retry_of: Some(source_run_id),
                            },
                            &scope,
                        )
                        .await
                        .map(ConversationCommandOutcome::Work)
                        .map_err(|failure| {
                            owner_command_failure(failure, ProductFailure::Conversation)
                        })?,
                    ConversationCommand::CancelPendingWork { work_command_id } => service
                        .cancel_pending_work(
                            &actor,
                            floe_conversation::CancelPendingWork {
                                command_id,
                                work_command_id,
                            },
                            &scope,
                        )
                        .await
                        .map(ConversationCommandOutcome::PendingWorkCancelled)
                        .map_err(|failure| {
                            owner_command_failure(failure, ProductFailure::Conversation)
                        })?,
                    ConversationCommand::ContinueTurn {
                        session_id,
                        expected_revision,
                        continuation_id,
                    } => service
                        .continue_turn(
                            &actor,
                            floe_conversation::ContinueTurn {
                                command_id,
                                session_id,
                                expected_revision,
                                continuation_ref: floe_conversation::ContinuationToken {
                                    id: continuation_id,
                                },
                            },
                            &scope,
                        )
                        .await
                        .map(ConversationCommandOutcome::Turn)
                        .map_err(|failure| {
                            owner_command_failure(failure, ProductFailure::Conversation)
                        })?,
                    ConversationCommand::SubmitCalendarProposal {
                        session_id,
                        origin_run_id,
                        receipt,
                        artifact_id,
                        destination_ref,
                    } => {
                        let result = service
                            .submit_calendar_proposal(
                                &actor,
                                command_id,
                                floe_conversation::CalendarProposalRequest {
                                    session_id,
                                    origin_run_id,
                                    receipt,
                                    artifact_id,
                                    destination_ref,
                                },
                                &scope,
                            )
                            .await
                            .map_err(|failure| {
                                owner_command_failure(failure, ProductFailure::Conversation)
                            })?;
                        ConversationCommandOutcome::CalendarProposal(result)
                    }
                    ConversationCommand::CancelRun { run_id } => {
                        let receipt = service
                            .cancel_run(&actor, command_id, run_id, &scope)
                            .await
                            .map_err(|failure| {
                                owner_command_failure(failure, ProductFailure::Conversation)
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
                            .map_err(|failure| {
                                owner_command_failure(failure, ProductFailure::Conversation)
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
                            .map_err(|failure| {
                                owner_command_failure(failure, ProductFailure::Conversation)
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
                    DayCommand::ExternalCalendarOperation { operation } => service
                        .operate_external_calendar(&actor, command_id.as_uuid(), operation, &scope)
                        .await
                        .map(DayCommandOutcome::ExternalCalendarOperation),
                    DayCommand::ReconcileExternalCalendarOperation {
                        operation_ref,
                        expected_revision,
                    } => service
                        .reconcile_external_calendar_operation(
                            &actor,
                            command_id.as_uuid(),
                            operation_ref,
                            expected_revision,
                            &scope,
                        )
                        .await
                        .map(DayCommandOutcome::ReconciledExternalCalendarOperation),
                };
                result.map(ProductCommandOutcome::Day).map_err(|failure| {
                    owner_command_failure(failure, |failure| {
                        ProductFailure::Day(crate::core::day_error(failure))
                    })
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
                    } => owners
                        .knowledge
                        .decide(&actor, command_id, candidate_id, decision, &scope)
                        .await
                        .map(MemoryCommandOutcome::Decided),
                    MemoryCommand::Forget {
                        target_id,
                        expected_revision,
                    } => owners
                        .knowledge
                        .forget(
                            &actor,
                            floe_knowledge::MemoryForgetRequest {
                                origin: floe_knowledge::MemoryForgetOrigin::User,
                                command_id,
                                target_id,
                                expected_revision,
                            },
                            &scope,
                        )
                        .await
                        .map(MemoryCommandOutcome::Forgotten),
                };
                outcome
                    .map(ProductCommandOutcome::Memory)
                    .map_err(|failure| owner_command_failure(failure, ProductFailure::Memory))
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
        ProductQuery::VoiceStartStatus { command_id } => {
            let owners = app.ready_owners(caller).map_err(ProductFailure::Voice)?;
            let actor = caller.owner_actor();
            let scope = scope();
            app.execute_owner(async move {
                read_voice_start_status(owners.voice.as_ref(), &actor, command_id, &scope)
                    .await
                    .map(ProductQueryOutcome::VoiceStartStatus)
                    .map_err(ProductFailure::Voice)
            })
        }
        ProductQuery::VoiceStartRecoverable => {
            let owners = app.ready_owners(caller).map_err(ProductFailure::Voice)?;
            let actor = caller.owner_actor();
            let scope = scope();
            app.execute_owner(async move {
                let status = match owners
                    .voice
                    .read_recoverable_start(&actor, &scope)
                    .await
                    .map_err(ProductFailure::Voice)?
                {
                    Some(start) => read_voice_start_status(
                        owners.voice.as_ref(),
                        &actor,
                        start.command.command_id,
                        &scope,
                    )
                    .await
                    .map_err(ProductFailure::Voice)?,
                    None => None,
                };
                // Candidate discovery and status projection are separate
                // read-only journal operations. If a concurrent Cancel or
                // Stop settles the command between them, return no recovery
                // candidate rather than surfacing a terminal record as active.
                let status = status.filter(|status| match status.state {
                    VoiceStartState::Preparing => true,
                    VoiceStartState::Bound => status.local_lease_released == Some(false),
                    VoiceStartState::Cancelled => false,
                });
                Ok(ProductQueryOutcome::VoiceStartRecoverable(status))
            })
        }
        ProductQuery::Conversation(query) => {
            let owners = app
                .ready_owners(caller)
                .map_err(ProductFailure::Conversation)?;
            let actor = caller.owner_actor();
            let scope = scope();
            app.execute_owner(async move {
                let service = owners.conversation.as_ref();
                let outcome = match query {
                    ConversationQuery::AssistantFeature(query) => {
                        let outcome = match query {
                            AssistantFeatureQuery::Snapshot => owners
                                .experts
                                .assistant_features(&actor, &scope)
                                .await
                                .map(AssistantFeatureQueryResult::Snapshot),
                            AssistantFeatureQuery::InspectSourceReview { review_ref } => owners
                                .experts
                                .inspect_binding_review(&actor, review_ref, &scope)
                                .await
                                .map(AssistantFeatureQueryResult::SourceReview),
                        };
                        outcome.map(ConversationQueryOutcome::AssistantFeature)
                    }
                    ConversationQuery::CalendarProposalPreview {
                        receipt,
                        artifact_id,
                    } => service
                        .preview_calendar_proposal(&actor, receipt, artifact_id, &scope)
                        .await
                        .map(ConversationQueryOutcome::CalendarProposalPreview),
                    ConversationQuery::CalendarOperationPolicy => service
                        .calendar_operation_policy(&actor, &scope)
                        .await
                        .map(ConversationQueryOutcome::CalendarOperationPolicy),
                    ConversationQuery::ResumeSession => service
                        .resume_session(&actor, &scope)
                        .await
                        .map(ConversationQueryOutcome::Session),
                    ConversationQuery::Surface(request) => service
                        .read_surface(&actor, request, &scope)
                        .await
                        .map(ConversationQueryOutcome::Surface),
                    ConversationQuery::Work { command_id } => service
                        .read_surface_work(&actor, command_id, &scope)
                        .await
                        .map(ConversationQueryOutcome::Work),
                    ConversationQuery::WorkPage(request) => service
                        .list_surface_work(&actor, request, &scope)
                        .await
                        .map(ConversationQueryOutcome::WorkPage),
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
                    ConversationQuery::ListInteractions { session_id, after } => service
                        .list_interactions(&actor, session_id, after, &scope)
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
                DayProductQuery::ExternalCalendarDestinations
                | DayProductQuery::ExternalCalendarOperationGet { .. }
                | DayProductQuery::ExternalCalendarOperations { .. } => PRODUCT_SCOPE,
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
                    DayProductQuery::ExternalCalendarDestinations => service
                        .external_calendar_destinations(&actor, &scope)
                        .await
                        .map(DayQueryOutcome::ExternalCalendarDestinations),
                    DayProductQuery::ExternalCalendarOperationGet { operation_ref } => service
                        .inspect_external_calendar_operation(&actor, operation_ref, &scope)
                        .await
                        .map(DayQueryOutcome::ExternalCalendarOperation),
                    DayProductQuery::ExternalCalendarOperations { cursor, limit } => service
                        .list_external_calendar_operations(&actor, cursor, limit, &scope)
                        .await
                        .map(DayQueryOutcome::ExternalCalendarOperations),
                };
                outcome
                    .map(ProductQueryOutcome::Day)
                    .map_err(|failure| ProductFailure::Day(crate::core::day_error(failure)))
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

async fn read_voice_start_status(
    voice: &floe_voice::VoiceService,
    actor: &floe_kernel::OwnerActor,
    command_id: Uuid,
    scope: &floe_execution::ExecutionScope,
) -> Result<Option<VoiceStartStatus>, floe_kernel::AgentFailure> {
    let Some(start) = voice.read_start(actor, command_id, scope).await? else {
        return Ok(None);
    };
    let call = if start.state == floe_voice::VoiceStartCommandState::Bound {
        Some(
            voice
                .read_call(actor, command_id, scope)
                .await?
                .ok_or(floe_kernel::AgentFailure::StorageUnavailable)?,
        )
    } else {
        None
    };
    Ok(Some(VoiceStartStatus {
        command_id,
        surface_session_id: start.command.surface_session_id,
        state: match start.state {
            floe_voice::VoiceStartCommandState::Preparing => VoiceStartState::Preparing,
            floe_voice::VoiceStartCommandState::Bound => VoiceStartState::Bound,
            floe_voice::VoiceStartCommandState::Cancelled => VoiceStartState::Cancelled,
        },
        media: call.as_ref().map(|record| match record.session.media() {
            floe_voice::MediaState::Preparing => VoiceMediaState::Preparing,
            floe_voice::MediaState::Muted => VoiceMediaState::Muted,
            floe_voice::MediaState::Capturing => VoiceMediaState::Capturing,
            floe_voice::MediaState::Stopped => VoiceMediaState::Stopped,
        }),
        remote: call.as_ref().map(|record| match record.session.remote() {
            floe_voice::RemoteState::NotStarted => VoiceRemoteState::NotStarted,
            floe_voice::RemoteState::Dispatching => VoiceRemoteState::Dispatching,
            floe_voice::RemoteState::Connected => VoiceRemoteState::Connected,
            floe_voice::RemoteState::CloseRequested => VoiceRemoteState::CloseRequested,
            floe_voice::RemoteState::Closed => VoiceRemoteState::Closed,
            floe_voice::RemoteState::Unknown => VoiceRemoteState::Unknown,
        }),
        stop_requested: call.as_ref().map(|record| record.session.stop_requested()),
        local_lease_released: call
            .as_ref()
            .map(|record| record.session.local_lease_released()),
    }))
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

fn owner_command_failure<E>(
    failure: floe_kernel::CommandFailure<E>,
    map: impl FnOnce(E) -> ProductFailure,
) -> ProductCommandFailure {
    let (disposition, reason) = match failure {
        floe_kernel::CommandFailure::NotAdmitted(reason) => {
            (ProductCommandDisposition::NotAdmitted, reason)
        }
        floe_kernel::CommandFailure::NotApplied(reason) => {
            (ProductCommandDisposition::NotApplied, reason)
        }
        floe_kernel::CommandFailure::Admitted(reason) => {
            (ProductCommandDisposition::Admitted, reason)
        }
        floe_kernel::CommandFailure::Indeterminate(reason) => {
            (ProductCommandDisposition::Indeterminate, reason)
        }
    };
    command_failure(disposition, map(reason))
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
