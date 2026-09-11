use chrono::TimeZone;
use floe_agent::*;
use floe_core::{
    CalendarAgentTurnRequest, CalendarReadAccess, CalendarReadAccessRequest,
    CalendarReadAccessStamp, CalendarTimelineGrant, ExpertCalendarDestination,
    ExpertCalendarRequest,
};
use floe_domain::{CalendarProvider, CalendarRange};

use super::*;

struct Access;

impl CalendarReadAccess for Access {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        Ok(CalendarReadAccessStamp {
            schema_version: 1,
            person_id: request.person_id,
            device_id: request.device_id,
            provider: request.provider,
            calendar_ids: request.calendar_ids,
            generation: "synthetic-generation".into(),
        })
    }
}

struct Model;

fn fixture_now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc.with_ymd_and_hms(2050, 1, 15, 9, 0, 0).unwrap()
}

impl ModelRunner for Model {
    fn placement(&self) -> ModelPlacement {
        ModelPlacement::DeviceLocal
    }

    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        let schedule_expert = request.prompt.role == PromptRole::ScheduleExpert;
        let step = if schedule_expert {
            let coverage = request.messages.iter().find_map(|message| match message {
                AgentMessage::User { text, .. } => serde_json::from_str::<serde_json::Value>(text)
                    .ok()
                    .map(|task| {
                        (
                            task["suggested_query_range"]["starts_at_unix_ms"].as_u64(),
                            task["suggested_query_range"]["ends_at_unix_ms"].as_u64(),
                        )
                    }),
                _ => None,
            });
            let (Some(starts_at_unix_ms), Some(ends_at_unix_ms)) =
                coverage.ok_or(AgentFailure::InvalidModelOutput)?
            else {
                return Err(AgentFailure::InvalidModelOutput);
            };
            let latest = request
                .messages
                .iter()
                .rev()
                .find_map(|message| match message {
                    AgentMessage::Capability {
                        capability_id,
                        result: Ok(output),
                        ..
                    } => Some((capability_id.as_str(), output.as_str())),
                    _ => None,
                });
            match latest {
                Some(("schedule.find_free_windows", _)) => ModelStep::Answer {
                    text: "Synthetic proposal recorded.".into(),
                },
                _ => ModelStep::Call {
                    capability_id: "schedule.find_free_windows".into(),
                    input: serde_json::json!({
                        "minimum_minutes": 60,
                        "range_start_unix_ms": starts_at_unix_ms,
                        "range_end_unix_ms": ends_at_unix_ms,
                    })
                    .to_string(),
                },
            }
        } else if request
            .messages
            .iter()
            .any(|message| matches!(message, AgentMessage::Delegation { .. }))
        {
            ModelStep::Answer {
                text: "Synthetic proposal recorded.".into(),
            }
        } else {
            ModelStep::Delegate {
                agent_id: request.active_agents[0].id.clone(),
                message: "Find a suitable time for this calendar request.".into(),
            }
        };
        Ok(ModelResponse {
            replay: None,
            schema_version: 1,
            output: vec![step],
            used_tokens: 10,
            cost_micros: 0,
        })
    }
}

