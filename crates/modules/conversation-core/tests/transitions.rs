use floe_conversation_contract::{
    AdmissionDisposition, AdmissionTarget, AgentIdentity, AgentInstanceId, AssignmentId,
    ConversationBranchId, ConversationFailure, ConversationId, ConversationMessage,
    LogicalContributionId, MessageAdmissionRequest, MessageEvidenceReference, MessageId,
    MessageOrigin, TaskEvidenceReference,
};
use floe_conversation_core::{
    ConversationCore, ExecutorDomain, OwnerGenerationFence, OwnerRunEvidence, OwnerRunState,
    OwnerSettlementEvidence, RecorderFence, RecorderRecoveryState, RecorderStartRequest,
    RecordingRequest,
};
use floe_kernel::{CommandId, PersonId, RunId, TaskId};
use uuid::Uuid;

struct Fixture {
    core: ConversationCore,
    identity: AgentIdentity,
    conversation_id: ConversationId,
    branch_id: ConversationBranchId,
}

impl Fixture {
    fn new() -> Self {
        let identity = AgentIdentity {
            person_id: PersonId::new(),
            agent_instance_id: AgentInstanceId::new(),
            assignment_id: AssignmentId::new(),
            definition_id: "fixture-agent".into(),
            definition_revision: 1,
        };
        let conversation_id = ConversationId::new();
        let branch_id = ConversationBranchId::new();
        let core = ConversationCore::new(identity.clone(), conversation_id, branch_id).unwrap();
        Self {
            core,
            identity,
            conversation_id,
            branch_id,
        }
    }

    fn append_input(&mut self, text: &str) -> floe_conversation_contract::TranscriptReference {
        let result = self
            .core
            .append_input(MessageAdmissionRequest {
                target: AdmissionTarget::New {
                    conversation_id: self.conversation_id,
                    branch_id: self.branch_id,
                    identity: self.identity.clone(),
                },
                message: message(
                    MessageOrigin::Person {
                        person_id: self.identity.person_id,
                    },
                    text,
                    None,
                ),
            })
            .unwrap();
        result.receipt.transcript
    }

    fn start(
        &self,
        run_id: RunId,
        input: floe_conversation_contract::TranscriptReference,
        generation: u64,
    ) -> RecorderStartRequest {
        RecorderStartRequest {
            identity: self.identity.clone(),
            conversation_id: self.conversation_id,
            branch_id: self.branch_id,
            run_id,
            input,
            executor_domain: ExecutorDomain::HostRun,
            executor_generation: generation,
            execution_task: None,
        }
    }

    fn open(
        &mut self,
        run_id: RunId,
        input: floe_conversation_contract::TranscriptReference,
        generation: u64,
    ) -> floe_conversation_core::RecorderOpenReceipt {
        let request = self.start(run_id, input, generation);
        let owner = owner(&request, OwnerRunState::Working, vec![]);
        self.core.open_recording(request, Some(owner)).unwrap()
    }
}

fn message(origin: MessageOrigin, text: &str, task_id: Option<TaskId>) -> ConversationMessage {
    ConversationMessage {
        message_id: MessageId::new(),
        command_id: CommandId::new(),
        origin,
        text: text.into(),
        evidence: None,
        task_id,
    }
}

fn owner(
    request: &RecorderStartRequest,
    state: OwnerRunState,
    unresolved_effects: Vec<Uuid>,
) -> OwnerRunEvidence {
    OwnerRunEvidence {
        domain: request.executor_domain,
        run_id: request.run_id,
        person_id: request.identity.person_id,
        input: request.input,
        executor_generation: request.executor_generation,
        aggregate_revision: 3,
        journal_revision: 2,
        state,
        record_digest: [7; 32],
        unresolved_effects,
    }
}

