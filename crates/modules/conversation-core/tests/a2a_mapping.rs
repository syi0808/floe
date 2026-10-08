use floe_a2a::{
    A2A_EXCHANGE_CONTRACT_VERSION, A2aArtifact, A2aArtifactPart, A2aEnvelope, A2aInboundMapping,
    A2aMessage, A2aPeerAgentId, A2aPeerContextId, A2aPeerId, A2aPeerMessageId, A2aPeerTaskId,
    A2aProtocolPolicy, AuthenticatedPeerAgent, map_inbound_message,
};
use floe_conversation_contract::{
    AdmissionDisposition, AdmissionTarget, AgentIdentity, AgentInstanceId, AssignmentId,
    ConversationBranchId, ConversationFailure, ConversationId, MessageId,
};
use floe_conversation_core::ConversationCore;
use floe_kernel::{CommandId, PersonId, TaskId};
use uuid::{Uuid, uuid};

fn identity(agent: Uuid, assignment: Uuid) -> AgentIdentity {
    AgentIdentity {
        person_id: PersonId::from_uuid(uuid!("00000000-0000-4000-8000-000000000010"))
            .expect("Person ID"),
        agent_instance_id: AgentInstanceId::from_uuid(agent).expect("Agent instance ID"),
        assignment_id: AssignmentId::from_uuid(assignment).expect("Assignment ID"),
        definition_id: "agent.a2a-host".into(),
        definition_revision: 1,
    }
}

fn envelope() -> A2aEnvelope {
    A2aEnvelope {
        contract_version: A2A_EXCHANGE_CONTRACT_VERSION,
        extensions: vec!["urn:floe:test:context:v1".into()],
        message: A2aMessage {
            peer_id: A2aPeerId::try_new("peer-a").expect("peer ID"),
            sender_agent_id: A2aPeerAgentId::try_new("manager-installed-instance")
                .expect("remote agent ID"),
            context_id: A2aPeerContextId::try_new("remote-context-a").expect("remote context ID"),
            message_id: A2aPeerMessageId::try_new("remote-message-a").expect("remote message ID"),
            task_id: Some(A2aPeerTaskId::try_new("remote-task-a").expect("remote Task ID")),
            content: "안녕하세요,\n\t일정 조건을 검토해 주세요.\n두 번째 줄입니다.".into(),
            artifacts: vec![A2aArtifact {
                artifact_id: "artifact-korean".into(),
                name: "회의 요약".into(),
                parts: vec![
                    A2aArtifactPart::Text {
                        text: "첫 번째 줄\n\t들여쓴 내용\n마지막 줄".into(),
                    },
                    A2aArtifactPart::Data {
                        media_type: "application/json".into(),
                        data: "{\n\t\"제목\": \"일정 검토\",\n\t\"확인\": true\n}".into(),
                    },
                ],
            }],
        },
    }
}

fn mapping(target: AdmissionTarget) -> A2aInboundMapping {
    A2aInboundMapping {
        peer_id: A2aPeerId::try_new("peer-a").expect("peer ID"),
        remote_context_id: A2aPeerContextId::try_new("remote-context-a")
            .expect("remote context ID"),
        remote_message_id: A2aPeerMessageId::try_new("remote-message-a")
            .expect("remote message ID"),
        remote_task_id: Some(A2aPeerTaskId::try_new("remote-task-a").expect("remote Task ID")),
        host_agent_instance_id: AgentInstanceId::from_uuid(uuid!(
            "00000000-0000-4000-8000-000000000020"
        ))
        .expect("host agent instance"),
        local_target: target,
        local_message_id: MessageId::from_uuid(uuid!("00000000-0000-4000-8000-000000000040"))
            .expect("local message ID"),
        local_command_id: CommandId::from_uuid(uuid!("00000000-0000-4000-8000-000000000041"))
            .expect("local command ID"),
        local_task_id: Some(
            TaskId::from_uuid(uuid!("00000000-0000-4000-8000-000000000042")).expect("host Task ID"),
        ),
    }
}

fn verified_peer() -> AuthenticatedPeerAgent {
    AuthenticatedPeerAgent::from_verified_binding(
        A2aPeerId::try_new("peer-a").expect("peer ID"),
        A2aPeerAgentId::try_new("manager-installed-instance").expect("remote agent ID"),
        AgentInstanceId::from_uuid(uuid!("00000000-0000-4000-8000-000000000030"))
            .expect("verified source agent instance"),
    )
    .expect("host binding supplied verified peer identity")
}

fn policy() -> A2aProtocolPolicy {
    A2aProtocolPolicy::new(["urn:floe:test:context:v1".to_owned()]).expect("valid extension policy")
}

fn new_target() -> AdmissionTarget {
    AdmissionTarget::New {
        conversation_id: ConversationId::from_uuid(uuid!("00000000-0000-4000-8000-000000000050"))
            .expect("conversation ID"),
        branch_id: ConversationBranchId::from_uuid(uuid!("00000000-0000-4000-8000-000000000051"))
            .expect("branch ID"),
        identity: identity(
            uuid!("00000000-0000-4000-8000-000000000020"),
            uuid!("00000000-0000-4000-8000-000000000021"),
        ),
    }
}

#[test]
fn mapped_korean_multiline_message_and_json_artifacts_reach_conversation_core() {
    let request = envelope();
    let expected_text = request.message.content.clone();
    let expected_artifacts = request.message.artifacts.clone();
    let mapped = map_inbound_message(
        &request,
        &policy(),
        &verified_peer(),
        &mapping(new_target()),
    )
    .expect("map multiline message and artifacts");
    assert_eq!(mapped.artifacts, expected_artifacts);
    assert!(mapped.message.message.evidence.is_some());

    let (core, result) = ConversationCore::open(mapped.message).expect("admit mapped inbound work");
    assert_eq!(result.disposition, AdmissionDisposition::Appended);
    let (_, stored) = core.transcript().next().expect("stored transcript message");
    assert_eq!(stored.text, expected_text);
    assert!(stored.text.contains('\n') && stored.text.contains('\t'));
}

#[test]
fn identical_mapped_evidence_replays_and_changed_artifact_conflicts() {
    let request = envelope();
    let initial = map_inbound_message(
        &request,
        &policy(),
        &verified_peer(),
        &mapping(new_target()),
    )
    .expect("map original payload");
    let (mut core, admitted) =
        ConversationCore::open(initial.message.clone()).expect("admit original payload");
    let mut replay_mapping = mapping(AdmissionTarget::Continue {
        reference: core.reference(),
    });
    replay_mapping.local_message_id = initial.message.message.message_id;
    replay_mapping.local_command_id = initial.message.message.command_id;

    let identical = map_inbound_message(&request, &policy(), &verified_peer(), &replay_mapping)
        .expect("map identical remote payload");
    let replayed = core
        .continue_with(identical.message)
        .expect("identical content-addressed evidence replays");
    assert_eq!(replayed.disposition, AdmissionDisposition::Replayed);
    assert_eq!(replayed.receipt, admitted.receipt);

    let mut changed = request;
    let A2aArtifactPart::Text { text } = &mut changed.message.artifacts[0].parts[0] else {
        panic!("fixture starts with a text artifact");
    };
    text.push_str(" changed");
    let changed = map_inbound_message(&changed, &policy(), &verified_peer(), &replay_mapping)
        .expect("map changed evidence with the same peer Message ID and text");
    assert_eq!(
        core.continue_with(changed.message),
        Err(ConversationFailure::CommandIdConflict)
    );
}
