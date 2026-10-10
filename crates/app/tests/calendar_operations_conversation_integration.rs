mod support;

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
mod qa {
    use std::time::{Duration, Instant};

    use floe_app::{
        AppComposition, AppHost, ConversationCommand, ConversationCommandOutcome,
        ConversationQuery, ConversationQueryOutcome, DayCommand, DayCommandOutcome,
        DayProductQuery, DayQueryOutcome, OperationPolicyMode, ProductCommand,
        ProductCommandDisposition, ProductCommandOutcome, ProductCommandRequest, ProductFailure,
        ProductQuery, ProductQueryOutcome, RuntimeReadinessState,
    };
    use floe_kernel::AgentFailure;
    use uuid::Uuid;

    use crate::support;
    use crate::support::{
        CalendarExecutorScript, IsolatedProfile, ModelOutput, PrimaryBehavior,
        ScriptModelSelection, ScriptedCalendarExecutor, ScriptedModel, prepare_runtime,
    };

    const APPROVAL_REQUEST: &str = "/focus";

    struct Scenario {
        _profile: IsolatedProfile,
        host: AppHost<AppComposition>,
        executor: ScriptedCalendarExecutor,
        source: floe_connections::SourceSummary,
        session_id: Uuid,
        proposal: floe_conversation::CalendarProposalResult,
    }

    fn set_policy(host: &AppHost<AppComposition>, mode: OperationPolicyMode) {
        let result = host
            .request(Uuid::new_v4())
            .expect("admit Conversation policy query")
            .product_query(ProductQuery::Conversation(
                ConversationQuery::CalendarOperationPolicy,
            ))
            .expect("read Access policy through Conversation");
        let ProductQueryOutcome::Conversation(ConversationQueryOutcome::CalendarOperationPolicy(
            policy,
        )) = result
        else {
            panic!("Conversation returned a different policy query result")
        };
        if policy.calendar_create == mode {
            return;
        }
        let result = host
            .request(Uuid::new_v4())
            .expect("admit Conversation policy command")
            .product_command(ProductCommandRequest {
                command_id: floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap(),
                command: ProductCommand::Conversation(
                    ConversationCommand::SetCalendarOperationPolicy {
                        mode,
                        expected_revision: policy.revision,
                    },
                ),
            })
            .expect("update Access operation policy through Conversation");
        let ProductCommandOutcome::Conversation(
            ConversationCommandOutcome::CalendarOperationPolicy(updated),
        ) = result
        else {
            panic!("Conversation returned a different policy command result")
        };
        assert_eq!(updated.calendar_create, mode);
    }

    fn prepare_scenario(mode: OperationPolicyMode, script: CalendarExecutorScript) -> Scenario {
        prepare_scenario_with_scripts(mode, [script])
    }

