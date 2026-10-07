mod support;

use std::time::{Duration, Instant};

use floe_app::{AppComposition, AppHost, VaultLifecycleCommand};
use floe_conversation::{RunState, SessionMessage};
use floe_inference::ModelObservationError;
use floe_kernel::AgentFailure;
use uuid::Uuid;

use support::{
    GenerateBarrier, IsolatedProfile, ModelOutput, PlanStage, PrimaryBehavior, ScriptedModel,
    activate_vault, assert_script_clean, cancel_run, read_events, read_run, read_session,
    start_session, start_turn, terminal_run_event, wait_terminal_run,
};

const USER_TEXT: &str = "Summarize my current priorities.";
const REPLY: &str = "A deterministic scripted reply.";
const WAIT_TIMEOUT: Duration = Duration::from_secs(20);

fn create_ready_app(model: &ScriptedModel) -> (IsolatedProfile, AppHost<AppComposition>) {
    let profile = IsolatedProfile::new();
    let host = profile.open(model);
    assert_eq!(
        activate_vault(&host, VaultLifecycleCommand::Create),
        floe_app::VaultState::Ready
    );
    (profile, host)
}

fn wait_terminal_event(
    host: &AppHost<AppComposition>,
    run_id: floe_kernel::RunId,
) -> floe_conversation::RunEventRecord {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        if let Some(event) = terminal_run_event(read_events(host), run_id) {
            return event;
        }
        assert!(
            Instant::now() < deadline,
            "terminal Conversation event exceeded deadline"
        );
        std::thread::yield_now();
    }
}

#[test]
fn text_turn_completes_and_persists_transcript_receipt_and_events() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, USER_TEXT);
    let receipt = wait_terminal_run(&host, command.run_id);

    assert_eq!(receipt.state, RunState::Completed);
    assert_eq!(receipt.output.as_deref(), Some(REPLY));
    assert_eq!(receipt.issue, None);
    let transcript = read_session(&host, session_id);
    assert_eq!(transcript.id, session_id);
    assert!(transcript.messages.iter().any(|message| matches!(
        message,
        SessionMessage::User { text, .. } if text == USER_TEXT
    )));
    assert!(transcript.messages.iter().any(|message| matches!(
        message,
        SessionMessage::Assistant { text, .. } if text == REPLY
    )));

    let event = wait_terminal_event(&host, command.run_id);
    assert_eq!(event.state, RunState::Completed);
    assert!(event.generated_reply);

    let snapshot = assert_script_clean(&recorder);
    assert_eq!(
        snapshot
            .plans
            .iter()
            .map(|(stage, _)| *stage)
            .collect::<Vec<_>>(),
        vec![PlanStage::Primary, PlanStage::LocalFallback]
    );
    assert_eq!(snapshot.generated.len(), 1);
    assert!(snapshot.generated[0].catalog.tools.is_empty());
}

#[test]
fn primary_observation_failure_never_invokes_fallback_or_generate() {
    let model = ScriptedModel::new(
        PrimaryBehavior::Fail(ModelObservationError::TransportUnavailable),
        USER_TEXT,
        ModelOutput::Answer,
    );
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, USER_TEXT);
    let receipt = wait_terminal_run(&host, command.run_id);

    assert_eq!(receipt.state, RunState::Failed);
    assert_eq!(receipt.issue, Some(AgentFailure::ServerModelUnavailable));
    assert_eq!(receipt.output, None);
    assert_eq!(
        wait_terminal_event(&host, command.run_id).state,
        RunState::Failed
    );

    let snapshot = assert_script_clean(&recorder);
    assert_eq!(snapshot.plans.len(), 1);
    assert_eq!(snapshot.plans[0].0, PlanStage::Primary);
    assert!(snapshot.generated.is_empty());
}

#[test]
fn in_flight_model_dispatch_can_be_cancelled_by_the_conversation_owner() {
    let (barrier, entered, _release) = GenerateBarrier::new();
    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        USER_TEXT,
        ModelOutput::WaitForCancellation(barrier),
    );
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, USER_TEXT);
    entered
        .recv_timeout(WAIT_TIMEOUT)
        .expect("scripted model reached the in-flight dispatch barrier");

    cancel_run(&host, command.run_id);
    let receipt = wait_terminal_run(&host, command.run_id);
    assert_eq!(receipt.state, RunState::Cancelled);
    assert_eq!(receipt.issue, Some(AgentFailure::Cancelled));
    assert_eq!(receipt.output, None);
    let event = wait_terminal_event(&host, command.run_id);
    assert_eq!(event.state, RunState::Cancelled);
    assert!(!event.generated_reply);

    let snapshot = assert_script_clean(&recorder);
    assert_eq!(snapshot.generated.len(), 1);
}

