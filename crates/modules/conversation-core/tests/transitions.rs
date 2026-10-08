use floe_conversation_contract::{
    AdmissionDisposition, AdmissionTarget, AgentIdentity, AgentInstanceId, AssignmentId,
    ConversationBranchId, ConversationId, ConversationMessage, MessageAdmissionRequest, MessageId,
    MessageOrigin, RunTaskLink,
};
use floe_conversation_core::ConversationCore;
use floe_kernel::{CommandId, PersonId, RunId, TaskId};
use uuid::Uuid;

fn uuid(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn identity(agent: u128, assignment: u128, definition_revision: u64) -> AgentIdentity {
    AgentIdentity {
        person_id: PersonId::from_uuid(uuid(10)).expect("person ID"),
        agent_instance_id: AgentInstanceId::from_uuid(uuid(agent)).expect("agent ID"),
        assignment_id: AssignmentId::from_uuid(uuid(assignment)).expect("assignment ID"),
        definition_id: format!("agent.{agent}"),
        definition_revision,
    }
}

fn message(id: u128, command: u128, text: &str, task: Option<TaskId>) -> ConversationMessage {
    ConversationMessage {
        message_id: MessageId::from_uuid(uuid(id)).expect("message ID"),
        command_id: CommandId::from_uuid(uuid(command)).expect("command ID"),
        origin: MessageOrigin::Person {
            person_id: PersonId::from_uuid(uuid(10)).expect("person ID"),
        },
        text: text.into(),
        task_id: task,
    }
}

fn agent_message(id: u128, command: u128, text: &str, sender: u128) -> ConversationMessage {
    let mut message = message(id, command, text, None);
    message.origin = MessageOrigin::Agent {
        agent_instance_id: AgentInstanceId::from_uuid(uuid(sender)).expect("sender ID"),
    };
    message
}

fn exercise_role_fixture(
    agent: u128,
    assignment: u128,
    conversation_id: u128,
    run_base: u128,
    initial_origin: MessageOrigin,
    task_id: Option<TaskId>,
) -> ConversationCore {
    let mut first = message(60, 70, "same assigned goal", task_id);
    first.origin = initial_origin.clone();
    let (mut conversation, first_admission) = ConversationCore::open(new_request(
        identity(agent, assignment, 1),
        conversation_id,
        first,
    ))
    .expect("new goal starts an isolated conversation");
    let first_run = RunId::from_uuid(uuid(run_base)).expect("first Run");
    let next_run = RunId::from_uuid(uuid(run_base + 1)).expect("next Run");
    let first_claim = conversation
        .claim_next_writer(RunTaskLink {
            run_id: first_run,
            task_id,
        })
        .expect("one active writer");
    assert_eq!(first_claim.message, first_admission.receipt.transcript);

    let mut follow_up = message(61, 71, "clarification for that goal", task_id);
    follow_up.origin = initial_origin;
    let queued = conversation
        .continue_with(MessageAdmissionRequest {
            target: AdmissionTarget::Continue {
                reference: conversation.reference(),
            },
            message: follow_up,
        })
        .expect("new Message ID queues as a distinct admission");
    assert_eq!(queued.disposition, AdmissionDisposition::Queued);
    assert_eq!(
        conversation.claim_next_writer(RunTaskLink {
            run_id: next_run,
            task_id,
        }),
        Err(floe_conversation_contract::ConversationFailure::WriterAlreadyActive)
    );

    conversation
        .complete_writer(first_run)
        .expect("first Run settled");
    let next_claim = conversation
        .claim_next_writer(RunTaskLink {
            run_id: next_run,
            task_id,
        })
        .expect("next queued message is claimed in order");
    assert_eq!(next_claim.message, queued.receipt.transcript);
    conversation
        .complete_writer(next_run)
        .expect("next Run settled");
    conversation
}

fn new_request(
    agent: AgentIdentity,
    conversation: u128,
    msg: ConversationMessage,
) -> MessageAdmissionRequest {
    MessageAdmissionRequest {
        target: AdmissionTarget::New {
            conversation_id: ConversationId::from_uuid(uuid(conversation))
                .expect("conversation ID"),
            branch_id: ConversationBranchId::from_uuid(uuid(conversation + 1)).expect("branch ID"),
            identity: agent,
        },
        message: msg,
    }
}

#[test]
fn agent_conversation_run_and_task_have_distinct_identity_and_lifetimes() {
    let task_id = TaskId::from_uuid(uuid(40)).expect("Task ID");
    let (mut conversation, first) = ConversationCore::open(new_request(
        identity(20, 30, 1),
        50,
        message(60, 70, "start a task", Some(task_id)),
    ))
    .expect("open a new conversation");
    let run_id = RunId::from_uuid(uuid(41)).expect("Run ID");
    let claim = conversation
        .claim_next_writer(RunTaskLink {
            run_id,
            task_id: Some(task_id),
        })
        .expect("claim first execution segment");

    assert_eq!(claim.link.run_id, run_id);
    assert_eq!(claim.link.task_id, Some(task_id));
    assert_ne!(claim.link.run_id.as_uuid(), task_id.as_uuid());
    assert_eq!(claim.message, first.receipt.transcript);
    assert_eq!(
        first.receipt.transcript.conversation_id,
        conversation.reference().conversation_id
    );
    assert_eq!(
        conversation
            .reference()
            .identity
            .agent_instance_id
            .as_uuid(),
        uuid(20)
    );

    let completed = conversation
        .complete_writer(run_id)
        .expect("settle the Run segment");
    assert_eq!(completed.task_id, Some(task_id));
    assert_eq!(conversation.completed_runs(), &[completed]);
}

#[test]
fn new_and_continuation_admission_are_explicit_and_revision_bound() {
    let (mut conversation, first) = ConversationCore::open(new_request(
        identity(20, 30, 1),
        50,
        message(60, 70, "first goal", None),
    ))
    .expect("new goal creates an isolated conversation");
    assert_eq!(first.disposition, AdmissionDisposition::Appended);

    let continuation = MessageAdmissionRequest {
        target: AdmissionTarget::Continue {
            reference: conversation.reference(),
        },
        message: message(61, 71, "clarification for that goal", None),
    };
    let second = conversation
        .continue_with(continuation)
        .expect("explicit continuation appends to the selected head");
    assert_eq!(second.receipt.head_revision, 2);

    let mut wrong_agent = conversation.reference();
    wrong_agent.identity = identity(21, 31, 1);
    let rejected = conversation.continue_with(MessageAdmissionRequest {
        target: AdmissionTarget::Continue {
            reference: wrong_agent,
        },
        message: message(62, 72, "cross-agent continuation", None),
    });
    assert_eq!(
        rejected,
        Err(floe_conversation_contract::ConversationFailure::AgentMismatch)
    );

    let new_again = conversation.continue_with(new_request(
        identity(20, 30, 1),
        80,
        message(63, 73, "new goal", None),
    ));
    assert_eq!(
        new_again,
        Err(floe_conversation_contract::ConversationFailure::ConversationMismatch)
    );
}

#[test]
fn changed_agent_definition_revision_defaults_to_an_isolated_conversation() {
    use floe_conversation_contract::{AdmissionTarget, ConversationReference};

    let old_identity = identity(20, 30, 1);
    let prior = ConversationReference {
        conversation_id: ConversationId::from_uuid(uuid(50)).expect("conversation ID"),
        branch_id: ConversationBranchId::from_uuid(uuid(51)).expect("branch ID"),
        identity: old_identity,
        head_revision: 1,
    };
    let target = AdmissionTarget::default_for_identity(
        identity(20, 30, 2),
        Some(&prior),
        ConversationId::from_uuid(uuid(80)).expect("new conversation ID"),
        ConversationBranchId::from_uuid(uuid(81)).expect("new branch ID"),
    )
    .expect("definition update defaults to a new conversation");

    assert_eq!(
        target,
        AdmissionTarget::New {
            conversation_id: ConversationId::from_uuid(uuid(80)).expect("new conversation ID"),
            branch_id: ConversationBranchId::from_uuid(uuid(81)).expect("new branch ID"),
            identity: identity(20, 30, 2),
        }
    );
}

#[test]
fn identical_message_replays_and_changed_body_conflicts_before_head_checks() {
    let (mut conversation, first) = ConversationCore::open(new_request(
        identity(20, 30, 1),
        50,
        message(60, 70, "same delivery", None),
    ))
    .expect("new conversation");
    let stale_reference = conversation.reference();

    let exact_replay = conversation
        .continue_with(MessageAdmissionRequest {
            target: AdmissionTarget::Continue {
                reference: stale_reference.clone(),
            },
            message: message(60, 70, "same delivery", None),
        })
        .expect("same Message and Command IDs replay the stored admission");
    assert_eq!(exact_replay.disposition, AdmissionDisposition::Replayed);
    assert_eq!(exact_replay.receipt, first.receipt);

    let replay = conversation
        .continue_with(MessageAdmissionRequest {
            target: AdmissionTarget::Continue {
                reference: stale_reference.clone(),
            },
            message: message(60, 71, "same delivery", None),
        })
        .expect("same Message ID and body return the stored admission");
    assert_eq!(replay.disposition, AdmissionDisposition::Replayed);
    assert_eq!(replay.receipt, first.receipt);

    let conflict = conversation.continue_with(MessageAdmissionRequest {
        target: AdmissionTarget::Continue {
            reference: stale_reference,
        },
        message: message(60, 72, "changed content", None),
    });
    assert_eq!(
        conflict,
        Err(floe_conversation_contract::ConversationFailure::MessageIdConflict)
    );
}

#[test]
fn different_agents_and_conversations_do_not_share_message_replay_or_provenance() {
    let (manager, manager_admission) = ConversationCore::open(new_request(
        identity(20, 30, 1),
        50,
        agent_message(60, 70, "same text, separate agent", 20),
    ))
    .expect("Manager conversation");
    let (expert, expert_admission) = ConversationCore::open(new_request(
        identity(21, 31, 1),
        80,
        agent_message(60, 70, "same text, separate agent", 21),
    ))
    .expect("Expert conversation");

    assert_ne!(
        manager.reference().conversation_id,
        expert.reference().conversation_id
    );
    assert_ne!(
        manager.reference().identity.agent_instance_id,
        expert.reference().identity.agent_instance_id
    );
    assert_ne!(
        manager_admission.receipt.transcript.conversation_id,
        expert_admission.receipt.transcript.conversation_id
    );
    assert_eq!(manager.transcript().count(), 1);
    assert_eq!(expert.transcript().count(), 1);
    let manager_origin = manager
        .transcript()
        .next()
        .expect("Manager message")
        .1
        .origin
        .clone();
    let expert_origin = expert
        .transcript()
        .next()
        .expect("Expert message")
        .1
        .origin
        .clone();
    assert_ne!(manager_origin, expert_origin);
}

#[test]
fn manager_and_expert_fixtures_share_core_transitions_but_keep_isolated_provenance() {
    let manager = exercise_role_fixture(
        20,
        30,
        50,
        110,
        MessageOrigin::Person {
            person_id: PersonId::from_uuid(uuid(10)).expect("person ID"),
        },
        None,
    );
    let task_id = TaskId::from_uuid(uuid(40)).expect("Expert host Task ID");
    let expert = exercise_role_fixture(
        21,
        31,
        80,
        120,
        MessageOrigin::Agent {
            agent_instance_id: AgentInstanceId::from_uuid(uuid(20)).expect("Manager instance"),
        },
        Some(task_id),
    );

    assert_ne!(
        manager.reference().conversation_id,
        expert.reference().conversation_id
    );
    assert_ne!(
        manager.reference().identity.agent_instance_id,
        expert.reference().identity.agent_instance_id
    );
    let manager_first = manager.transcript().next().expect("Manager transcript").1;
    let expert_first = expert.transcript().next().expect("Expert transcript").1;
    assert_eq!(manager_first.text, expert_first.text);
    assert_ne!(manager_first.origin, expert_first.origin);
    assert_eq!(expert.completed_runs().len(), 2);
    assert!(
        expert
            .completed_runs()
            .iter()
            .all(|run| run.task_id == Some(task_id))
    );
}

#[test]
fn active_writer_queues_follow_up_and_next_run_keeps_the_same_task_link() {
    let task_id = TaskId::from_uuid(uuid(40)).expect("Task ID");
    let (mut conversation, first) = ConversationCore::open(new_request(
        identity(20, 30, 1),
        50,
        message(60, 70, "first task message", Some(task_id)),
    ))
    .expect("open conversation");
    let run_one = RunId::from_uuid(uuid(41)).expect("Run ID");
    let run_two = RunId::from_uuid(uuid(42)).expect("second Run ID");
    let state_before_claim = conversation.state_revision();
    assert_eq!(
        conversation.claim_next_writer(RunTaskLink {
            run_id: run_one,
            task_id: Some(TaskId::from_uuid(uuid(99)).expect("wrong Task ID")),
        }),
        Err(floe_conversation_contract::ConversationFailure::TaskMismatch)
    );
    assert_eq!(conversation.state_revision(), state_before_claim);
    assert_eq!(conversation.pending_messages().count(), 1);

    let first_claim = conversation
        .claim_next_writer(RunTaskLink {
            run_id: run_one,
            task_id: Some(task_id),
        })
        .expect("claim the first message");
    assert_eq!(first_claim.message, first.receipt.transcript);

    let queued = conversation
        .continue_with(MessageAdmissionRequest {
            target: AdmissionTarget::Continue {
                reference: conversation.reference(),
            },
            message: message(61, 71, "next message while working", Some(task_id)),
        })
        .expect("durably queue the new message without mutating current input");
    assert_eq!(queued.disposition, AdmissionDisposition::Queued);
    assert_eq!(
        conversation.claim_next_writer(RunTaskLink {
            run_id: run_two,
            task_id: Some(task_id),
        }),
        Err(floe_conversation_contract::ConversationFailure::WriterAlreadyActive)
    );

    conversation
        .complete_writer(run_one)
        .expect("settle the active writer");
    let second_claim = conversation
        .claim_next_writer(RunTaskLink {
            run_id: run_two,
            task_id: Some(task_id),
        })
        .expect("claim the next message in FIFO order");
    assert_eq!(second_claim.message, queued.receipt.transcript);
    assert_eq!(second_claim.link.task_id, Some(task_id));
    assert_ne!(second_claim.link.run_id, first_claim.link.run_id);
}

#[test]
fn checkpoint_cannot_cross_conversation_or_transcript_prefix() {
    let (mut conversation, first) = ConversationCore::open(new_request(
        identity(20, 30, 1),
        50,
        message(60, 70, "checkpoint source", None),
    ))
    .expect("open conversation");
    let checkpoint = conversation
        .checkpoint_for(first.receipt.transcript, "bounded summary")
        .expect("build a checkpoint for the exact prefix");
    conversation
        .apply_checkpoint(checkpoint.clone())
        .expect("apply checkpoint to its source conversation");
    assert_eq!(conversation.checkpoint(), Some(&checkpoint));

    let (mut other, _) = ConversationCore::open(new_request(
        identity(21, 31, 1),
        80,
        message(90, 91, "other conversation", None),
    ))
    .expect("separate conversation");
    assert_eq!(
        other.apply_checkpoint(checkpoint),
        Err(floe_conversation_contract::ConversationFailure::CheckpointMismatch)
    );
}