    fn prepare_scenario_with_scripts(
        mode: OperationPolicyMode,
        scripts: impl IntoIterator<Item = CalendarExecutorScript>,
    ) -> Scenario {
        let model = ScriptedModel::new(
            PrimaryBehavior::NoGateway,
            APPROVAL_REQUEST,
            ModelOutput::ScheduleOperationApprovalFlow,
        );
        model.recorder().set_model_selection_sequence(vec![
            ScriptModelSelection::device(1),
            ScriptModelSelection::device(2),
            ScriptModelSelection::device(2),
            ScriptModelSelection::device(1),
            ScriptModelSelection::device(1),
        ]);
        let executor = ScriptedCalendarExecutor::new(scripts);
        let profile = IsolatedProfile::new();
        let host = profile.open_with_qa_calendar_executor(&model, executor.clone());
        assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
        let source = support::configure_fixture_calendar(&host, "Synthetic team calendar");
        support::bind_schedule_expert(&host);
        set_policy(&host, mode);

        let session_id = support::start_session(&host);
        let turn = support::start_turn(&host, session_id, APPROVAL_REQUEST);
        let run = support::wait_terminal_run(&host, turn.run_id);
        assert_eq!(run.state, floe_conversation::RunState::Completed);

        let session = support::read_session(&host, session_id);
        let task = session
            .messages
            .iter()
            .find_map(|message| match message {
                floe_conversation::SessionMessage::Delegation { turn_id, task, .. }
                    if *turn_id == turn.run_id.as_uuid() =>
                {
                    Some(task)
                }
                _ => None,
            })
            .expect("the Conversation journal retains its admitted Schedule Task");
        assert_eq!(task.state, floe_agent_contract::TaskState::Completed);
        let receipt = task
            .execution_receipt
            .clone()
            .expect("Task admission receipt remains linked in Conversation");
        let artifact_id = task
            .artifacts
            .iter()
            .find(|artifact| {
                artifact.media_types.iter().any(|media_type| {
                    media_type == floe_calendar_operations::EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE
                })
            })
            .expect("the completed Task retains its exact proposal artifact")
            .artifact_id;
        let destinations = host
            .request(Uuid::new_v4())
            .expect("admit Day destination query")
            .product_query(ProductQuery::Day(
                DayProductQuery::ExternalCalendarDestinations,
            ))
            .expect("read manual Calendar destinations through Day");
        let ProductQueryOutcome::Day(DayQueryOutcome::ExternalCalendarDestinations(destinations)) =
            destinations
        else {
            panic!("Day returned a different Calendar destination result")
        };
        let [destination] = destinations.as_slice() else {
            panic!("expected one configured writable fixture destination: {destinations:?}")
        };

        let command_id = Uuid::new_v4();
        let command = ProductCommand::Conversation(ConversationCommand::SubmitCalendarProposal {
            session_id,
            origin_run_id: turn.run_id,
            receipt,
            artifact_id,
            destination_ref: destination.destination_ref,
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        let proposal = loop {
            let attempt = host
                .request(Uuid::new_v4())
                .expect("admit Conversation proposal command")
                .product_command(ProductCommandRequest {
                    command_id: floe_kernel::CommandId::from_uuid(command_id).unwrap(),
                    command: command.clone(),
                });
            match attempt {
                Err(failure)
                    if matches!(
                        &failure.failure,
                        ProductFailure::Conversation(AgentFailure::StorageBusy)
                    ) && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(25));
                }
                result => {
                    let result = result.expect("Conversation admits exact operation and review");
                    let ProductCommandOutcome::Conversation(
                        ConversationCommandOutcome::CalendarProposal(proposal),
                    ) = result
                    else {
                        panic!("Conversation returned a different proposal result")
                    };
                    break proposal;
                }
            }
        };
        Scenario {
            _profile: profile,
            host,
            executor,
            source,
            session_id,
            proposal,
        }
    }

    fn decide(
        host: &AppHost<AppComposition>,
        session_id: Uuid,
        interaction: &floe_conversation::InteractionSnapshot,
        kind: floe_conversation::InteractionDecisionKind,
        target_digest: [u8; 32],
    ) -> Result<ProductCommandOutcome, floe_app::ProductCommandFailure> {
        decide_with_command_id(
            host,
            session_id,
            interaction,
            kind,
            target_digest,
            Uuid::new_v4(),
        )
    }

