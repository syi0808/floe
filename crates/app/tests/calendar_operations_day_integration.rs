mod support;

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
mod qa {
    use std::time::{Duration, Instant};

    use floe_app::{
        AppComposition, AppHost, ConversationCommand, ConversationCommandOutcome,
        ConversationQuery, ConversationQueryOutcome, DayCommand, DayCommandOutcome,
        DayProductQuery, DayQueryOutcome, OperationPolicyMode, ProductCommand,
        ProductCommandOutcome, ProductCommandRequest, ProductQuery, ProductQueryOutcome,
        RuntimeReadinessState,
    };
    use floe_kernel::AgentFailure;
    use uuid::Uuid;

    use crate::support;
    use crate::support::{
        CalendarExecutorScript, IsolatedProfile, ModelOutput, PrimaryBehavior,
        ScriptedCalendarExecutor, ScriptedModel, prepare_runtime,
    };

    const QUERY_DATE: (i32, u32, u32) = (2026, 10, 8);

    fn start_fixture_day(
        executor: ScriptedCalendarExecutor,
    ) -> (
        IsolatedProfile,
        AppHost<AppComposition>,
        ScriptedCalendarExecutor,
        floe_connections::SourceSummary,
        floe_day::DayEvent,
    ) {
        let model = ScriptedModel::new(
            PrimaryBehavior::NoGateway,
            "Manual calendar command integration fixture.",
            ModelOutput::Answer,
        );
        let profile = IsolatedProfile::new();
        let host = profile.open_with_qa_calendar_executor(&model, executor.clone());
        assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
        let source = support::configure_fixture_calendar(&host, "Synthetic team calendar");

        let query = floe_app::DayQuery {
            date: chrono::NaiveDate::from_ymd_opt(QUERY_DATE.0, QUERY_DATE.1, QUERY_DATE.2)
                .expect("fixed fixture day"),
            timezone_offset_seconds: 0,
            end_timezone_offset_seconds: None,
            now: chrono::DateTime::parse_from_rfc3339("2026-10-07T00:00:00Z")
                .expect("fixed fixture instant")
                .with_timezone(&chrono::Utc),
        };
        let admitted = host
            .request(Uuid::new_v4())
            .expect("admit the Day refresh command")
            .product_command(ProductCommandRequest {
                command_id: floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap(),
                command: ProductCommand::Day(DayCommand::Refresh(query.clone())),
            })
            .expect("refresh the configured fixture calendar");
        let ProductCommandOutcome::Day(DayCommandOutcome::Refresh(admitted)) = admitted else {
            panic!("Day returned a different refresh result")
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let result = host
                .request(Uuid::new_v4())
                .expect("admit refresh status query")
                .product_query(ProductQuery::Day(DayProductQuery::RefreshGet {
                    operation_ref: admitted.operation_ref,
                }))
                .expect("read Day refresh status");
            let ProductQueryOutcome::Day(DayQueryOutcome::Refresh(refresh)) = result else {
                panic!("Day returned a different refresh query result")
            };
            if refresh.state.terminal() {
                assert!(
                    matches!(refresh.state, floe_day::DayRefreshState::Completed { .. }),
                    "fixture refresh did not complete: {:?}",
                    refresh.state
                );
                break;
            }
            assert!(Instant::now() < deadline, "fixture Day refresh timed out");
            std::thread::sleep(Duration::from_millis(20));
        }
        let result = host
            .request(Uuid::new_v4())
            .expect("admit Day snapshot query")
            .product_query(ProductQuery::Day(DayProductQuery::Snapshot(query)))
            .expect("read the refreshed Day snapshot");
        let ProductQueryOutcome::Day(DayQueryOutcome::Snapshot(snapshot)) = result else {
            panic!("Day returned a different snapshot query result")
        };
        let event = snapshot
            .items
            .into_iter()
            .find_map(|item| match item {
                floe_day::DayTimelineItem::Event(event)
                    if event.title == "Synthetic planning event" =>
                {
                    Some(event)
                }
                _ => None,
            })
            .expect("selected fixture event is present in Day");
        assert!(event.action_target.is_some());
        (profile, host, executor, source, event)
    }