fn close_owner(
    fence: &RecorderFence,
    state: OwnerRunState,
    unresolved: Vec<Uuid>,
) -> OwnerSettlementEvidence {
    OwnerSettlementEvidence {
        owner: OwnerRunEvidence {
            domain: fence.executor_domain,
            run_id: fence.run_id,
            person_id: fence.identity.person_id,
            input: fence.input,
            executor_generation: fence.executor_generation,
            aggregate_revision: 4,
            journal_revision: 3,
            state,
            record_digest: [8; 32],
            unresolved_effects: unresolved,
        },
        terminal_receipt_digest: [9; 32],
        // This slice has no owner field that proves transcript settlement.
        settled_through: None,
        protection_roots: vec![],
    }
}

fn task_receipt(task_id: TaskId) -> TaskEvidenceReference {
    TaskEvidenceReference::from_digest(task_id, [11; 32])
}

fn output_request(
    fence: RecorderFence,
    task_id: Option<TaskId>,
    receipt: Option<TaskEvidenceReference>,
    contribution: LogicalContributionId,
) -> RecordingRequest {
    RecordingRequest {
        recorder: fence.clone(),
        message: message(
            MessageOrigin::Agent {
                agent_instance_id: fence.identity.agent_instance_id,
            },
            "generated output",
            task_id,
        ),
        contribution_id: contribution,
        producing_task: receipt,
    }
}

#[test]
fn repeated_retained_input_opens_two_fresh_runs_without_appending_again() {
    let mut fixture = Fixture::new();
    let input = fixture.append_input("retained input");
    let first = fixture.open(RunId::new(), input, 1);
    let first_close = fixture
        .core
        .close_recording(
            first.fence.clone(),
            Some(close_owner(&first.fence, OwnerRunState::Terminal, vec![])),
            false,
        )
        .unwrap();
    assert_eq!(first_close.settled_prefix, 0);

    let second = fixture.open(RunId::new(), input, 1);
    assert_eq!(second.fence.input, first.fence.input);
    assert!(second.fence.run_id != first.fence.run_id);
    assert_eq!(second.fence.recorder_epoch, first.fence.recorder_epoch + 1);
    assert_eq!(
        fixture.core.head().head_revision,
        1,
        "Continue/Resume reuses the input reference"
    );
}

#[test]
fn lost_open_output_and_close_acknowledgements_replay_exact_receipts() {
    let mut fixture = Fixture::new();
    let input = fixture.append_input("input");
    let run = RunId::new();
    let start = fixture.start(run, input, 1);
    let open = fixture
        .core
        .open_recording(
            start.clone(),
            Some(owner(&start, OwnerRunState::Working, vec![])),
        )
        .unwrap();
    let replay_open = fixture.core.open_recording(start.clone(), None).unwrap();
    assert_eq!(replay_open, open);

    let contribution = LogicalContributionId::new();
    let output = output_request(open.fence.clone(), None, None, contribution);
    let output_receipt = fixture
        .core
        .record_entry(
            output.clone(),
            None,
            Some(owner(&start, OwnerRunState::Working, vec![])),
        )
        .unwrap();
    let close = fixture
        .core
        .close_recording(
            open.fence.clone(),
            Some(close_owner(&open.fence, OwnerRunState::Terminal, vec![])),
            false,
        )
        .unwrap();
    let replay_output = fixture.core.record_entry(output, None, None).unwrap();
    let replay_close = fixture
        .core
        .close_recording(open.fence, None, false)
        .unwrap();
    assert_eq!(replay_output, output_receipt);
    assert_eq!(replay_close, close);
    assert_eq!(fixture.core.head().head_revision, 2);
}

