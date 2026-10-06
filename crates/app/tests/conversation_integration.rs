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