    fn set_agent_policy_deny(host: &AppHost<AppComposition>) {
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
        let result = host
            .request(Uuid::new_v4())
            .expect("admit Conversation policy command")
            .product_command(ProductCommandRequest {
                command_id: floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap(),
                command: ProductCommand::Conversation(
                    ConversationCommand::SetCalendarOperationPolicy {
                        mode: OperationPolicyMode::Deny,
                        expected_revision: policy.revision,
                    },
                ),
            })
            .expect("set agent Calendar policy to Deny");
        let ProductCommandOutcome::Conversation(
            ConversationCommandOutcome::CalendarOperationPolicy(updated),
        ) = result
        else {
            panic!("Conversation returned a different policy command result")
        };
        assert_eq!(updated.calendar_create, OperationPolicyMode::Deny);
    }

    fn assert_missing_receipt_stays_unknown(delete: bool) {
        let executor = ScriptedCalendarExecutor::new([CalendarExecutorScript::MissingReceipt]);
        let (_profile, host, executor, _source, event) = start_fixture_day(executor);
        set_agent_policy_deny(&host);
        let target = event
            .action_target
            .expect("external event retains its exact revision");
        let operation = if delete {
            floe_day::ManualCalendarOperation::Delete {
                event_ref: target.event_id,
                expected_revision: target.expected_revision,
            }
        } else {
            let starts_at = chrono::DateTime::parse_from_rfc3339("2026-10-12T10:00:00Z")
                .expect("future manual operation start")
                .with_timezone(&chrono::Utc);
            let ends_at = chrono::DateTime::parse_from_rfc3339("2026-10-12T11:00:00Z")
                .expect("future manual operation end")
                .with_timezone(&chrono::Utc);
            floe_day::ManualCalendarOperation::Update {
                event_ref: target.event_id,
                expected_revision: target.expected_revision,
                title: "Manual update under agent Deny".into(),
                schedule: floe_day::TimedSchedule::new(starts_at, ends_at, "UTC")
                    .expect("valid update schedule"),
            }
        };
        let command_id = Uuid::new_v4();
        let result = host
            .request(Uuid::new_v4())
            .expect("admit direct Day operation")
            .product_command(ProductCommandRequest {
                command_id: floe_kernel::CommandId::from_uuid(command_id).unwrap(),
                command: ProductCommand::Day(DayCommand::ExternalCalendarOperation { operation }),
            })
            .expect("Day forwards the manual operation to Calendar Operations");
        let ProductCommandOutcome::Day(DayCommandOutcome::ExternalCalendarOperation(submitted)) =
            result
        else {
            panic!("Day returned a different manual operation result")
        };

        let deadline = Instant::now() + Duration::from_secs(20);
        let operation = loop {
            let action_ref = submitted.operation_id;
            let inspected = support::with_ready(&host, |services, caller, owners| {
                let actor = caller.owner_actor();
                services.execute_owner(async move {
                    owners
                        .calendar_operations
                        .inspect(
                            &actor,
                            action_ref,
                            &floe_app::host_scope(
                                Uuid::new_v4(),
                                floe_execution::Cancellation::new(),
                                Duration::from_secs(10),
                            ),
                        )
                        .await
                })
            });
            match inspected {
                Ok(snapshot)
                    if matches!(
                        snapshot.status,
                        floe_calendar_operations::ActionStatus::Unknown { .. }
                            | floe_calendar_operations::ActionStatus::Blocked { .. }
                            | floe_calendar_operations::ActionStatus::Succeeded { .. }
                    ) =>
                {
                    break snapshot;
                }
                Ok(_) => {}
                Err(AgentFailure::StorageBusy) => {}
                Err(failure) => panic!("inspect direct Day operation: {failure:?}"),
            }
            assert!(Instant::now() < deadline, "manual operation did not settle");
            std::thread::sleep(Duration::from_millis(25));
        };
        assert!(
            matches!(
                operation.status,
                floe_calendar_operations::ActionStatus::Unknown { .. }
            ),
            "a missing causal receipt must remain Unknown: {:?}; executor={:?}",
            operation.status,
            executor.snapshot()
        );

        let query = host
            .request(Uuid::new_v4())
            .expect("admit Day operation query")
            .product_query(ProductQuery::Day(
                DayProductQuery::ExternalCalendarOperationGet {
                    operation_ref: operation.action_ref,
                },
            ))
            .expect("read direct operation from Day");
        let ProductQueryOutcome::Day(DayQueryOutcome::ExternalCalendarOperation(day_receipt)) =
            query
        else {
            panic!("Day returned a different external operation query result")
        };
        assert_eq!(
            day_receipt.status,
            floe_day::ManualCalendarOperationStatus::Unknown
        );

        let before_reconcile = executor.snapshot();
        let reconciled = host
            .request(Uuid::new_v4())
            .expect("admit Day reconciliation command")
            .product_command(ProductCommandRequest {
                command_id: floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap(),
                command: ProductCommand::Day(DayCommand::ReconcileExternalCalendarOperation {
                    operation_ref: operation.action_ref,
                    expected_revision: operation.revision,
                }),
            })
            .expect("Calendar Operations performs lookup-only reconciliation through Day");
        let ProductCommandOutcome::Day(DayCommandOutcome::ReconciledExternalCalendarOperation(
            reconciled,
        )) = reconciled
        else {
            panic!("Day returned a different reconciliation result")
        };
        assert_eq!(
            reconciled.status,
            floe_day::ManualCalendarOperationStatus::Unknown
        );
        let reconcile_deadline = Instant::now() + Duration::from_secs(10);
        let after_reconcile = loop {
            let snapshot = executor.snapshot();
            if snapshot.lookups > before_reconcile.lookups {
                break snapshot;
            }
            assert!(
                Instant::now() < reconcile_deadline,
                "Day reconciliation did not perform a provider receipt lookup"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(after_reconcile.dispatch_attempts, 1);
        assert_eq!(
            after_reconcile.dispatch_attempts,
            before_reconcile.dispatch_attempts
        );
        assert_eq!(after_reconcile.external_effects, 1);
        assert_eq!(
            after_reconcile.effect_kinds,
            vec![if delete { "delete" } else { "update" }]
        );
    }

    fn update_operation(
        event: &floe_day::DayEvent,
        expected_revision: floe_kernel::Revision,
    ) -> floe_day::ManualCalendarOperation {
        let target = event.action_target.as_ref().expect("external event target");
        let starts_at = chrono::DateTime::parse_from_rfc3339("2026-10-12T10:00:00Z")
            .expect("future manual operation start")
            .with_timezone(&chrono::Utc);
        let ends_at = chrono::DateTime::parse_from_rfc3339("2026-10-12T11:00:00Z")
            .expect("future manual operation end")
            .with_timezone(&chrono::Utc);
        floe_day::ManualCalendarOperation::Update {
            event_ref: target.event_id,
            expected_revision,
            title: "Stale manual operation".into(),
            schedule: floe_day::TimedSchedule::new(starts_at, ends_at, "UTC")
                .expect("valid update schedule"),
        }
    }

    #[test]
    fn direct_day_operations_reject_stale_event_revision_and_source_before_dispatch() {
        let executor = ScriptedCalendarExecutor::new([]);
        let (_profile, host, executor, source, event) = start_fixture_day(executor);
        let target = event.action_target.as_ref().expect("external event target");
        let stale_command = host
            .request(Uuid::new_v4())
            .expect("admit stale event command")
            .product_command(ProductCommandRequest {
                command_id: floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap(),
                command: ProductCommand::Day(DayCommand::ExternalCalendarOperation {
                    operation: update_operation(
                        &event,
                        floe_kernel::Revision(target.expected_revision.0 + 1),
                    ),
                }),
            })
            .expect_err("the Day caller cannot update a stale event revision");
        assert_eq!(
            stale_command.disposition,
            floe_app::ProductCommandDisposition::NotApplied
        );
        assert!(matches!(
            stale_command.failure,
            floe_app::ProductFailure::Day(ref failure)
                if failure.code == floe_app::ErrorCode::Conflict
        ));
        assert_eq!(executor.snapshot().dispatch_attempts, 0);

        support::disconnect_fixture_calendar(&host, &source);
        let stale_source = host
            .request(Uuid::new_v4())
            .expect("admit stale source command")
            .product_command(ProductCommandRequest {
                command_id: floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap(),
                command: ProductCommand::Day(DayCommand::ExternalCalendarOperation {
                    operation: update_operation(&event, target.expected_revision),
                }),
            })
            .expect_err("the Day caller cannot write through a disconnected source");
        assert_eq!(
            stale_source.disposition,
            floe_app::ProductCommandDisposition::NotApplied
        );
        assert!(matches!(
            stale_source.failure,
            floe_app::ProductFailure::Day(ref failure)
                if failure.code == floe_app::ErrorCode::Conflict
        ));
        assert_eq!(executor.snapshot().dispatch_attempts, 0);
        assert_eq!(executor.snapshot().preparations, 0);
    }

    #[test]
    fn direct_day_update_and_delete_ignore_agent_deny_but_keep_missing_receipts_unknown() {
        assert_missing_receipt_stays_unknown(false);
        assert_missing_receipt_stays_unknown(true);
    }
}