async fn seed(
    root: &std::path::Path,
    person: PersonId,
    keys: Keys,
    core: &FloeCore,
) -> (AgentSession, ExpertResult) {
    let vault = EncryptedAgentVault::create(root, person, keys)
        .await
        .unwrap();
    let setup = vault
        .install_calendar_expert(
            CalendarExpertSetup {
                instance_id: vault.registry_instance_id(),
                expected_revision: 0,
                setup_id: Uuid::new_v4(),
                provider: CalendarProvider::Fixture,
                device_id: "test-device".into(),
                calendar_ids: vec!["test-calendar".into()],
            },
            Cancellation::default(),
        )
        .await
        .unwrap()
        .setup;
    for target in [
        RegistryConfigurationTarget::Installation {
            id: setup.tool_installation_id,
            enabled: true,
        },
        RegistryConfigurationTarget::Installation {
            id: setup.expert_installation_id,
            enabled: true,
        },
        RegistryConfigurationTarget::Assignment {
            id: setup.tool_assignment_id,
            enabled: true,
        },
        RegistryConfigurationTarget::Assignment {
            id: setup.expert_assignment_id,
            enabled: true,
        },
        RegistryConfigurationTarget::CalendarView {
            id: setup.view_handle,
            enabled: true,
        },
    ] {
        let registry = vault.registry_overview().await.unwrap().unwrap();
        vault
            .configure_registry(
                RegistryConfiguration {
                    instance_id: registry.instance_id,
                    expected_revision: registry.revision,
                    target,
                },
                Cancellation::default(),
            )
            .await
            .unwrap();
    }
    let now = fixture_now();
    let day = CalendarRange {
        start_date: now.date_naive(),
        end_date_exclusive: (now + chrono::Duration::days(1)).date_naive(),
        timezone_offset_seconds: 0,
        end_timezone_offset_seconds: None,
    };
    core.select_calendar(
        person,
        CalendarProvider::Fixture,
        "test-calendar".into(),
        "Synthetic".into(),
    )
    .await
    .unwrap();
    let revision = core
        .calendar_connection(person)
        .await
        .unwrap()
        .unwrap()
        .revision;
    core.import_calendar(person, revision, day.clone(), vec![], now)
        .await
        .unwrap();
    let revision = core
        .calendar_connection(person)
        .await
        .unwrap()
        .unwrap()
        .revision;
    let initial = vault.create_sample_session().await.unwrap();
    let result = core
        .run_calendar_agent_turn(
            &vault,
            &Access,
            &Model,
            CalendarAgentTurnRequest {
                continuation: false,
                command: AgentCommand {
                    schema_version: 1,
                    person_id: person,
                    session_id: initial.id,
                    expected_revision: initial.revision,
                    text: "Synthetic focus request".into(),
                },
                context: AgentContext {
                    projection_version: 1,
                    persona: None,
                    memories: vec![],
                    evidence: vec![],
                },
                policy: InferencePolicyDecision {
                    purpose: "synthetic-briefing".into(),
                    data_classes: vec![DataClass::Synthetic],
                    allowed_placements: vec![ModelPlacement::DeviceLocal],
                    performance_class: "fixture".into(),
                    projection_version: 1,
                    external_transfer_consent: TransferConsent::NotGranted,
                    bounded_sensitive_projection: false,
                },
                budget: AgentBudget::default(),
                grant: CalendarTimelineGrant {
                    person_id: person,
                    handle: setup.view_handle,
                    provider: CalendarProvider::Fixture,
                    device_id: "test-device".into(),
                    calendar_ids: vec!["test-calendar".into()],
                    connection_revision: revision,
                    day,
                    starts_at: now + chrono::Duration::minutes(5),
                    ends_at: now + chrono::Duration::hours(2),
                    expires_at: now + chrono::Duration::minutes(2),
                },
                assignment_id: setup.expert_assignment_id,
                feasibility: None,
                wellbeing: None,
                destination: None,
                propose_focus: true,
                cancellation: Cancellation::default(),
            },
            || now,
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(result.session.last_outcome, Some(AgentOutcome::Completed));
    let evidence = result
        .session
        .messages
        .iter()
        .find_map(|message| match message {
            AgentMessage::Delegation { task, .. } => Some(
                serde_json::from_str(task.data_part(EXPERT_RESULT_MEDIA_TYPE).unwrap()).unwrap(),
            ),
            _ => None,
        })
        .unwrap();
    (result.session, evidence)
}

#[test]
fn proposal_jobs_read_absent_and_published_actions_without_republishing_after_revocation() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let person = PersonId::new();
    let keys = Keys::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let core = Arc::new(
        runtime
            .block_on(FloeCore::open(directory.path().join("core.db")))
            .unwrap(),
    );
    let (session, evidence) = runtime.block_on(seed(&root, person, keys.clone(), &core));
    let reference = ExpertProposalReference {
        person_id: person,
        session_id: session.id,
        invocation_id: evidence.invocation_id,
    };
    let inspect = AgentVaultActionDto::InspectProposal {
        session_id: session.id.to_string(),
        invocation_id: evidence.invocation_id.to_string(),
    };
    let worker = Worker::with_core(
        root.clone(),
        keys.clone(),
        core.clone(),
        Arc::new(LocalContextStore::default()),
    )
    .unwrap();
    assert_eq!(
        perform(&worker, person, inspect.clone()).failure,
        Some(AgentFailure::VaultUnavailable)
    );
    perform(&worker, person, AgentVaultActionDto::Unlock {});
    let absent = perform(&worker, person, inspect.clone());
    assert_eq!(absent.state, Some(AgentVaultStateDto::Ready));
    assert!(absent.proposal.unwrap().action.is_none());
    assert!(
        runtime
            .block_on(core.calendar_actions(person))
            .unwrap()
            .is_empty()
    );
    perform(&worker, person, AgentVaultActionDto::Lock {});
    let action = runtime.block_on(async {
        let vault = EncryptedAgentVault::open(&root, person, keys.clone())
            .await
            .unwrap();
        let connection = core.calendar_connection(person).await.unwrap().unwrap();
        core.prepare_expert_calendar_action(
            &vault,
            ExpertCalendarRequest {
                reference,
                destination: ExpertCalendarDestination {
                    provider: CalendarProvider::Fixture,
                    calendar_id: "test-calendar".into(),
                    connection_revision: connection.revision,
                    timezone: "Asia/Seoul".into(),
                },
                cancellation: Cancellation::default(),
                deadline: tokio::time::Instant::now() + Duration::from_secs(5),
            },
            fixture_now,
        )
        .await
        .unwrap()
    });
    perform(&worker, person, AgentVaultActionDto::Unlock {});
    let overview = perform(
        &worker,
        person,
        AgentVaultActionDto::Registry { change: None },
    )
    .registry
    .unwrap();
    perform(
        &worker,
        person,
        AgentVaultActionDto::Registry {
            change: Some(
                encode_contract(&RegistryConfiguration {
                    instance_id: overview.instance_id,
                    expected_revision: overview.revision,
                    target: RegistryConfigurationTarget::Assignment {
                        id: evidence.assignment_id,
                        enabled: false,
                    },
                })
                .unwrap(),
            ),
        },
    );
    let id = Uuid::new_v4();
    let submit = AgentVaultOperationDto::Submit {
        action: inspect.clone(),
    };
    worker.request(person, id, submit.clone()).unwrap();
    let result = wait(&worker, person, id);
    assert_eq!(worker.request(person, id, submit).unwrap(), result);
    assert!(result.failure.is_none() && result.events.is_empty() && result.session.is_none());
    let projection = result.proposal.unwrap();
    assert_eq!(projection.session_id, session.id.to_string());
    let linked = projection.action.unwrap();
    assert_eq!(linked.action_id, action.id.to_string());
    assert_eq!(linked.execution_id, action.execution_id.to_string());
    assert_eq!(linked.status, AgentProposalStatusDto::Pending);
    let wire = serde_json::to_string(&linked).unwrap();
    for private in [
        "calendar_id",
        "source_handle",
        "Synthetic",
        "private_state",
        "external_id",
    ] {
        assert!(!wire.contains(private));
    }
    worker
        .request(person, id, AgentVaultOperationDto::Release {})
        .unwrap();
    let invalid = perform(
        &worker,
        person,
        AgentVaultActionDto::InspectProposal {
            session_id: "not-a-uuid".into(),
            invocation_id: evidence.invocation_id.to_string(),
        },
    );
    assert_eq!(invalid.failure, Some(AgentFailure::InvalidInput));
    assert!(invalid.proposal.is_none());
    assert_eq!(
        perform(&worker, PersonId::new(), inspect.clone()).failure,
        Some(AgentFailure::NotFound)
    );
    let saved = perform(
        &worker,
        person,
        AgentVaultActionDto::Session {
            operation: AgentFixtureOperationDto::Get {
                session_id: session.id.to_string(),
            },
        },
    )
    .session
    .unwrap();
    assert_eq!(saved, session);
    assert_eq!(
        runtime.block_on(core.calendar_actions(person)).unwrap(),
        vec![action]
    );
    keys.0.unavailable.store(true, Ordering::Release);
    let unavailable = perform(&worker, person, inspect);
    assert_eq!(unavailable.failure, Some(AgentFailure::VaultUnavailable));
    assert!(unavailable.proposal.is_none());
}