#[test]
fn closing_and_reopening_profile_preserves_result_without_model_redispatch() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let recorder = model.recorder();
    let profile = IsolatedProfile::new();
    let host = profile.open(&model);
    assert_eq!(
        activate_vault(&host, VaultLifecycleCommand::Create),
        floe_app::VaultState::Ready
    );
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, USER_TEXT);
    let first_receipt = wait_terminal_run(&host, command.run_id);
    assert_eq!(first_receipt.state, RunState::Completed);
    assert_eq!(first_receipt.output.as_deref(), Some(REPLY));
    assert_eq!(
        wait_terminal_event(&host, command.run_id).state,
        RunState::Completed
    );
    let calls_before_close = assert_script_clean(&recorder);
    assert_eq!(calls_before_close.generated.len(), 1);
    host.shutdown().expect("close first App generation");
    drop(host);

    let reopened = profile.open(&model);
    assert_eq!(
        activate_vault(&reopened, VaultLifecycleCommand::Unlock),
        floe_app::VaultState::Ready
    );
    let resumed_session = support::with_ready(&reopened, |services, caller, owners| {
        let actor = caller.owner_actor();
        let scope = floe_app::host_scope(
            Uuid::new_v4(),
            floe_execution::Cancellation::new(),
            Duration::from_secs(15),
        );
        services
            .execute_owner(async move { owners.conversation.resume_session(&actor, &scope).await })
            .expect("resume persisted Conversation session")
    });
    assert_eq!(
        resumed_session.as_ref().map(|session| session.id),
        Some(session_id)
    );

    let reopened_receipt = read_run(
        &reopened,
        floe_kernel::RunId::from_uuid(command.run_id.as_uuid()).expect("valid run id"),
    )
    .expect("persisted run receipt after reopen");
    assert_eq!(reopened_receipt.state, RunState::Completed);
    assert_eq!(reopened_receipt.output.as_deref(), Some(REPLY));
    let transcript = read_session(&reopened, session_id);
    assert!(transcript.messages.iter().any(|message| matches!(
        message,
        SessionMessage::Assistant { text, .. } if text == REPLY
    )));
    let after_reopen = assert_script_clean(&recorder);
    assert_eq!(after_reopen.plans.len(), calls_before_close.plans.len());
    assert_eq!(
        after_reopen.generated.len(),
        calls_before_close.generated.len()
    );
    reopened.shutdown().expect("close reopened App generation");
}