#[test]
fn changed_recorder_fences_and_claim_conflicts_fail_closed() {
    let mut fixture = Fixture::new();
    let input = fixture.append_input("input");
    let run = RunId::new();
    let start = fixture.start(run, input, 1);
    let open = fixture
        .core
        .open_recording(
            start.clone(),
            Some(owner(&start, OwnerRunState::Working, vec![])),
        )
        .unwrap();

    let mut changed_identity = start.clone();
    changed_identity.identity.assignment_id = AssignmentId::new();
    assert_eq!(
        fixture.core.open_recording(changed_identity, None),
        Err(ConversationFailure::RunAlreadyUsed)
    );

    let mut changed_domain = start.clone();
    changed_domain.executor_domain = ExecutorDomain::TaskExecution;
    changed_domain.execution_task = Some(TaskId::new());
    assert_eq!(
        fixture.core.open_recording(changed_domain, None),
        Err(ConversationFailure::RunAlreadyUsed)
    );

    let mut changed_generation = start.clone();
    changed_generation.executor_generation += 1;
    assert_eq!(
        fixture.core.open_recording(changed_generation, None),
        Err(ConversationFailure::RunAlreadyUsed)
    );

    let other = fixture.start(RunId::new(), input, 1);
    assert_eq!(
        fixture.core.open_recording(
            other.clone(),
            Some(owner(&other, OwnerRunState::Working, vec![]))
        ),
        Err(ConversationFailure::WriterAlreadyActive)
    );

    let mut changed_output_fence =
        output_request(open.fence.clone(), None, None, LogicalContributionId::new());
    let first_receipt = fixture
        .core
        .record_entry(
            changed_output_fence.clone(),
            None,
            Some(owner(&start, OwnerRunState::Working, vec![])),
        )
        .unwrap();
    changed_output_fence.recorder.executor_generation += 1;
    assert_eq!(
        fixture.core.record_entry(changed_output_fence, None, None),
        Err(ConversationFailure::MessageIdConflict)
    );
    assert_eq!(first_receipt.recorder, open.fence);
}

#[test]
fn crashed_recorder_retires_only_after_owner_generation_fence_then_reopens() {
    let mut fixture = Fixture::new();
    let input = fixture.append_input("input");
    let old_start = fixture.start(RunId::new(), input, 1);
    let old = fixture
        .core
        .open_recording(
            old_start.clone(),
            Some(owner(&old_start, OwnerRunState::Working, vec![])),
        )
        .unwrap();
    assert_eq!(
        fixture.core.observe_recorder(old.fence.run_id, 1).state,
        RecorderRecoveryState::ActiveCurrentGeneration
    );

    let generation_fence = OwnerGenerationFence {
        domain: ExecutorDomain::HostRun,
        old_generation: 1,
        current_generation: 2,
        fence_revision: 2,
        evidence_digest: [12; 32],
    };
    let recovery_owner = owner(&old_start, OwnerRunState::PendingTerminal, vec![]);
    let retired = fixture
        .core
        .retire_stale_recording(
            old.fence.clone(),
            generation_fence.clone(),
            Some(recovery_owner.clone()),
        )
        .unwrap();
    let replay = fixture
        .core
        .retire_stale_recording(
            old.fence.clone(),
            OwnerGenerationFence {
                domain: ExecutorDomain::HostRun,
                old_generation: 1,
                current_generation: 9,
                fence_revision: 99,
                evidence_digest: [99; 32],
            },
            None,
        )
        .unwrap();
    assert_eq!(retired, replay);

    let new_start = fixture.start(RunId::new(), input, 2);
    let new = fixture
        .core
        .open_recording(
            new_start.clone(),
            Some(owner(&new_start, OwnerRunState::Working, vec![])),
        )
        .unwrap();
    assert!(new.fence.recorder_epoch > old.fence.recorder_epoch);
    assert_eq!(
        fixture.core.observe_recorder(old.fence.run_id, 2).state,
        RecorderRecoveryState::Retired
    );
    assert_eq!(fixture.core.active_recorder(), Some(&new.fence));
}

#[test]
fn unresolved_owner_effects_reject_close_and_stale_retirement() {
    let mut fixture = Fixture::new();
    let input = fixture.append_input("input");
    let start = fixture.start(RunId::new(), input, 1);
    let open = fixture
        .core
        .open_recording(
            start.clone(),
            Some(owner(&start, OwnerRunState::Working, vec![])),
        )
        .unwrap();
    assert_eq!(
        fixture.core.close_recording(
            open.fence.clone(),
            Some(close_owner(
                &open.fence,
                OwnerRunState::Terminal,
                vec![Uuid::new_v4()]
            )),
            false,
        ),
        Err(ConversationFailure::UnresolvedEffect),
    );
    assert_eq!(
        fixture.core.retire_stale_recording(
            open.fence.clone(),
            OwnerGenerationFence {
                domain: ExecutorDomain::HostRun,
                old_generation: 1,
                current_generation: 2,
                fence_revision: 2,
                evidence_digest: [13; 32]
            },
            Some(owner(
                &start,
                OwnerRunState::PendingTerminal,
                vec![Uuid::new_v4()]
            )),
        ),
        Err(ConversationFailure::UnresolvedEffect),
    );
}

