use floe_conversation_contract::{
    AdmissionTarget, AgentIdentity, AgentInstanceId, AssignmentId, ConversationBranchId,
    ConversationId, ConversationMessage, MessageAdmissionRequest, MessageId, MessageOrigin,
    TranscriptReference,
};
use floe_conversation_core::{
    ConversationCore, ConversationReadTarget, ConversationStoreFailure, TranscriptEntryLookup,
    TranscriptPageBudget, TranscriptReadCursor,
};
use floe_kernel::{CommandId, PersonId};

fn message(identity: &AgentIdentity, text: &str) -> ConversationMessage {
    ConversationMessage {
        message_id: MessageId::new(),
        command_id: CommandId::new(),
        origin: MessageOrigin::Person {
            person_id: identity.person_id,
        },
        text: text.to_owned(),
        evidence: None,
        task_id: None,
    }
}

fn append_input(
    core: &mut ConversationCore,
    identity: &AgentIdentity,
    conversation_id: ConversationId,
    branch_id: ConversationBranchId,
    text: &str,
) -> TranscriptReference {
    let target = if core.head().head_revision == 0 {
        AdmissionTarget::New {
            identity: identity.clone(),
            conversation_id,
            branch_id,
        }
    } else {
        AdmissionTarget::AppendToExisting {
            reference: core.reference(),
        }
    };
    core.append_input(MessageAdmissionRequest {
        target,
        message: message(identity, text),
    })
    .expect("append Core input")
    .receipt
    .transcript
}

#[test]
fn pure_core_read_boundary_lookup_and_reverse_page_pin_exact_transcript_order() {
    let identity = AgentIdentity {
        person_id: PersonId::new(),
        agent_instance_id: AgentInstanceId::new(),
        assignment_id: AssignmentId::new(),
        definition_id: "test.read-api".into(),
        definition_revision: 1,
    };
    let conversation_id = ConversationId::new();
    let branch_id = ConversationBranchId::new();
    let mut core =
        ConversationCore::new(identity.clone(), conversation_id, branch_id).expect("create Core");
    let target = ConversationReadTarget {
        identity: identity.clone(),
        conversation_id,
        branch_id,
    };

    let empty_boundary = core.read_boundary().expect("capture empty boundary");
    let empty_page = core
        .read_previous_page(
            &TranscriptReadCursor::start(empty_boundary.clone()),
            TranscriptPageBudget {
                max_entries: 4,
                max_bytes: 1024,
            },
        )
        .expect("read empty Core transcript");
    assert!(empty_page.entries.is_empty());
    assert!(!empty_page.has_more);
    assert!(matches!(
        core.read_transcript_entry(
            &empty_boundary,
            TranscriptEntryLookup::MessageId(MessageId::new()),
            1024,
        ),
        Err(ConversationStoreFailure::TranscriptMessageNotFound)
    ));

    let first = append_input(
        &mut core,
        &identity,
        conversation_id,
        branch_id,
        "oldest λ🙂",
    );
    let second = append_input(&mut core, &identity, conversation_id, branch_id, "middle");
    let third = append_input(&mut core, &identity, conversation_id, branch_id, "newest");
    assert!(
        core.read_previous_page(
            &TranscriptReadCursor::start(empty_boundary),
            TranscriptPageBudget {
                max_entries: 4,
                max_bytes: 1024,
            },
        )
        .expect("empty read remains pinned after append")
        .entries
        .is_empty()
    );

    let boundary = core.read_boundary().expect("capture current boundary");
    assert_eq!(boundary.target, target);
    assert_eq!(boundary.through, Some(third));
    let exact = core
        .read_transcript_entry(
            &boundary,
            TranscriptEntryLookup::Reference(second),
            usize::MAX,
        )
        .expect("lookup exact MessageId and sequence");
    assert_eq!(exact.reference, second);
    assert!(
        serde_json::to_vec(&exact)
            .expect("encode record envelope")
            .len()
            > exact.message.text.chars().count()
    );
    assert!(matches!(
        core.read_transcript_entry(
            &boundary,
            TranscriptEntryLookup::Reference(TranscriptReference {
                message_id: MessageId::new(),
                ..second
            }),
            usize::MAX,
        ),
        Err(ConversationStoreFailure::TranscriptReferenceMismatch)
    ));

    let newer = core
        .read_previous_page(
            &TranscriptReadCursor::start(boundary.clone()),
            TranscriptPageBudget {
                max_entries: 2,
                max_bytes: usize::MAX,
            },
        )
        .expect("read latest reverse page");
    assert_eq!(
        newer
            .entries
            .iter()
            .map(|entry| entry.reference)
            .collect::<Vec<_>>(),
        [second, third]
    );
    assert!(newer.has_more);
    let older = core
        .read_previous_page(
            &newer.next_cursor.clone().expect("older entries remain"),
            TranscriptPageBudget {
                max_entries: 2,
                max_bytes: usize::MAX,
            },
        )
        .expect("read earlier page");
    assert_eq!(older.entries[0].reference, first);
    assert!(!older.has_more);

    append_input(
        &mut core,
        &identity,
        conversation_id,
        branch_id,
        "after snapshot",
    );
    let pinned_last = core
        .read_previous_page(
            &newer.next_cursor.expect("continue at pinned boundary"),
            TranscriptPageBudget {
                max_entries: 4,
                max_bytes: usize::MAX,
            },
        )
        .expect("read continuation after append");
    assert_eq!(pinned_last.entries[0].reference, first);
    assert!(!pinned_last.has_more);
}
