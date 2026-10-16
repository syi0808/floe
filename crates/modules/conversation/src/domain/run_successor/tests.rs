use super::*;
use floe_kernel::{CommandId, PersonId, RunId};
use uuid::Uuid;
fn completed() -> RunRecord {
    RunRecord {
        expert_environment: floe_experts::RunExpertEnvironmentIdentity {
            revision: 1,
            digest: [7; 32],
        },
        run_id: RunId::new(),
        command_id: CommandId::new(),
        session_id: Uuid::new_v4(),
        person_id: PersonId::new(),
        device_id: "synthetic-device".into(),
        user_message_id: Uuid::new_v4(),
        initial_session_revision: 0,
        request_digest: [9; 32],
        state: RunState::Completed,
        pending_terminal: None,
        output: Some("Synthetic result".into()),
        coverage: floe_agent_contract::DependencyCoverage::Independent,
        issue: None,
        blocked: None,
        session_revision: 2,
        aggregate_revision: 2,
        journal_revision: 1,
        executor_generation: 1,
        continuation_of: None,
        continuation_executor_generation: None,
        continuation_level: 0,
        retry_of: None,
        resume_of: None,
        resume_lineage: 0,
    }
}
fn continued(parent: &RunRecord) -> RunRecord {
    let mut child = parent.clone();
    child.run_id = RunId::new();
    child.command_id = CommandId::new();
    child.initial_session_revision = parent.session_revision;
    child.session_revision = parent.session_revision + 1;
    child.aggregate_revision = 1;
    child.journal_revision = 0;
    child.state = RunState::Working;
    child.issue = None;
    child.output = None;
    child.coverage = floe_agent_contract::DependencyCoverage::Unknown;
    child.continuation_of = Some(parent.run_id);
    child.continuation_executor_generation = Some(parent.executor_generation);
    child.continuation_level = parent.continuation_level + 1;
    child.resume_of = None;
    child
}
#[test]
fn continuing_a_resumed_run_retains_depth_in_record_and_receipt() {
    let mut parent = completed();
    parent.state = RunState::Failed;
    parent.issue = Some(AgentFailure::BudgetExceeded);
    parent.output = None;
    parent.coverage = floe_agent_contract::DependencyCoverage::Unknown;
    parent.resume_of = Some(RunId::new());
    parent.resume_lineage = 1;
    let child = continued(&parent);
    validate_run_successor(&parent, &child, RunSuccessorKind::Continue).unwrap();
    let receipt = crate::project_run_receipt(child.clone()).unwrap();
    assert_eq!(receipt.resume_lineage, 1);
    let mut reset = child.clone();
    reset.resume_lineage = 0;
    assert!(validate_run_successor(&parent, &reset, RunSuccessorKind::Continue).is_err());
    let mut changed = child;
    changed.user_message_id = Uuid::new_v4();
    assert!(validate_run_successor(&parent, &changed, RunSuccessorKind::Continue).is_err());
}
#[test]
fn continue_cannot_reset_the_existing_resume_limit() {
    let mut parent = completed();
    parent.state = RunState::TimedOut;
    parent.issue = Some(AgentFailure::DeadlineExceeded);
    parent.output = None;
    parent.coverage = floe_agent_contract::DependencyCoverage::Unknown;
    parent.resume_of = Some(RunId::new());
    parent.resume_lineage = crate::MAX_RESUME_LINEAGE;
    let mut child = continued(&parent);
    validate_run_successor(&parent, &child, RunSuccessorKind::Continue).unwrap();
    child.state = RunState::Completed;
    child.session_revision += 1;
    child.aggregate_revision = 2;
    child.output = Some("settled".into());
    child.coverage = floe_agent_contract::DependencyCoverage::Independent;
    let receipt = crate::project_run_receipt(child).unwrap();
    assert!(
        receipt.resume().is_none(),
        "continuation must not renew the resume budget"
    );
}
#[test]
fn resume_link_requires_original_identity_and_cumulative_depth() {
    let parent = completed();
    let mut child = continued(&parent);
    child.continuation_of = None;
    child.continuation_executor_generation = None;
    child.continuation_level = 0;
    child.resume_of = Some(parent.run_id);
    child.resume_lineage = 1;
    validate_run_successor(&parent, &child, RunSuccessorKind::Resume).unwrap();
    assert!(validate_run_successor(&parent, &child, RunSuccessorKind::Continue).is_err());
    child.resume_lineage = 2;
    assert!(validate_run_successor(&parent, &child, RunSuccessorKind::Resume).is_err());
    child.resume_lineage = 1;
    child.device_id = "another-device".into();
    assert!(validate_run_successor(&parent, &child, RunSuccessorKind::Resume).is_err());
}

#[test]
fn continuation_depth_is_part_of_intent_digest_and_exact_admission() {
    let mut parent = completed();
    parent.state = RunState::Failed;
    parent.issue = Some(AgentFailure::BudgetExceeded);
    parent.output = None;
    parent.coverage = floe_agent_contract::DependencyCoverage::Unknown;
    parent.resume_of = Some(RunId::new());
    parent.resume_lineage = 1;
    let reference = crate::project_run_receipt(parent.clone())
        .unwrap()
        .continuation()
        .unwrap();
    assert_eq!(reference.resume_lineage, 1);
    let mut intent = crate::CanonicalTurnIntent {
        session_id: parent.session_id,
        expected_revision: parent.session_revision,
        text: "Synthetic original input".into(),
        mode: crate::TurnMode::Continue(reference.clone()),
        retry_of: None,
    };
    let principal = parent.person_id.to_string();
    let digest = intent.digest(&principal).unwrap();
    let mut child = continued(&parent);
    child.request_digest = digest;
    let mut admission = crate::TurnAdmissionRequest {
        expert_environment: child.expert_environment.clone(),
        run_id: child.run_id,
        command_id: child.command_id,
        session_id: child.session_id,
        expected_session_revision: child.initial_session_revision,
        principal,
        device_id: child.device_id.clone(),
        request_digest: digest,
        mode: intent.mode.clone(),
        retry_of: None,
        input: crate::TurnInput::ExistingMessage {
            message_id: child.user_message_id,
        },
    };
    assert!(child.matches_admission(&admission));
    let mut reset = reference;
    reset.resume_lineage = 0;
    intent.mode = crate::TurnMode::Continue(reset);
    assert_ne!(digest, intent.digest(&admission.principal).unwrap());
    admission.mode = intent.mode;
    assert!(
        !child.matches_admission(&admission),
        "same stored digest must not hide a changed lineage reference"
    );
}