#[test]
fn unsupported_manager_tool_call_is_rejected_without_execution() {
    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        USER_TEXT,
        ModelOutput::UnsupportedManagerToolCall,
    );
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, USER_TEXT);
    let receipt = wait_terminal_run(&host, command.run_id);

    assert_eq!(receipt.state, RunState::Failed);
    assert_eq!(receipt.issue, Some(AgentFailure::ServerModelInvalidOutput));
    assert_eq!(receipt.output, None);
    let transcript = read_session(&host, session_id);
    assert!(
        !transcript
            .messages
            .iter()
            .any(|message| matches!(message, SessionMessage::Capability { .. }))
    );
    assert_eq!(
        wait_terminal_event(&host, command.run_id).state,
        RunState::Failed
    );

    let snapshot = assert_script_clean(&recorder);
    assert_eq!(snapshot.generated.len(), 1);
    assert!(snapshot.generated[0].catalog.tools.is_empty());
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn schedule_expert_reads_only_the_selected_synthetic_calendar_and_persists_evidence() {
    const SCHEDULE_REQUEST: &str = "Review my calendar this week.";

    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        SCHEDULE_REQUEST,
        ModelOutput::ScheduleExpertFlow,
    );
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);

    let source = support::configure_fixture_calendar(&host, "Synthetic team calendar");
    assert!(source.revision > 0);
    assert_eq!(source.observe_state, floe_connections::ObserveState::Enabled);
    assert_eq!(
        source
            .selected_resources
            .iter()
            .map(|resource| resource.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Synthetic team calendar"]
    );
    support::bind_schedule_expert(&host);

    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, SCHEDULE_REQUEST);
    let receipt = wait_terminal_run(&host, command.run_id);
    assert_eq!(receipt.state, RunState::Completed);
    assert_eq!(receipt.output.as_deref(), Some(REPLY));
    assert_eq!(receipt.task_refs.len(), 1);

    let transcript = read_session(&host, session_id);
    assert!(transcript.messages.iter().any(|message| matches!(
        message,
        SessionMessage::User { text, .. } if text == SCHEDULE_REQUEST
    )));
    assert!(transcript.messages.iter().any(|message| matches!(
        message,
        SessionMessage::Assistant { text, .. } if text == REPLY
    )));
    let (task_id, execution_ref) = transcript
        .messages
        .iter()
        .find_map(|message| match message {
            SessionMessage::Delegation { task, .. }
                if task.agent_id == "floe.builtin.schedule"
                    && task.state == floe_agent_contract::TaskState::Completed =>
            {
                Some((task.task_id, task.execution_receipt.clone()?))
            }
            _ => None,
        })
        .expect("persisted completed Schedule Expert delegation");
    assert!(receipt.task_refs.contains(&task_id.as_uuid()));

    let (person_id, device_id) = support::with_ready(&host, |_, caller, _| {
        let actor = caller.owner_actor();
        (actor.person_id, actor.device_id)
    });
    let task_receipt = support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let scope = floe_app::host_scope(
            Uuid::new_v4(),
            floe_execution::Cancellation::new(),
            Duration::from_secs(15),
        );
        services
            .execute_owner(async move {
                owners
                    .experts
                    .read_task_execution_receipt(&actor, &execution_ref, &scope)
                    .await
            })
            .expect("read persisted Expert execution receipt")
    });
    assert_eq!(task_receipt.snapshot.task_id, task_id);
    assert_eq!(task_receipt.snapshot.state, floe_agent_contract::TaskState::Completed);
    assert!(!task_receipt.accounting.attempt_refs.is_empty());

    let task_evidence = serde_json::to_string(&task_receipt.snapshot.artifacts)
        .expect("serialize persisted Schedule Expert evidence");
    assert!(task_evidence.contains("Synthetic planning event"));
    assert!(!task_evidence.contains("Unselected calendar sentinel event"));
    let floe_agent_contract::DependencyCoverage::Dependent { dependencies } =
        &task_receipt.snapshot.coverage
    else {
        panic!("Schedule Expert result must retain source dependency evidence");
    };
    let fixture_dependency = dependencies
        .iter()
        .find(|dependency| dependency.source().connector().as_str() == "calendar.fixture")
        .expect("fixture Calendar dependency in task evidence");
    assert_eq!(fixture_dependency.person_id(), person_id);
    assert_eq!(
        fixture_dependency.source().connection_id().as_str(),
        "calendar.fixture.local"
    );
    assert_eq!(
        fixture_dependency.source().execution_owner().as_str(),
        format!("fixture:{device_id}")
    );
    assert_eq!(fixture_dependency.operation(), floe_context_contract::GrantOperation::Read);
    assert_eq!(fixture_dependency.purpose(), floe_context_contract::GrantPurpose::Assistant);
    assert_eq!(
        fixture_dependency.consumer().identifier(),
        "floe.builtin.schedule"
    );
    assert_eq!(
        fixture_dependency.processing(),
        &floe_context_contract::ProcessingRestriction::DeviceOnly
    );
    assert_eq!(
        fixture_dependency
            .source_resources()
            .iter()
            .map(|resource| resource.as_str())
            .collect::<Vec<_>>(),
        vec!["fixture.calendar.team"]
    );

    let scripted = assert_script_clean(&recorder);
    assert_eq!(scripted.plans.len(), 6);
    assert_eq!(scripted.plan_bindings.len(), 6);
    assert_eq!(scripted.generated.len(), 4);
    assert_eq!(scripted.generated_bindings.len(), 4);
    for binding in &scripted.generated_bindings {
        assert_eq!(binding.run_id, Some(command.run_id.as_uuid()));
        assert!(binding.attempt_id.is_some());
    }
    assert_eq!(
        scripted.generated_bindings[0].consumer,
        floe_conversation::CONVERSATION_CONSUMER
    );
    assert_eq!(
        scripted.generated_bindings[1].consumer,
        floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER
    );
    assert_eq!(
        scripted.generated_bindings[2].consumer,
        floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER
    );
    assert_eq!(
        scripted.generated_bindings[3].consumer,
        floe_conversation::CONVERSATION_CONSUMER
    );
    assert_eq!(scripted.generated_bindings[0].task_id, None);
    assert_eq!(
        scripted.generated_bindings[1].task_id,
        Some(task_id.as_uuid())
    );
    assert_eq!(
        scripted.generated_bindings[2].task_id,
        Some(task_id.as_uuid())
    );
    assert_eq!(scripted.generated_bindings[3].task_id, None);
    for binding in scripted.generated_bindings[..1]
        .iter()
        .chain(scripted.generated_bindings[3..].iter())
    {
        let attempt_id = binding.attempt_id.expect("attempt binding");
        assert!(receipt.attempt_refs.contains(&attempt_id));
    }
    for binding in &scripted.generated_bindings[1..3] {
        let attempt_id = binding.attempt_id.expect("attempt binding");
        assert!(task_receipt.accounting.attempt_refs.contains(&attempt_id));
    }
}