#[test]
fn one_manager_recorder_can_record_two_owner_proven_task_contributions() {
    let mut fixture = Fixture::new();
    let input = fixture.append_input("manager request");
    let run = RunId::new();
    let start = fixture.start(run, input, 1);
    let open = fixture
        .core
        .open_recording(
            start.clone(),
            Some(owner(&start, OwnerRunState::Working, vec![])),
        )
        .unwrap();
    let task_a = TaskId::new();
    let task_b = TaskId::new();
    let receipt_a = task_receipt(task_a);
    let receipt_b = task_receipt(task_b);
    let request_a = output_request(
        open.fence.clone(),
        Some(task_a),
        Some(receipt_a.clone()),
        LogicalContributionId::new(),
    );
    let request_b = output_request(
        open.fence.clone(),
        Some(task_b),
        Some(receipt_b.clone()),
        LogicalContributionId::new(),
    );
    let recorded_a = fixture
        .core
        .record_entry(
            request_a.clone(),
            Some(receipt_a.clone()),
            Some(owner(&start, OwnerRunState::Working, vec![])),
        )
        .unwrap();
    let recorded_b = fixture
        .core
        .record_entry(
            request_b,
            Some(receipt_b.clone()),
            Some(owner(&start, OwnerRunState::Working, vec![])),
        )
        .unwrap();
    assert_eq!(recorded_a.producing_task, Some(receipt_a));
    assert_eq!(recorded_b.producing_task, Some(receipt_b));
    assert_ne!(recorded_a.transcript, recorded_b.transcript);
    assert_eq!(
        fixture
            .core
            .record_entry(request_a.clone(), None, None)
            .unwrap(),
        recorded_a,
        "exact replay returns its original owner receipt without re-resolving Task state",
    );

    let changed_task = TaskId::new();
    let changed_receipt = task_receipt(changed_task);
    let mut conflicting = request_a;
    conflicting.message.task_id = Some(changed_task);
    conflicting.producing_task = Some(changed_receipt.clone());
    assert_eq!(
        fixture
            .core
            .record_entry(conflicting, Some(changed_receipt), None),
        Err(ConversationFailure::MessageIdConflict)
    );
}

#[test]
fn stale_append_retry_uses_the_original_receipt_and_structured_empty_text_survives() {
    let mut fixture = Fixture::new();
    let target = AdmissionTarget::New {
        conversation_id: fixture.conversation_id,
        branch_id: fixture.branch_id,
        identity: fixture.identity.clone(),
    };
    let mut request = MessageAdmissionRequest {
        target,
        message: message(
            MessageOrigin::Person {
                person_id: fixture.identity.person_id,
            },
            "",
            None,
        ),
    };
    request.message.evidence = Some(MessageEvidenceReference::from_digest([21; 32]));
    let appended = fixture.core.append_input(request.clone()).unwrap();
    assert_eq!(appended.disposition, AdmissionDisposition::Appended);
    let mut different_command = request.clone();
    different_command.message.command_id = CommandId::new();
    let replay = fixture.core.append_input(different_command).unwrap();
    assert_eq!(replay.disposition, AdmissionDisposition::Replayed);
    assert_eq!(replay.receipt, appended.receipt);

    let mut changed_body = request;
    changed_body.message.evidence = Some(MessageEvidenceReference::from_digest([22; 32]));
    assert_eq!(
        fixture.core.append_input(changed_body),
        Err(ConversationFailure::MessageIdConflict)
    );
}