#[test]
fn stopped_proposal_inspection_retains_the_owned_job_until_key_access_finishes() {
    let directory = tempfile::tempdir().unwrap();
    let keys = Keys::default();
    let person = PersonId::new();
    let worker = Worker::new(directory.path().join("vaults"), keys.clone()).unwrap();
    perform(&worker, person, AgentVaultActionDto::Create {});
    keys.0.entered.store(false, Ordering::Release);
    *keys.0.paused.lock().unwrap() = true;
    let id = Uuid::new_v4();
    let inspect = AgentVaultActionDto::InspectProposal {
        session_id: Uuid::new_v4().to_string(),
        invocation_id: Uuid::new_v4().to_string(),
    };
    worker
        .request(
            person,
            id,
            AgentVaultOperationDto::Submit { action: inspect },
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !keys.0.entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !worker
            .request(person, id, AgentVaultOperationDto::Stop {})
            .unwrap()
            .done
    );
    assert_eq!(
        worker
            .request(person, id, AgentVaultOperationDto::Release {})
            .unwrap_err(),
        AgentFailure::Conflict
    );
    *keys.0.paused.lock().unwrap() = false;
    keys.0.wake.notify_all();
    let result = wait(&worker, person, id);
    assert_eq!(result.failure, Some(AgentFailure::Cancelled));
    assert!(result.proposal.is_none());
}