    fn decide_with_command_id(
        host: &AppHost<AppComposition>,
        session_id: Uuid,
        interaction: &floe_conversation::InteractionSnapshot,
        kind: floe_conversation::InteractionDecisionKind,
        target_digest: [u8; 32],
        command_id: Uuid,
    ) -> Result<ProductCommandOutcome, floe_app::ProductCommandFailure> {
        let command = ProductCommand::Conversation(ConversationCommand::ResolveInteraction {
            interaction_id: interaction.interaction_id,
            session_id,
            expected_revision: interaction.revision,
            decision: kind,
            target_digest,
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let attempt = host
                .request(Uuid::new_v4())
                .expect("admit Conversation interaction decision")
                .product_command(ProductCommandRequest {
                    command_id: floe_kernel::CommandId::from_uuid(command_id).unwrap(),
                    command: command.clone(),
                });
            match attempt {
                Err(failure)
                    if matches!(
                        &failure.failure,
                        ProductFailure::Conversation(AgentFailure::StorageBusy)
                    ) && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(25));
                }
                result => return result,
            }
        }
    }

    fn inspect(
        host: &AppHost<AppComposition>,
        operation_id: Uuid,
    ) -> Result<floe_calendar_operations::ActionSnapshot, AgentFailure> {
        support::with_ready(host, |services, caller, owners| {
            let actor = caller.owner_actor();
            services.execute_owner(async move {
                owners
                    .calendar_operations
                    .inspect(
                        &actor,
                        operation_id,
                        &floe_app::host_scope(
                            Uuid::new_v4(),
                            floe_execution::Cancellation::new(),
                            Duration::from_secs(10),
                        ),
                    )
                    .await
            })
        })
    }

    fn wait_for_status(
        host: &AppHost<AppComposition>,
        operation_id: Uuid,
        accepted: impl Fn(&floe_calendar_operations::ActionStatus) -> bool,
    ) -> floe_calendar_operations::ActionSnapshot {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let last = match inspect(host, operation_id) {
                Ok(snapshot) if accepted(&snapshot.status) => return snapshot,
                Ok(snapshot) => format!("{:?}", snapshot.status),
                Err(AgentFailure::StorageBusy) => "storage busy".to_owned(),
                Err(failure) => panic!("inspect Calendar operation: {failure:?}"),
            };
            assert!(
                Instant::now() < deadline,
                "Calendar operation did not settle; last status: {last}"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn wait_for_linked_resume(
        host: &AppHost<AppComposition>,
        origin_run_id: floe_kernel::RunId,
    ) -> floe_conversation::RunReceipt {
        let command_id = floe_conversation::resume_command_id(origin_run_id)
            .expect("valid Conversation origin run");
        let deadline = Instant::now() + Duration::from_secs(55);
        let linked = loop {
            let receipt = support::with_ready(host, |services, caller, owners| {
                let actor = caller.owner_actor();
                let scope = floe_app::host_scope(
                    Uuid::new_v4(),
                    floe_execution::Cancellation::new(),
                    Duration::from_secs(15),
                );
                services.execute_owner(async move {
                    owners
                        .conversation
                        .read_command(&actor, command_id, &scope)
                        .await
                })
            });
            match receipt {
                Ok(Some(receipt)) => break receipt,
                Ok(None) | Err(AgentFailure::StorageBusy) => {}
                Err(failure) => panic!("read linked Conversation resume: {failure:?}"),
            }
            assert!(
                Instant::now() < deadline,
                "linked Conversation resume was not admitted"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        assert_eq!(linked.resume_of, Some(origin_run_id));

        loop {
            let receipt = support::with_ready(host, |services, caller, owners| {
                let actor = caller.owner_actor();
                let scope = floe_app::host_scope(
                    Uuid::new_v4(),
                    floe_execution::Cancellation::new(),
                    Duration::from_secs(15),
                );
                services.execute_owner(async move {
                    owners
                        .conversation
                        .read_run(&actor, linked.run_id, &scope)
                        .await
                })
            });
            match receipt {
                Ok(Some(receipt)) if receipt.state.is_terminal() => return receipt,
                Ok(Some(_)) | Ok(None) | Err(AgentFailure::StorageBusy) => {}
                Err(failure) => panic!("read linked Conversation run: {failure:?}"),
            }
            assert!(
                Instant::now() < deadline,
                "linked Conversation run did not settle"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    fn conversation_allow_and_deny_use_the_real_vault_operation_owner() {
        let allowed = prepare_scenario(OperationPolicyMode::Allow, CalendarExecutorScript::Commit);
        assert!(allowed.proposal.interaction.is_none());
        let allowed_dispatch = allowed
            .executor
            .wait_for_snapshot(Duration::from_secs(20), |snapshot| {
                snapshot.dispatch_attempts == 1
            });
        assert!(
            allowed_dispatch.is_some(),
            "Allow did not reach its scripted dispatch seam: {:?}",
            allowed.executor.snapshot()
        );
        let allowed_operation = wait_for_status(
            &allowed.host,
            allowed.proposal.operation.action_ref,
            |status| {
                matches!(
                    status,
                    floe_calendar_operations::ActionStatus::Succeeded { .. }
                        | floe_calendar_operations::ActionStatus::Blocked { .. }
                        | floe_calendar_operations::ActionStatus::Unknown { .. }
                )
            },
        );
        assert!(
            matches!(
                allowed_operation.status,
                floe_calendar_operations::ActionStatus::Succeeded { .. }
            ),
            "Allow operation did not succeed: status={:?}; executor={:?}",
            allowed_operation.status,
            allowed.executor.snapshot()
        );
        assert_eq!(allowed.executor.snapshot().dispatch_attempts, 1);

        let denied = prepare_scenario(OperationPolicyMode::Deny, CalendarExecutorScript::Commit);
        assert!(denied.proposal.interaction.is_none());
        assert!(matches!(
            denied.proposal.operation.status,
            floe_calendar_operations::ActionStatus::Blocked {
                reason: floe_calendar_operations::ActionBlockedReason::PolicyDenied
            }
        ));
        assert_eq!(denied.executor.snapshot().dispatch_attempts, 0);
    }

    #[test]
    fn ask_approval_records_pre_dispatch_failure_and_post_dispatch_ack_loss() {
        let before_dispatch_scenario = prepare_scenario(
            OperationPolicyMode::Ask,
            CalendarExecutorScript::PreDispatchFailure,
        );
        let interaction = before_dispatch_scenario
            .proposal
            .interaction
            .as_ref()
            .expect("Ask publishes the typed Conversation review");
        let decision_command_id = Uuid::new_v4();
        // Model a lost product ACK after the Access decision committed while
        // the executor still fails before any external dispatch.
        let _lost_response = decide_with_command_id(
            &before_dispatch_scenario.host,
            before_dispatch_scenario.session_id,
            interaction,
            floe_conversation::InteractionDecisionKind::Approve,
            interaction.target_digest,
            decision_command_id,
        )
        .expect("Conversation resolves the review through Access");
        let replayed_response = decide_with_command_id(
            &before_dispatch_scenario.host,
            before_dispatch_scenario.session_id,
            interaction,
            floe_conversation::InteractionDecisionKind::Approve,
            interaction.target_digest,
            decision_command_id,
        )
        .expect("the exact decision command replays after its response was lost");
        assert!(matches!(
            replayed_response,
            ProductCommandOutcome::Conversation(ConversationCommandOutcome::Interaction(_))
        ));
        let predispatch_barrier = before_dispatch_scenario
            .executor
            .wait_for_snapshot(Duration::from_secs(10), |snapshot| {
                snapshot.pre_dispatch_failures == 1
            });
        assert!(
            predispatch_barrier.is_some(),
            "Ask approval did not reach its scripted pre-dispatch failure seam"
        );
        let blocked = wait_for_status(
            &before_dispatch_scenario.host,
            before_dispatch_scenario.proposal.operation.action_ref,
            |status| {
                matches!(
                    status,
                    floe_calendar_operations::ActionStatus::Blocked { .. }
                )
            },
        );
        assert!(matches!(
            blocked.status,
            floe_calendar_operations::ActionStatus::Blocked {
                reason: floe_calendar_operations::ActionBlockedReason::ExecutorUnavailable
            }
        ));
        let before = before_dispatch_scenario.executor.snapshot();
        assert_eq!(before.pre_dispatch_failures, 1);
        assert_eq!(before.dispatch_attempts, 0);
        assert_eq!(before.external_effects, 0);

        let after_dispatch = prepare_scenario(
            OperationPolicyMode::Ask,
            CalendarExecutorScript::ApplyThenLoseAck,
        );
        let interaction = after_dispatch
            .proposal
            .interaction
            .as_ref()
            .expect("Ask publishes the typed Conversation review");
        let result = decide(
            &after_dispatch.host,
            after_dispatch.session_id,
            interaction,
            floe_conversation::InteractionDecisionKind::Approve,
            interaction.target_digest,
        )
        .expect("Conversation resolves the review through Access");
        assert!(matches!(
            result,
            ProductCommandOutcome::Conversation(ConversationCommandOutcome::Interaction(_))
        ));
        let dispatched = after_dispatch
            .executor
            .wait_for_snapshot(Duration::from_secs(20), |snapshot| {
                snapshot.dispatch_attempts == 1
            });
        assert!(
            dispatched.is_some(),
            "approved operation did not reach scripted dispatch; owner status={:?}; executor={:?}",
            inspect(
                &after_dispatch.host,
                after_dispatch.proposal.operation.action_ref
            )
            .map(|snapshot| snapshot.status),
            after_dispatch.executor.snapshot()
        );
        let unknown = wait_for_status(
            &after_dispatch.host,
            after_dispatch.proposal.operation.action_ref,
            |status| {
                matches!(
                    status,
                    floe_calendar_operations::ActionStatus::Unknown { .. }
                        | floe_calendar_operations::ActionStatus::Blocked { .. }
                        | floe_calendar_operations::ActionStatus::Succeeded { .. }
                        | floe_calendar_operations::ActionStatus::Failed { .. }
                )
            },
        );
        let after = after_dispatch.executor.snapshot();
        assert!(
            matches!(
                unknown.status,
                floe_calendar_operations::ActionStatus::Unknown { .. }
            ),
            "expected post-dispatch uncertainty, got {:?} with executor {:?}",
            unknown.status,
            after
        );
        assert_eq!(after.dispatch_attempts, 1);
        assert_eq!(after.external_effects, 1);
        assert_eq!(after.effect_kinds, vec!["create"]);
    }

    #[test]
    fn conversation_review_rejects_stale_target_policy_and_source_without_dispatch() {
        let stale_target =
            prepare_scenario(OperationPolicyMode::Ask, CalendarExecutorScript::Commit);
        let interaction = stale_target
            .proposal
            .interaction
            .as_ref()
            .expect("Ask publishes the typed Conversation review");
        let mut changed_digest = interaction.target_digest;
        changed_digest[0] ^= 1;
        let stale_review = decide(
            &stale_target.host,
            stale_target.session_id,
            interaction,
            floe_conversation::InteractionDecisionKind::Approve,
            changed_digest,
        )
        .expect_err("stale review digest cannot resolve the exact interaction");
        assert_eq!(
            stale_review.disposition,
            ProductCommandDisposition::NotApplied
        );
        assert_eq!(stale_target.executor.snapshot().dispatch_attempts, 0);
        let approved = decide(
            &stale_target.host,
            stale_target.session_id,
            interaction,
            floe_conversation::InteractionDecisionKind::Approve,
            interaction.target_digest,
        )
        .expect("the unchanged exact review remains resolvable");
        assert!(matches!(
            approved,
            ProductCommandOutcome::Conversation(ConversationCommandOutcome::Interaction(_))
        ));
        assert!(matches!(
            wait_for_status(
                &stale_target.host,
                stale_target.proposal.operation.action_ref,
                |status| matches!(
                    status,
                    floe_calendar_operations::ActionStatus::Succeeded { .. }
                )
            )
            .status,
            floe_calendar_operations::ActionStatus::Succeeded { .. }
        ));

        let stale_policy =
            prepare_scenario(OperationPolicyMode::Ask, CalendarExecutorScript::Commit);
        set_policy(&stale_policy.host, OperationPolicyMode::Deny);
        assert!(matches!(
            inspect(
                &stale_policy.host,
                stale_policy.proposal.operation.action_ref
            )
            .expect("policy revision invalidates the pending operation")
            .status,
            floe_calendar_operations::ActionStatus::Blocked {
                reason: floe_calendar_operations::ActionBlockedReason::PolicyDenied
            }
        ));
        let interaction = stale_policy.proposal.interaction.as_ref().unwrap();
        let stale_decision = decide(
            &stale_policy.host,
            stale_policy.session_id,
            interaction,
            floe_conversation::InteractionDecisionKind::Approve,
            interaction.target_digest,
        )
        .expect_err("the old review cannot consume authority at a newer policy revision");
        assert_eq!(
            stale_decision.disposition,
            ProductCommandDisposition::Admitted
        );
        assert_eq!(stale_policy.executor.snapshot().dispatch_attempts, 0);

        let stale_source =
            prepare_scenario(OperationPolicyMode::Ask, CalendarExecutorScript::Commit);
        support::disconnect_fixture_calendar(&stale_source.host, &stale_source.source);
        let interaction = stale_source.proposal.interaction.as_ref().unwrap();
        let result = decide(
            &stale_source.host,
            stale_source.session_id,
            interaction,
            floe_conversation::InteractionDecisionKind::Approve,
            interaction.target_digest,
        );
        if let Err(failure) = result {
            assert_eq!(failure.disposition, ProductCommandDisposition::Admitted);
        }
        let blocked = wait_for_status(
            &stale_source.host,
            stale_source.proposal.operation.action_ref,
            |status| {
                matches!(
                    status,
                    floe_calendar_operations::ActionStatus::Blocked { .. }
                        | floe_calendar_operations::ActionStatus::Succeeded { .. }
                        | floe_calendar_operations::ActionStatus::Unknown { .. }
                )
            },
        );
        assert!(matches!(
            blocked.status,
            floe_calendar_operations::ActionStatus::Blocked {
                reason: floe_calendar_operations::ActionBlockedReason::SourceChanged
                    | floe_calendar_operations::ActionBlockedReason::ExecutorUnavailable
                    | floe_calendar_operations::ActionBlockedReason::PolicyDenied
            }
        ));
        assert_eq!(stale_source.executor.snapshot().dispatch_attempts, 0);
    }

    #[test]
    fn day_pages_only_direct_origins_and_rejects_expert_reconciliation_before_owner_mutation() {
        let scenario = prepare_scenario_with_scripts(
            OperationPolicyMode::Ask,
            [CalendarExecutorScript::MissingReceipt],
        );
        let destinations = scenario
            .host
            .request(Uuid::new_v4())
            .expect("admit Day destination query")
            .product_query(ProductQuery::Day(
                DayProductQuery::ExternalCalendarDestinations,
            ))
            .expect("read direct Day destinations");
        let ProductQueryOutcome::Day(DayQueryOutcome::ExternalCalendarDestinations(destinations)) =
            destinations
        else {
            panic!("Day returned a different destination result")
        };
        let [destination] = destinations.as_slice() else {
            panic!("expected one direct Calendar destination: {destinations:?}")
        };

        let create_direct = |index: usize, command_id: Uuid| {
            let starts_at = chrono::DateTime::parse_from_rfc3339("2026-10-13T10:00:00Z")
                .expect("fixed direct operation start")
                .with_timezone(&chrono::Utc);
            let ends_at = chrono::DateTime::parse_from_rfc3339("2026-10-13T11:00:00Z")
                .expect("fixed direct operation end")
                .with_timezone(&chrono::Utc);
            let result = scenario
                .host
                .request(Uuid::new_v4())
                .expect("admit direct Day create")
                .product_command(ProductCommandRequest {
                    command_id: floe_kernel::CommandId::from_uuid(command_id).unwrap(),
                    command: ProductCommand::Day(DayCommand::ExternalCalendarOperation {
                        operation: floe_day::ManualCalendarOperation::Create {
                            destination_ref: destination.destination_ref,
                            title: format!("Direct pagination fixture {index}"),
                            schedule: floe_day::TimedSchedule::new(starts_at, ends_at, "UTC")
                                .expect("valid direct create schedule"),
                        },
                    }),
                })
                .expect("Day directs the write despite the independent agent Ask policy");
            let ProductCommandOutcome::Day(DayCommandOutcome::ExternalCalendarOperation(value)) =
                result
            else {
                panic!("Day returned a different direct operation result")
            };
            value
        };

        let expert_id = scenario.proposal.operation.action_ref;
        // Direct operation IDs are hashed from command IDs. Select one on each
        // side of the Expert ID so this test always exercises a mixed-origin
        // cursor page without relying on a lucky random ordering.
        let (direct_before_expert, direct_after_expert) =
            support::with_ready(&scenario.host, |_, caller, _| {
                let person_id = floe_kernel::PersonId(caller.person_id());
                let mut before = None;
                let mut after = Vec::with_capacity(2);
                for _ in 0..1_000_000 {
                    let command_id = Uuid::new_v4();
                    let operation_id = floe_calendar_operations::action_uuid(
                        b"floe.actions.action.v1\0",
                        person_id,
                        command_id,
                    );
                    if operation_id < expert_id {
                        before.get_or_insert(command_id);
                    } else if after.len() < 2 {
                        after.push(command_id);
                    }
                    if before.is_some() && after.len() == 2 {
                        return (
                            before.expect("selected direct command before Expert"),
                            after,
                        );
                    }
                }
                panic!("could not choose direct commands around Expert operation ID");
            });
        let missing_receipt = create_direct(0, direct_before_expert);
        let unknown = wait_for_status(&scenario.host, missing_receipt.operation_id, |status| {
            matches!(
                status,
                floe_calendar_operations::ActionStatus::Unknown { .. }
            )
        });
        assert!(matches!(
            unknown.status,
            floe_calendar_operations::ActionStatus::Unknown { .. }
        ));
        let mut direct_ids = vec![missing_receipt.operation_id];
        for (index, command_id) in direct_after_expert.into_iter().enumerate() {
            let operation = create_direct(index + 1, command_id);
            let snapshot = wait_for_status(&scenario.host, operation.operation_id, |status| {
                matches!(
                    status,
                    floe_calendar_operations::ActionStatus::Succeeded { .. }
                        | floe_calendar_operations::ActionStatus::Unknown { .. }
                )
            });
            assert!(matches!(
                snapshot.status,
                floe_calendar_operations::ActionStatus::Succeeded { .. }
            ));
            direct_ids.push(operation.operation_id);
        }

        let cursor_before_expert = direct_ids
            .iter()
            .copied()
            .filter(|id| *id < expert_id)
            .max();
        let mixed = support::with_ready(&scenario.host, |services, caller, owners| {
            let actor = caller.owner_actor();
            services.execute_owner(async move {
                owners
                    .calendar_operations
                    .list(
                        &actor,
                        None,
                        100,
                        &floe_app::host_scope(
                            Uuid::new_v4(),
                            floe_execution::Cancellation::new(),
                            Duration::from_secs(15),
                        ),
                    )
                    .await
            })
        })
        .expect("read mixed owner page for the pagination fixture");
        assert!(
            mixed
                .actions
                .iter()
                .any(|action| action.action_ref == expert_id)
        );
        assert!(
            direct_ids
                .iter()
                .all(|id| mixed.actions.iter().any(|item| item.action_ref == *id))
        );

        let first_page = scenario
            .host
            .request(Uuid::new_v4())
            .expect("admit Day cursor page query")
            .product_query(ProductQuery::Day(
                DayProductQuery::ExternalCalendarOperations {
                    cursor: cursor_before_expert,
                    limit: 1,
                },
            ))
            .expect("source query filters Direct before limit and cursor");
        let ProductQueryOutcome::Day(DayQueryOutcome::ExternalCalendarOperations(first_page)) =
            first_page
        else {
            panic!("Day returned a different operation page")
        };
        assert_eq!(first_page.operations.len(), 1);
        assert!(direct_ids.contains(&first_page.operations[0].operation_id));
        assert_ne!(first_page.operations[0].operation_id, expert_id);
        assert!(first_page.next_cursor.is_some());

        let mut page_cursor = None;
        let mut listed = Vec::new();
        loop {
            let result = scenario
                .host
                .request(Uuid::new_v4())
                .expect("admit Day operation page")
                .product_query(ProductQuery::Day(
                    DayProductQuery::ExternalCalendarOperations {
                        cursor: page_cursor,
                        limit: 1,
                    },
                ))
                .expect("read the next Direct-origin page");
            let ProductQueryOutcome::Day(DayQueryOutcome::ExternalCalendarOperations(page)) =
                result
            else {
                panic!("Day returned a different operation page")
            };
            listed.extend(
                page.operations
                    .iter()
                    .map(|operation| operation.operation_id),
            );
            page_cursor = page.next_cursor;
            if page_cursor.is_none() {
                break;
            }
        }
        assert_eq!(listed.len(), direct_ids.len());
        assert!(listed.iter().all(|id| direct_ids.contains(id)));
        assert!(listed.contains(&missing_receipt.operation_id));
        assert!(!listed.contains(&expert_id));
        let unknown_row = inspect(&scenario.host, missing_receipt.operation_id)
            .expect("inspect retained Unknown direct operation");
        assert!(matches!(
            unknown_row.status,
            floe_calendar_operations::ActionStatus::Unknown { .. }
        ));

        scenario
            .executor
            .queue_script(CalendarExecutorScript::ApplyThenLoseAck);
        let interaction = scenario.proposal.interaction.as_ref().unwrap();
        let decision_id = Uuid::new_v4();
        decide_with_command_id(
            &scenario.host,
            scenario.session_id,
            interaction,
            floe_conversation::InteractionDecisionKind::Approve,
            interaction.target_digest,
            decision_id,
        )
        .expect("approve the Expert operation through its Access review");
        let expert_unknown = wait_for_status(&scenario.host, expert_id, |status| {
            matches!(
                status,
                floe_calendar_operations::ActionStatus::Unknown { .. }
            )
        });
        // Resolving the review starts a linked Conversation resume; let its
        // durable writes settle before exercising Day's direct-origin fence.
        let linked_resume = wait_for_linked_resume(&scenario.host, interaction.origin_run_id);
        assert_eq!(linked_resume.state, floe_conversation::RunState::Completed);
        assert_eq!(linked_resume.issue, None);
        let reconciliation_command = Uuid::new_v4();
        let lookups_before_reject = scenario.executor.snapshot().lookups;
        let rejected_expert_reconciliation = scenario
            .host
            .request(Uuid::new_v4())
            .expect("admit Day Expert-target reconciliation attempt")
            .product_command(ProductCommandRequest {
                command_id: floe_kernel::CommandId::from_uuid(reconciliation_command).unwrap(),
                command: ProductCommand::Day(DayCommand::ReconcileExternalCalendarOperation {
                    operation_ref: expert_id,
                    expected_revision: expert_unknown.revision,
                }),
            })
            .expect_err("Day cannot reconcile an Expert operation");
        assert_eq!(
            rejected_expert_reconciliation.disposition,
            ProductCommandDisposition::NotApplied
        );
        assert!(
            matches!(
                &rejected_expert_reconciliation.failure,
                ProductFailure::Day(failure) if failure.code == floe_app::ErrorCode::Conflict
            ),
            "Day rejected the Expert origin with a different error: {:?}",
            rejected_expert_reconciliation.failure
        );
        assert_eq!(scenario.executor.snapshot().lookups, lookups_before_reject);
        assert!(matches!(
            inspect(&scenario.host, expert_id).unwrap().status,
            floe_calendar_operations::ActionStatus::Unknown { .. }
        ));

        let direct_unknown_snapshot =
            inspect(&scenario.host, missing_receipt.operation_id).unwrap();
        let direct_reconciliation = scenario
            .host
            .request(Uuid::new_v4())
            .expect("reuse the globally unoccupied reconciliation command")
            .product_command(ProductCommandRequest {
                command_id: floe_kernel::CommandId::from_uuid(reconciliation_command).unwrap(),
                command: ProductCommand::Day(DayCommand::ReconcileExternalCalendarOperation {
                    operation_ref: missing_receipt.operation_id,
                    expected_revision: direct_unknown_snapshot.revision,
                }),
            })
            .expect("failed Expert reconciliation did not reserve the command ID");
        let ProductCommandOutcome::Day(DayCommandOutcome::ReconciledExternalCalendarOperation(
            direct_receipt,
        )) = direct_reconciliation
        else {
            panic!("Day returned a different direct reconciliation result");
        };
        assert_eq!(direct_receipt.operation_id, missing_receipt.operation_id);
        let lookup_deadline = Instant::now() + Duration::from_secs(10);
        while scenario.executor.snapshot().lookups == lookups_before_reject {
            assert!(
                Instant::now() < lookup_deadline,
                "direct reconciliation did not look up its owner receipt"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
